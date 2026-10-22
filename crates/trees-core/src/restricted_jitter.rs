//! Stateless, position-hashed sampling on a world-origin grid.
//!
//! Each cubic cell contributes one point, displaced from its center by at most
//! `jitter * cell_size` on each axis. In exact arithmetic the minimum separation
//! is `cell_size * (1 - 2 * jitter)`. Returned `Vec3` coordinates are rounded to
//! f32, so this bound is subject to coordinate precision; use local coordinates
//! and a jitter comfortably below 0.5 when spacing is important.

use glam::Vec3;
use std::{error::Error, fmt};

// Bounds integer conversion and keeps cell coordinates within f32's useful range.
const CELL_LIMIT: f64 = 1_048_576.0;

/// Invalid sampler configuration or query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestrictedJitterError {
    /// Cell size must be finite and strictly positive.
    InvalidCellSize,
    /// Jitter must be finite and in `[0, 0.5)`.
    InvalidJitter,
    /// Bounds must be finite, with minimum no greater than maximum on each axis.
    InvalidBounds,
    /// Bounds divided by cell size must lie within `[-2^20, 2^20]`.
    CoordinateOutOfRange,
}

impl fmt::Display for RestrictedJitterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidCellSize => "cell size must be finite and positive",
            Self::InvalidJitter => "jitter must be finite and in [0, 0.5)",
            Self::InvalidBounds => "bounds must be finite and ordered",
            Self::CoordinateOutOfRange => "bounds exceed the supported cell coordinate range",
        })
    }
}

impl Error for RestrictedJitterError {}

/// A repeatable grid with one independently jittered point per sampling cell.
///
/// Sampling depends only on seed and signed cell coordinates, never query order.
///
/// ```
/// use glam::Vec3;
/// use trees_core::RestrictedJitter;
///
/// let grid = RestrictedJitter::new(0.5, 0.25, 42)?;
/// let points: Vec<Vec3> = grid.points_in_aabb(Vec3::ZERO, Vec3::ONE)?.collect();
/// assert_eq!(points.len(), 8);
/// # Ok::<(), trees_core::RestrictedJitterError>(())
/// ```
#[derive(Clone, Copy, Debug)]
pub struct RestrictedJitter {
    cell_size: f32,
    jitter: f32,
    seed: u64,
}

impl RestrictedJitter {
    /// Construct a grid anchored at world origin.
    pub fn new(cell_size: f32, jitter: f32, seed: u64) -> Result<Self, RestrictedJitterError> {
        if !cell_size.is_finite() || cell_size <= 0.0 {
            return Err(RestrictedJitterError::InvalidCellSize);
        }
        if !jitter.is_finite() || !(0.0..0.5).contains(&jitter) {
            return Err(RestrictedJitterError::InvalidJitter);
        }
        Ok(Self {
            cell_size,
            jitter,
            seed,
        })
    }

    /// Lazily return points inside the half-open box `[min, max)`.
    ///
    /// Equal bounds on any axis produce an empty iterator. Invalid bounds or
    /// bounds outside ±2^20 cells return an error. Order is stable, with x
    /// changing fastest, then y, then z. No points or grid cells are cached.
    /// Work scales with the number of intersected cells, so callers should use
    /// reasonably sized boxes; laziness avoids allocating the entire result.
    pub fn points_in_aabb(
        &self,
        min: Vec3,
        max: Vec3,
    ) -> Result<JitterPoints, RestrictedJitterError> {
        if !min.is_finite() || !max.is_finite() || min.cmpgt(max).any() {
            return Err(RestrictedJitterError::InvalidBounds);
        }
        let lo = min.as_dvec3() / f64::from(self.cell_size);
        let hi = max.as_dvec3() / f64::from(self.cell_size);
        if lo.abs().max_element() > CELL_LIMIT || hi.abs().max_element() > CELL_LIMIT {
            return Err(RestrictedJitterError::CoordinateOutOfRange);
        }
        // A one-cell halo also catches points rounded onto a cell boundary by
        // f32 conversion. Filtering always uses the actual returned position.
        let start = lo.floor().as_i64vec3().to_array().map(|v| v - 1);
        let end = hi.ceil().as_i64vec3().to_array().map(|v| v + 1);
        Ok(JitterPoints {
            grid: *self,
            min,
            max,
            start,
            end,
            cursor: start,
            done: min.cmpeq(max).any(),
        })
    }

    fn point(&self, cell: [i64; 3]) -> Vec3 {
        let mut key = mix(self.seed ^ 0xA076_1D64_78BD_642F);
        for coordinate in cell {
            key = mix(key ^ coordinate as u64);
        }
        let mut position = [0.0; 3];
        for axis in 0..3 {
            key = mix(key.wrapping_add(0x9E37_79B9_7F4A_7C15));
            // Exactly representable 53-bit fraction in [0, 1).
            let uniform = (key >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0);
            position[axis] =
                ((cell[axis] as f64 + 0.5 + f64::from(self.jitter) * (2.0 * uniform - 1.0))
                    * f64::from(self.cell_size)) as f32;
        }
        Vec3::from_array(position)
    }
}

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

/// Lazy iterator returned by [`RestrictedJitter::points_in_aabb`].
#[derive(Clone, Debug)]
pub struct JitterPoints {
    grid: RestrictedJitter,
    min: Vec3,
    max: Vec3,
    start: [i64; 3],
    end: [i64; 3],
    cursor: [i64; 3],
    done: bool,
}

impl Iterator for JitterPoints {
    type Item = Vec3;

    fn next(&mut self) -> Option<Vec3> {
        while !self.done {
            let point = self.grid.point(self.cursor);
            for axis in 0..3 {
                self.cursor[axis] += 1;
                if self.cursor[axis] < self.end[axis] {
                    break;
                }
                self.cursor[axis] = self.start[axis];
                if axis == 2 {
                    self.done = true;
                }
            }
            if point.cmpge(self.min).all() && point.cmplt(self.max).all() {
                return Some(point);
            }
        }
        None
    }
}

impl std::iter::FusedIterator for JitterPoints {}

#[cfg(test)]
mod tests {
    use super::*;

    fn points(grid: &RestrictedJitter, min: Vec3, max: Vec3) -> Vec<Vec3> {
        grid.points_in_aabb(min, max).unwrap().collect()
    }

    #[test]
    fn zero_jitter_centers_and_half_open_bounds() {
        let grid = RestrictedJitter::new(2.0, 0.0, 42).unwrap();
        assert_eq!(
            points(&grid, Vec3::splat(-1.0), Vec3::splat(1.0)),
            vec![Vec3::splat(-1.0)]
        );
        let result = points(&grid, Vec3::ZERO, Vec3::splat(4.0));
        assert_eq!(result.len(), 8);
        assert_eq!(result[0], Vec3::ONE);
        assert_eq!(result[1], Vec3::new(3.0, 1.0, 1.0));
        assert_eq!(result[7], Vec3::splat(3.0));
    }

    #[test]
    fn repeatable_across_queries_seeds_and_partitions() {
        let grid = RestrictedJitter::new(1.0, 0.25, 42).unwrap();
        let min = Vec3::splat(-3.0);
        let max = Vec3::splat(3.0);
        let whole = points(&grid, min, max);
        assert_eq!(whole.len(), 216);
        assert_eq!(whole, points(&grid, min, max));
        assert_ne!(
            whole,
            points(&RestrictedJitter::new(1.0, 0.25, 43).unwrap(), min, max)
        );
        let split = 0.37;
        let left = points(&grid, min, Vec3::new(split, 3.0, 3.0));
        let right = points(&grid, Vec3::new(split, -3.0, -3.0), max);
        assert_eq!(
            left,
            whole
                .iter()
                .copied()
                .filter(|p| p.x < split)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            right,
            whole
                .iter()
                .copied()
                .filter(|p| p.x >= split)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn jitter_limits_and_spacing_across_cell_boundaries() {
        let grid = RestrictedJitter::new(1.0, 0.25, 123).unwrap();
        let result = points(&grid, Vec3::splat(-3.0), Vec3::splat(3.0));
        for (i, p) in result.iter().enumerate() {
            let offset = *p - p.floor();
            assert!(offset.cmpge(Vec3::splat(0.25)).all());
            assert!(offset.cmple(Vec3::splat(0.75)).all());
            for q in &result[i + 1..] {
                assert!(p.distance(*q) >= 0.5);
            }
        }
    }

    #[test]
    fn validates_inputs_and_handles_empty_boxes() {
        for size in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                RestrictedJitter::new(size, 0.25, 0).unwrap_err(),
                RestrictedJitterError::InvalidCellSize
            );
        }
        for jitter in [-0.1, 0.5, f32::NAN, f32::INFINITY] {
            assert_eq!(
                RestrictedJitter::new(1.0, jitter, 0).unwrap_err(),
                RestrictedJitterError::InvalidJitter
            );
        }
        let grid = RestrictedJitter::new(1.0, 0.25, 0).unwrap();
        assert_eq!(points(&grid, Vec3::ZERO, Vec3::new(0.0, 2.0, 3.0)).len(), 0);
        for (min, max) in [
            (Vec3::ONE, Vec3::ZERO),
            (Vec3::NAN, Vec3::ONE),
            (Vec3::ZERO, Vec3::INFINITY),
        ] {
            assert_eq!(
                grid.points_in_aabb(min, max).unwrap_err(),
                RestrictedJitterError::InvalidBounds
            );
        }
        assert_eq!(
            grid.points_in_aabb(Vec3::ZERO, Vec3::splat(2_000_000.0))
                .unwrap_err(),
            RestrictedJitterError::CoordinateOutOfRange
        );
    }

    #[test]
    fn small_queries_match_filtering_a_larger_query() {
        let grid = RestrictedJitter::new(0.3, 0.49, 9).unwrap();
        let min = Vec3::new(-0.42, 0.13, -0.22);
        let max = Vec3::new(0.76, 0.81, 0.47);
        let expected: Vec<_> = points(&grid, Vec3::splat(-1.0), Vec3::ONE)
            .into_iter()
            .filter(|p| p.cmpge(min).all() && p.cmplt(max).all())
            .collect();
        assert_eq!(points(&grid, min, max), expected);
    }
}

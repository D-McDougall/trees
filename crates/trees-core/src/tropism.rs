//! Pure Longay-style direction steering, independent of sensing and tree mutation.

use glam::Vec3;

use crate::{NodeId, SlotIndex, Tropism};

// On a unit heading, horizontal lengths <= 1e-6 have an unstable azimuth.
// This angular singularity threshold is never used to discard a small blend.
const VERTICAL_EPSILON: f32 = 1e-6;

/// Local target at `inclination` radians from +Y, in the heading's vertical plane.
///
/// The heading must be finite and nonzero and inclination must be in [0, π].
/// The horizontal projection selects the nearer of the plane's two targets.
/// Within 1e-6 of vertical (measured on the unit heading), a stateless hash of
/// `seed`, `node`, and `slot` selects an azimuth, independently of call order.
/// Exactly up/down targets are returned directly to avoid trigonometric residue.
pub fn gravitropic_target(
    heading: Vec3,
    inclination: f32,
    seed: u64,
    node: NodeId,
    slot: SlotIndex,
) -> Vec3 {
    if inclination == 0.0 {
        return Vec3::Y;
    }
    if inclination == std::f32::consts::PI {
        return -Vec3::Y;
    }
    let heading = unit_or_zero(heading);
    let horizontal = Vec3::new(heading.x, 0.0, heading.z);
    let radial = if horizontal.length_squared() > VERTICAL_EPSILON * VERTICAL_EPSILON {
        horizontal.normalize()
    } else {
        let slot_key = match slot {
            SlotIndex::Main => 0,
            SlotIndex::Lateral(i) => u64::from(i) + 1,
        };
        let bits = mix(mix(seed) ^ mix(node.index() as u64) ^ mix(slot_key + 0x100));
        let azimuth = ((bits >> 40) as f32 / 16_777_216.0) * std::f32::consts::TAU;
        let (sin, cos) = azimuth.sin_cos();
        Vec3::new(cos, 0.0, sin)
    };
    let (sin, cos) = inclination.sin_cos();
    (cos * Vec3::Y + sin * radial).normalize()
}

/// Blend unit heading, local target, and environment directions using `weights`.
///
/// Inputs are normalized independently; zero-weight inputs are ignored entirely.
/// Weights must be finite, nonnegative, and sum to one (as validated by species).
/// A zero blend returns zero without substituting a heading. There is no
/// cancellation epsilon: every representable nonzero blend is normalized,
/// including near cancellation. Scaling before normalization avoids underflow.
/// This function does not decide whether a bud is eligible to grow.
pub fn blend_directions(heading: Vec3, target: Vec3, environment: Vec3, weights: &Tropism) -> Vec3 {
    let influence = |direction: Vec3, weight: f32| {
        if weight == 0.0 {
            Vec3::ZERO
        } else {
            unit_or_zero(direction) * weight
        }
    };
    unit_or_zero(
        influence(heading, weights.straightness)
            + influence(target, weights.gravitropism)
            + influence(environment, weights.environment),
    )
}

fn unit_or_zero(direction: Vec3) -> Vec3 {
    let scale = direction.abs().max_element();
    if scale == 0.0 {
        Vec3::ZERO
    } else {
        (direction / scale).normalize()
    }
}

fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    fn close(a: Vec3, b: Vec3) {
        assert!((a - b).length() < 1e-6, "{a:?} != {b:?}");
    }

    fn weights(a: f32, b: f32, c: f32) -> Tropism {
        Tropism {
            main_set_point: 0.0,
            lateral_set_point: 0.0,
            straightness: a,
            gravitropism: b,
            environment: c,
        }
    }

    #[test]
    fn target_inclination_azimuth_and_nearest_solution() {
        let radial = Vec3::new(-3.0, 0.0, 4.0).normalize();
        for heading in [radial + Vec3::Y, radial - Vec3::Y] {
            for angle in [0.0, PI / 6.0, FRAC_PI_2, 2.0 * PI / 3.0, PI] {
                let target = gravitropic_target(heading, angle, 42, NodeId(0), SlotIndex::Main);
                close(target, angle.cos() * Vec3::Y + angle.sin() * radial);
                assert!(
                    target.dot(heading)
                        >= (angle.cos() * Vec3::Y - angle.sin() * radial).dot(heading) - 1e-6
                );
            }
        }
    }

    #[test]
    fn vertical_symmetry_breaking_is_finite_stable_and_keyed() {
        let target =
            |seed, node, slot, heading| gravitropic_target(heading, FRAC_PI_2, seed, node, slot);
        let expected = target(42, NodeId(7), SlotIndex::Main, Vec3::Y);
        for heading in [Vec3::Y, -Vec3::Y, Vec3::new(1e-8, 1.0, -1e-8)] {
            assert_eq!(target(42, NodeId(7), SlotIndex::Main, heading), expected);
            assert!(expected.is_finite());
            close(expected.length() * Vec3::Y, Vec3::Y);
        }
        for actual in [
            target(43, NodeId(7), SlotIndex::Main, Vec3::Y),
            target(42, NodeId(8), SlotIndex::Main, Vec3::Y),
            target(42, NodeId(7), SlotIndex::Lateral(0), Vec3::Y),
            target(42, NodeId(7), SlotIndex::Lateral(1), Vec3::Y),
        ] {
            assert_ne!(actual, expected);
        }
        assert_eq!(target(42, NodeId(7), SlotIndex::Main, Vec3::Y), expected);
        close(
            gravitropic_target(
                Vec3::new(2e-6, 1.0, 0.0),
                FRAC_PI_2,
                42,
                NodeId(7),
                SlotIndex::Main,
            ),
            Vec3::X,
        );
    }

    #[test]
    fn weight_corners_and_normalized_mixture() {
        for (w, expected) in [
            (weights(1.0, 0.0, 0.0), Vec3::X),
            (weights(0.0, 1.0, 0.0), Vec3::Y),
            (weights(0.0, 0.0, 1.0), Vec3::Z),
        ] {
            close(
                blend_directions(2.0 * Vec3::X, 3.0 * Vec3::Y, 4.0 * Vec3::Z, &w),
                expected,
            );
        }
        close(
            blend_directions(Vec3::X, Vec3::Y, Vec3::Z, &weights(0.2, 0.3, 0.5)),
            Vec3::new(0.2, 0.3, 0.5).normalize(),
        );
    }

    #[test]
    fn cancellation_is_zero_but_small_nonzero_blends_are_normalized() {
        assert_eq!(
            blend_directions(Vec3::X, -Vec3::X, Vec3::Z, &weights(0.5, 0.5, 0.0)),
            Vec3::ZERO
        );
        close(
            blend_directions(
                Vec3::X,
                -Vec3::X,
                Vec3::Z,
                &weights(0.50000006, 0.49999994, 0.0),
            ),
            Vec3::X,
        );
        close(unit_or_zero(Vec3::X * f32::MIN_POSITIVE), Vec3::X);
    }

    #[test]
    fn zero_weight_inputs_are_ignored() {
        close(
            blend_directions(Vec3::X, Vec3::NAN, Vec3::ZERO, &weights(1.0, 0.0, 0.0)),
            Vec3::X,
        );
    }
}

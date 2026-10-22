//! Longay (2014), Appendix C.1.2: an inextensible framed petiole rod.
//!
//! Initial attachments stay in [`Leaf`]. This isolated solver integrates from
//! bark to blade and returns points and frames without mutating the tree.
//! See `docs/foliage.md` for equations, units and numerical conventions.

use std::fmt;

use glam::{DQuat, DVec3, Vec3};

use crate::{Foliage, Leaf, PetioleResponse, SpeciesError};

/// Numerical resolution, separate from species-specific biological parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PetioleOptions {
    /// Equal arc-length segments, in 1..=4096. Default: 32.
    /// Strong responses or long rods may require higher resolution; this is
    /// not an adaptive solver and does not promise convergence at every setting.
    pub segments: u32,
}

impl Default for PetioleOptions {
    fn default() -> Self {
        Self { segments: 32 }
    }
}

/// Position and right-handed orthonormal frame at one arc-length sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PetioleSample {
    /// World-space position; the last sample is the blade's bottom-center base.
    pub position: Vec3,
    /// Unit tangent H, also the blade's base-to-tip direction at the endpoint.
    pub heading: Vec3,
    /// Unit blade-left vector L.
    pub left: Vec3,
    /// Unit blade normal U = H × L.
    pub normal: Vec3,
}

/// Solved rod samples, including both attachment and blade endpoint.
#[derive(Clone, Debug, PartialEq)]
pub struct Petiole {
    samples: Vec<PetioleSample>,
}

impl Petiole {
    /// Proximal-to-distal positions and frames. A zero-length rod has one sample;
    /// otherwise there are `options.segments + 1`. The vector is never empty.
    pub fn samples(&self) -> &[PetioleSample] {
        &self.samples
    }

    /// Final frame and bottom-center blade base, ready for geometry generation.
    pub fn tip(&self) -> &PetioleSample {
        self.samples
            .last()
            .expect("a petiole always has its attachment")
    }
}

/// Invalid solver input or a solution outside the public f32 coordinate range.
#[derive(Clone, Debug, PartialEq)]
pub enum PetioleError {
    /// Invalid species-specific target or susceptibility.
    Response(SpeciesError),
    /// Petiole length must be finite and non-negative.
    Length(f32),
    /// Resolution must be in 1..=4096.
    Segments(u32),
    /// Attachment must be finite; initial H/L/U must form a unit right-handed
    /// frame within 1e-4 tolerance. Leaf fields are public, so check at this API.
    InitialFrame,
    /// Computed output cannot be represented as finite f32 samples.
    NonFiniteSolution,
}

impl fmt::Display for PetioleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Response(error) => write!(f, "invalid petiole response: {error}"),
            Self::Length(length) => write!(f, "invalid petiole length: {length}"),
            Self::Segments(n) => write!(f, "petiole segments must be in 1..=4096, got {n}"),
            Self::InitialFrame => write!(f, "invalid petiole attachment or initial frame"),
            Self::NonFiniteSolution => write!(f, "petiole solution exceeds finite f32 coordinates"),
        }
    }
}

impl std::error::Error for PetioleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Response(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct Frame {
    h: DVec3,
    l: DVec3,
    u: DVec3,
}

impl Frame {
    fn rotated(self, rotation: DVec3) -> Self {
        let q = DQuat::from_scaled_axis(rotation);
        Self {
            h: q * self.h,
            l: q * self.l,
            u: q * self.u,
        }
    }

    fn sample(self, position: DVec3) -> Result<PetioleSample, PetioleError> {
        let sample = PetioleSample {
            position: position.as_vec3(),
            heading: self.h.as_vec3(),
            left: self.l.as_vec3(),
            normal: self.u.as_vec3(),
        };
        if [sample.position, sample.heading, sample.left, sample.normal]
            .iter()
            .all(|v| v.is_finite())
        {
            Ok(sample)
        } else {
            Err(PetioleError::NonFiniteSolution)
        }
    }
}

// Longay C.1.2, p.144. T is a unit preferred blade-normal direction.
fn angular_velocity(frame: Frame, target: DVec3, response: &PetioleResponse) -> DVec3 {
    let twist = -f64::from(response.twist) * frame.l.dot(target);
    let bend = f64::from(response.bend) * frame.h.dot(target);
    let turn = f64::from(response.turn) * frame.l.dot(target.cross(frame.h.cross(target)));
    twist * frame.h + bend * frame.l + turn * frame.u
}

/// Integrates Longay's three rotation rates along the petiole from a stored leaf.
///
/// Uses an explicit midpoint step and quaternion frame rotations in f64, then
/// emits f32 samples. Each chord has length `petiole_length / segments` along
/// its midpoint tangent. Refining the resolution approximates the smooth rod.
/// Target magnitude is normalized away. No randomness, scene lighting or tree
/// edits occur. Only response and petiole length are read from `foliage`.
/// Invalid inputs are rejected even for zero-length rods.
///
/// ```
/// use std::sync::Arc;
/// use glam::Vec3;
/// use trees_core::{Tree, TreeSpecies, PetioleOptions, solve_petiole};
///
/// let tree = Tree::new(Arc::new(TreeSpecies::default()), 42, Vec3::ZERO)?;
/// let leaf = &tree.leaves()[0];
/// let rod = solve_petiole(leaf, &tree.species().foliage, PetioleOptions::default())?;
/// assert_eq!(rod.samples()[0].position, leaf.attachment);
/// // rod.tip().position is the final blade base, with its final H/L/U frame.
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn solve_petiole(
    leaf: &Leaf,
    foliage: &Foliage,
    options: PetioleOptions,
) -> Result<Petiole, PetioleError> {
    foliage
        .petiole_response
        .validate()
        .map_err(PetioleError::Response)?;
    let length = foliage.petiole_length;
    if !length.is_finite() || length < 0.0 {
        return Err(PetioleError::Length(length));
    }
    if !(1..=4096).contains(&options.segments) {
        return Err(PetioleError::Segments(options.segments));
    }
    let vectors = [leaf.heading, leaf.left, leaf.normal];
    if !leaf.attachment.is_finite()
        || vectors
            .iter()
            .any(|v| !v.is_finite() || (v.length_squared() - 1.0).abs() > 1e-4)
        || leaf.heading.dot(leaf.left).abs() > 1e-4
        || leaf.heading.cross(leaf.left).distance(leaf.normal) > 1e-4
    {
        return Err(PetioleError::InitialFrame);
    }
    // Orthonormalize within the accepted input tolerance, so a slightly rounded
    // input frame cannot introduce artificial stretching or accumulating shear.
    let h = leaf.heading.as_dvec3().normalize();
    let left = leaf.left.as_dvec3();
    let l = (left - h * h.dot(left)).normalize();
    let mut frame = Frame {
        h,
        l,
        u: h.cross(l),
    };
    let mut position = leaf.attachment.as_dvec3();
    let mut samples = Vec::with_capacity(if length == 0.0 {
        1
    } else {
        options.segments as usize + 1
    });
    samples.push(frame.sample(position)?);
    if length == 0.0 {
        return Ok(Petiole { samples });
    }
    let ds = f64::from(length) / f64::from(options.segments);
    let target = foliage.petiole_response.target.as_dvec3().normalize();
    for _ in 0..options.segments {
        let midpoint =
            frame.rotated(angular_velocity(frame, target, &foliage.petiole_response) * (0.5 * ds));
        position += midpoint.h * ds;
        frame = frame.rotated(angular_velocity(midpoint, target, &foliage.petiole_response) * ds);
        samples.push(frame.sample(position)?);
    }
    Ok(Petiole { samples })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LeafId, Tree, TreeSpecies};
    use std::sync::Arc;

    fn leaf() -> Leaf {
        let tree = Tree::new(Arc::new(TreeSpecies::default()), 0, Vec3::ZERO).unwrap();
        Leaf {
            id: LeafId {
                axis: tree.root(),
                station: 0,
                side: 0,
            },
            node: tree.root(),
            distance: 0.0,
            attachment: Vec3::ZERO,
            heading: Vec3::X,
            left: Vec3::Y,
            normal: Vec3::Z,
        }
    }

    fn foliage(response: PetioleResponse) -> Foliage {
        Foliage {
            petiole_length: 1.0,
            petiole_response: response,
            ..Foliage::default()
        }
    }

    fn response(target: Vec3, twist: f32, bend: f32, turn: f32) -> PetioleResponse {
        PetioleResponse {
            target,
            twist,
            bend,
            turn,
        }
    }

    fn solve(leaf: &Leaf, f: &Foliage, segments: u32) -> Petiole {
        solve_petiole(leaf, f, PetioleOptions { segments }).unwrap()
    }

    fn assert_frame(sample: &PetioleSample) {
        for vector in [sample.heading, sample.left, sample.normal] {
            assert!(vector.is_finite());
            assert!((vector.length() - 1.0).abs() < 2e-6);
        }
        assert!(sample.heading.dot(sample.left).abs() < 2e-6);
        assert!(sample.heading.cross(sample.left).distance(sample.normal) < 2e-6);
        assert!(sample.position.is_finite());
    }

    #[test]
    fn torque_components_match_longay_equations_including_turn_sign() {
        let frame = Frame {
            h: DVec3::X,
            l: DVec3::Y,
            u: DVec3::Z,
        };
        let t = (DVec3::X + DVec3::Y).normalize();
        let actual = angular_velocity(frame, t, &response(t.as_vec3(), 2.0, 3.0, 4.0));
        let expected = DVec3::new(-2.0f64.sqrt(), 3.0 / 2.0f64.sqrt(), -2.0);
        assert!(
            actual.distance(expected) < 1e-12,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn zero_response_is_straight_at_every_resolution() {
        let leaf = leaf();
        let f = foliage(response(Vec3::Y, 0.0, 0.0, 0.0));
        for n in [1, 8, 32, 256] {
            let rod = solve(&leaf, &f, n);
            assert_eq!(rod.samples().len(), n as usize + 1);
            for (i, sample) in rod.samples().iter().enumerate() {
                assert!((sample.position - Vec3::X * (i as f32 / n as f32)).length() < 1e-6);
                assert_eq!(sample.heading, leaf.heading);
                assert_eq!(sample.left, leaf.left);
                assert_eq!(sample.normal, leaf.normal);
            }
        }
    }

    #[test]
    fn each_response_axis_has_the_prescribed_effect() {
        let leaf = leaf();
        let twist = solve(&leaf, &foliage(response(Vec3::Y, 1.0, 0.0, 0.0)), 64);
        assert_eq!(twist.tip().heading, Vec3::X);
        assert!(twist.tip().position.distance(Vec3::X) < 1e-6);
        assert!(twist.tip().normal.y > 0.5);
        let bend = solve(&leaf, &foliage(response(Vec3::X, 0.0, 1.0, 0.0)), 64);
        assert_eq!(bend.tip().left, Vec3::Y);
        assert!(bend.tip().normal.x > 0.5);
        assert!(bend.tip().position.z < -0.2);
        let turn = solve(
            &leaf,
            &foliage(response((Vec3::X + Vec3::Y).normalize(), 0.0, 0.0, 1.0)),
            64,
        );
        assert_eq!(turn.tip().normal, Vec3::Z);
        assert!(turn.tip().heading.y < -0.2);
    }

    #[test]
    fn single_axis_solutions_match_closed_form_reference() {
        // For e=1 and unit length, dθ/ds = cos θ with θ(0)=0:
        // sin θ=tanh(s), cos θ=sech(s). These are independent analytic
        // references for both the final frame and the integrated centerline.
        let tangent_component = 1.0f32 / 1.0f32.cosh();
        let target_component = 1.0f32.tanh();
        let twist = solve(&leaf(), &foliage(response(Vec3::Y, 1.0, 0.0, 0.0)), 256);
        assert!(
            twist
                .tip()
                .normal
                .distance(Vec3::new(0.0, target_component, tangent_component))
                < 1e-5
        );
        let bend = solve(&leaf(), &foliage(response(Vec3::X, 0.0, 1.0, 0.0)), 256);
        assert!(
            bend.tip()
                .normal
                .distance(Vec3::new(target_component, 0.0, tangent_component))
                < 1e-5
        );
        let expected_endpoint = Vec3::new(1.0f32.sinh().atan(), 0.0, -1.0f32.cosh().ln());
        assert!(bend.tip().position.distance(expected_endpoint) < 1e-5);
    }

    #[test]
    fn rod_is_inextensible_and_frames_stay_orthonormal() {
        let f = foliage(response(Vec3::new(0.3, 0.8, 0.5), 5.0, 7.0, 3.0));
        let rod = solve(&leaf(), &f, 128);
        assert_eq!(rod.samples()[0].position, Vec3::ZERO);
        let mut length = 0.0;
        for pair in rod.samples().windows(2) {
            let chord = pair[0].position.distance(pair[1].position);
            assert!((chord - 1.0 / 128.0).abs() < 1e-6);
            length += chord;
        }
        assert!((length - f.petiole_length).abs() < 2e-6);
        for sample in rod.samples() {
            assert_frame(sample);
        }
    }

    #[test]
    fn midpoint_solver_converges_with_resolution() {
        let f = foliage(response(Vec3::new(0.3, 0.8, 0.5), 5.0, 7.0, 3.0));
        let reference = solve(&leaf(), &f, 2048);
        let error = |n| {
            let rod = solve(&leaf(), &f, n);
            rod.tip().position.distance(reference.tip().position)
                + rod.tip().heading.distance(reference.tip().heading)
                + rod.tip().normal.distance(reference.tip().normal)
        };
        let (coarse, medium, fine) = (error(8), error(16), error(32));
        assert!(medium < coarse * 0.4, "{coarse}, {medium}, {fine}");
        assert!(fine < medium * 0.4, "{coarse}, {medium}, {fine}");
    }

    #[test]
    fn zero_length_has_one_sample_and_no_degenerate_segments() {
        let f = Foliage {
            petiole_length: 0.0,
            ..Foliage::default()
        };
        let leaf = leaf();
        let rod = solve(&leaf, &f, 32);
        assert_eq!(rod.samples().len(), 1);
        assert_eq!(rod.tip().position, leaf.attachment);
        assert_eq!(rod.tip().heading, leaf.heading);
        assert_eq!(rod.tip().normal, leaf.normal);
    }

    #[test]
    fn parallel_antiparallel_and_near_parallel_targets_are_finite() {
        for target in [
            Vec3::Z,
            -Vec3::Z,
            Vec3::X,
            -Vec3::X,
            Vec3::Y,
            Vec3::new(1e-7, 0.0, 1.0),
        ] {
            let rod = solve(&leaf(), &foliage(response(target, 2.0, 2.0, 2.0)), 64);
            for sample in rod.samples() {
                assert_frame(sample);
            }
        }
        // These equilibria follow directly from Longay's torques: do not
        // invent an arbitrary flip for an exactly opposite normal.
        for target in [Vec3::Z, -Vec3::Z] {
            let rod = solve(&leaf(), &foliage(response(target, 2.0, 2.0, 2.0)), 64);
            assert_eq!(rod.tip().normal, Vec3::Z);
            assert_eq!(rod.tip().position, Vec3::X);
        }
    }

    #[test]
    fn target_magnitude_is_irrelevant_and_rebuilds_are_deterministic() {
        let f = foliage(response(Vec3::Y, 2.0, 3.0, 4.0));
        let rod = solve(&leaf(), &f, 64);
        assert_eq!(rod, solve(&leaf(), &f, 64));
        let mut scaled = f;
        scaled.petiole_response.target *= 8.0;
        assert_eq!(rod, solve(&leaf(), &scaled, 64));
    }

    #[test]
    fn rejects_invalid_inputs_instead_of_emitting_bad_geometry() {
        let leaf = leaf();
        let mut f = Foliage::default();
        for n in [0, 4097, u32::MAX] {
            assert!(matches!(
                solve_petiole(&leaf, &f, PetioleOptions { segments: n }),
                Err(PetioleError::Segments(_))
            ));
        }
        for length in [-1.0, f32::NAN, f32::INFINITY] {
            f.petiole_length = length;
            assert!(matches!(
                solve_petiole(&leaf, &f, PetioleOptions::default()),
                Err(PetioleError::Length(_))
            ));
        }
        f = Foliage::default();
        f.petiole_response.target = Vec3::ZERO;
        assert!(matches!(
            solve_petiole(&leaf, &f, PetioleOptions::default()),
            Err(PetioleError::Response(_))
        ));
        f = Foliage::default();
        for case in 0..4 {
            let mut bad = leaf.clone();
            match case {
                0 => bad.normal = -Vec3::Z,
                1 => bad.heading *= 2.0,
                2 => bad.left = bad.heading,
                _ => bad.attachment.x = f32::NAN,
            }
            assert!(matches!(
                solve_petiole(&bad, &f, PetioleOptions::default()),
                Err(PetioleError::InitialFrame)
            ));
        }
        let mut overflowing = leaf;
        overflowing.attachment.x = f32::MAX;
        f.petiole_length = f32::MAX;
        f.petiole_response = response(Vec3::Y, 0.0, 0.0, 0.0);
        assert_eq!(
            solve_petiole(&overflowing, &f, PetioleOptions::default()),
            Err(PetioleError::NonFiniteSolution)
        );
    }

    #[test]
    fn real_tree_attachments_are_accepted_and_response_does_not_move_anchors() {
        let species = TreeSpecies::default();
        let tree = Tree::new(Arc::new(species.clone()), 42, Vec3::ZERO).unwrap();
        let mut other = species.clone();
        other.foliage.petiole_response = response(Vec3::X, 4.0, 1.0, 3.0);
        let changed = Tree::new(Arc::new(other), 42, Vec3::ZERO).unwrap();
        assert_eq!(tree.leaves(), changed.leaves());
        for (a, b) in tree.leaves().iter().zip(changed.leaves()) {
            let rod = solve(a, &species.foliage, 32);
            let changed_rod = solve(b, &changed.species().foliage, 32);
            assert_eq!(rod.samples()[0].position, a.attachment);
            assert_eq!(changed_rod.samples()[0].position, a.attachment);
            assert_ne!(rod.tip().normal, changed_rod.tip().normal);
        }
    }
}

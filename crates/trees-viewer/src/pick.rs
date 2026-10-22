//! Picking a branch under the pointer: a ray test against every internode,
//! each taken as a capsule around its axis with the larger of the radii of
//! the tube rings drawn at its two ends. The tube
//! mesh is that cylinder with flat joints, so the rounded caps only matter
//! where branches meet, and there either answer is fine.

use glam::Vec3;
use trees_core::{NodeId, Tree};
use trees_mesh::base_ring_radius;

/// Smallest radius a branch is picked with, in world units, so that twigs
/// a few pixels wide can still be clicked.
const MIN_PICK_RADIUS: f32 = 0.06;

/// The internode whose capsule `direction` (unit length) from `origin` hits
/// first, if any. Stumps can be picked like any other node.
pub fn pick(tree: &Tree, origin: Vec3, direction: Vec3) -> Option<NodeId> {
    tree.iter()
        .filter_map(|(id, node)| {
            let radius = node
                .radius
                .max(base_ring_radius(tree, id))
                .max(MIN_PICK_RADIUS);
            capsule_hit(origin, direction, node.base(), node.position, radius).map(|t| (t, id))
        })
        .min_by(|(a, _), (b, _)| a.total_cmp(b))
        .map(|(_, id)| id)
}

/// Distance along the ray to where it enters the capsule of `radius` around
/// the segment from `a` to `b`, or `None` if it misses or the capsule is
/// behind the origin. `direction` must be unit length. This is the usual
/// closed form: a quadratic for the cylinder, then a sphere at whichever
/// end the cylinder hit falls beyond.
fn capsule_hit(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3, radius: f32) -> Option<f32> {
    let axis = b - a;
    let oa = origin - a;
    let (axis2, axis_d, axis_oa) = (axis.dot(axis), axis.dot(direction), axis.dot(oa));
    // Quadratic in t for the infinite cylinder, scaled by |axis|².
    let qa = axis2 - axis_d * axis_d;
    let qb = axis2 * direction.dot(oa) - axis_oa * axis_d;
    let qc = axis2 * oa.dot(oa) - axis_oa * axis_oa - radius * radius * axis2;
    let disc = qb * qb - qa * qc;
    if disc < 0.0 {
        return None;
    }
    if qa > f32::EPSILON * axis2 {
        let t = (-qb - disc.sqrt()) / qa;
        let along = axis_oa + t * axis_d;
        if (0.0..=axis2).contains(&along) {
            return (t >= 0.0).then_some(t);
        }
    }
    // The hit is beyond one end (or the ray runs along the axis): try the
    // end cap nearer the ray's start.
    [a, b]
        .into_iter()
        .filter_map(|center| {
            let oc = origin - center;
            let half_b = direction.dot(oc);
            let disc = half_b * half_b - (oc.dot(oc) - radius * radius);
            let t = -half_b - disc.sqrt();
            (disc >= 0.0 && t >= 0.0).then_some(t)
        })
        .min_by(f32::total_cmp)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use trees_core::{OpenAir, SlotIndex, TreeSpecies};

    use super::*;

    #[test]
    fn capsule_hits_body_caps_and_misses() {
        let (a, b) = (Vec3::ZERO, Vec3::Y * 2.0);
        // Side on, through the middle: enters at the surface.
        let t = capsule_hit(Vec3::new(-5.0, 1.0, 0.0), Vec3::X, a, b, 0.5).unwrap();
        assert!((t - 4.5).abs() < 1e-5);
        // Along the axis from below: enters the lower cap.
        let t = capsule_hit(Vec3::new(0.0, -3.0, 0.0), Vec3::Y, a, b, 0.5).unwrap();
        assert!((t - 2.5).abs() < 1e-5);
        // Past the top end, but within the cap.
        let t = capsule_hit(Vec3::new(-5.0, 2.3, 0.0), Vec3::X, a, b, 0.5).unwrap();
        assert!((t - (5.0 - (0.25f32 - 0.09).sqrt())).abs() < 1e-5);
        assert_eq!(
            capsule_hit(Vec3::new(-5.0, 1.0, 0.6), Vec3::X, a, b, 0.5),
            None
        );
        assert_eq!(
            capsule_hit(Vec3::new(-5.0, 2.6, 0.0), Vec3::X, a, b, 0.5),
            None
        );
        // Behind the ray.
        assert_eq!(
            capsule_hit(Vec3::new(5.0, 1.0, 0.0), Vec3::X, a, b, 0.5),
            None
        );
    }

    #[test]
    fn nearest_branch_wins() {
        let mut t = Tree::new(Arc::new(TreeSpecies::default()), 0, Vec3::ZERO).unwrap();
        let root = t.root();
        // Two parallel branches above the root, one behind the other along X.
        let near = t
            .grow(
                &OpenAir,
                root,
                SlotIndex::Main,
                Vec3::new(-1.0, 1.0, 0.0),
                1.0,
            )
            .unwrap();
        let far = t
            .grow(
                &OpenAir,
                root,
                SlotIndex::Lateral(0),
                Vec3::new(1.0, 1.0, 0.0),
                1.0,
            )
            .unwrap();
        let y = t.node(root).position.y + 0.5;
        assert_eq!(pick(&t, Vec3::new(-10.0, y, 0.0), Vec3::X), Some(near));
        assert_eq!(pick(&t, Vec3::new(10.0, y, 0.0), -Vec3::X), Some(far));
        assert_eq!(pick(&t, Vec3::new(-10.0, 50.0, 0.0), Vec3::X), None);
    }
}

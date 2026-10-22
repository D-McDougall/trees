//! Stored leaf attachments following Longay (2014), Appendix C.1.1.
//!
//! The paper specifies thin-branch eligibility and phyllotactic initial
//! orientation. Distance sampling, surface offsets and identity are project
//! conventions; see `docs/foliage.md`. These are initial frames, before the
//! petiole bending/twisting of C.1.2, and contain no renderer-specific data.

use glam::{Quat, Vec3};

use crate::{NodeId, SlotIndex, Tree};

/// Identity stable across rebuilds and edits to unrelated branches within a tree.
/// Node ids are tree-local; this is not a cross-tree asset identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LeafId {
    /// First internode of the supporting main-child axis.
    pub axis: NodeId,
    /// Distance station, including stations rejected by the radius cutoff.
    pub station: u64,
    /// Zero for alternate leaves, zero or one for an opposite pair.
    pub side: u8,
}

/// A leaf owned by a tree snapshot, attached to the bark, independent of buds.
#[derive(Clone, Debug, PartialEq)]
pub struct Leaf {
    /// Stable attachment identity.
    pub id: LeafId,
    /// Supporting internode; always live and not a stump after a rebuild.
    pub node: NodeId,
    /// Distance from the supporting internode's base along its centerline.
    pub distance: f32,
    /// Petiole origin on the tapered branch surface.
    pub attachment: Vec3,
    /// Unit initial petiole/blade heading H, from blade base toward tip.
    pub heading: Vec3,
    /// Unit initial blade left vector L; H × L = U.
    pub left: Vec3,
    /// Unit initial blade normal U, before petiole reorientation.
    pub normal: Vec3,
}

impl Leaf {
    /// Initial blade base at the end of the straight, unbent petiole.
    /// Use the owning tree's species petiole length. For the reoriented
    /// endpoint, use [`crate::solve_petiole`] and [`crate::Petiole::tip`].
    pub fn blade_base(&self, petiole_length: f32) -> Vec3 {
        self.attachment + self.heading * petiole_length
    }
}

pub(crate) fn place_leaves(tree: &Tree) -> Vec<Leaf> {
    let f = &tree.species().foliage;
    let mut leaves = Vec::new();
    if !f.enabled || f.max_radius == 0.0 || f.blade_length == 0.0 || f.blade_width == 0.0 {
        return leaves;
    }
    for (axis, first) in tree.iter() {
        if first
            .parent
            .is_some_and(|p| tree.node(p).main.child() == Some(axis))
        {
            continue;
        }
        // Hash the structural path, not arena allocation order, for phase.
        let mut key = tree.seed();
        for slot in tree.slot_path(axis).expect("live axis has a path") {
            key = mix(key
                ^ match slot {
                    SlotIndex::Main => 1,
                    SlotIndex::Lateral(i) => u64::from(i) + 2,
                });
        }
        let phase = (mix(key) >> 40) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU;
        let mut radial =
            Quat::from_axis_angle(first.heading, phase) * first.heading.any_orthonormal_vector();
        let mut heading = first.heading;
        let mut start = 0.0f64;
        let spacing = f64::from(f.spacing);
        let mut current = Some(axis);
        while let Some(id) = current {
            let node = tree.node(id);
            if node.stump {
                break;
            }
            // Transport the axis frame without branch-bud divergence.
            radial = Quat::from_rotation_arc(heading, node.heading) * radial;
            radial = (radial - node.heading * radial.dot(node.heading)).normalize();
            heading = node.heading;
            let length = f64::from(node.length);
            let end = start + length;
            let base_radius = tree.base_radius(id);
            if length > 0.0 && base_radius.min(node.radius) < f.max_radius {
                // Half-open intervals assign junction stations to the distal
                // segment. Rejected thick stations still advance the phase.
                let mut station = (start / spacing - 0.5).ceil().max(0.0) as u64;
                loop {
                    let along_axis = (station as f64 + 0.5) * spacing;
                    if along_axis >= end {
                        break;
                    }
                    let distance = (along_axis - start).max(0.0) as f32;
                    let t = distance / node.length;
                    let radius = base_radius + (node.radius - base_radius) * t;
                    if radius < f.max_radius {
                        let angle = ((station as f64 * f64::from(f.divergence_angle))
                            .rem_euclid(std::f64::consts::TAU))
                            as f32;
                        let direction = Quat::from_axis_angle(heading, angle) * radial;
                        for side in 0..f.arrangement.lateral_count() as u8 {
                            let outward = if side == 0 { direction } else { -direction };
                            let (sin, cos) = f.attachment_angle.sin_cos();
                            let leaf_heading = (cos * heading + sin * outward).normalize();
                            let left = heading.cross(outward).normalize();
                            leaves.push(Leaf {
                                id: LeafId {
                                    axis,
                                    station,
                                    side,
                                },
                                node: id,
                                distance,
                                attachment: node.base() + heading * distance + outward * radius,
                                heading: leaf_heading,
                                left,
                                normal: leaf_heading.cross(left).normalize(),
                            });
                        }
                    }
                    station = station.checked_add(1).expect("too many leaf stations");
                }
            }
            start = end;
            current = node.main.child();
        }
    }
    leaves
}

fn mix(mut key: u64) -> u64 {
    key = key.wrapping_add(0x9E37_79B9_7F4A_7C15);
    key = (key ^ (key >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    key = (key ^ (key >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    key ^ (key >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BranchingPattern, OpenAir, TreeSpecies};
    use std::sync::Arc;

    fn seedling(edit: impl FnOnce(&mut TreeSpecies)) -> Tree {
        let mut s = TreeSpecies::default();
        s.foliage.spacing = 0.5;
        s.foliage.max_radius = 1.0;
        s.foliage.divergence_angle = 0.0;
        s.foliage.attachment_angle = std::f32::consts::FRAC_PI_2;
        edit(&mut s);
        Tree::new(Arc::new(s), 42, Vec3::ZERO).unwrap()
    }

    fn close(a: Vec3, b: Vec3) {
        assert!(a.distance(b) < 2e-5, "{a:?} != {b:?}");
    }

    #[test]
    fn exact_stations_surface_offsets_and_initial_frames() {
        let t = seedling(|_| {});
        assert_eq!(t.leaves().len(), 2);
        for (leaf, distance) in t.leaves().iter().zip([0.25, 0.75]) {
            assert_eq!(leaf.distance, distance);
            let radial = leaf.attachment - Vec3::Y * distance;
            assert!((radial.length() - t.node(t.root()).radius).abs() < 1e-6);
            close(leaf.heading, radial.normalize());
            close(leaf.heading.cross(leaf.left), leaf.normal);
            close(leaf.normal, Vec3::Y);
            assert!(leaf.heading.dot(leaf.left).abs() < 1e-6);
            close(leaf.blade_base(0.1), leaf.attachment + 0.1 * leaf.heading);
        }
        let mut clone = t.clone();
        clone.update_leaves();
        assert_eq!(clone.leaves(), t.leaves());
    }

    #[test]
    fn opposite_foliage_is_independent_of_branch_buds() {
        let t = seedling(|s| s.foliage.arrangement = BranchingPattern::Opposite);
        assert_eq!(
            t.species().architecture.branching,
            BranchingPattern::Alternate
        );
        assert_eq!(t.leaves().len(), 4);
        for pair in t.leaves().as_chunks::<2>().0 {
            close(pair[0].heading, -pair[1].heading);
            close(
                (pair[0].attachment + pair[1].attachment) * 0.5,
                Vec3::Y * pair[0].distance,
            );
            assert_eq!(pair[0].id.station, pair[1].id.station);
            assert_ne!(pair[0].id.side, pair[1].id.side);
        }
        let alternate = seedling(|s| s.architecture.branching = BranchingPattern::Opposite);
        assert_eq!(alternate.leaves().len(), 2);
    }

    #[test]
    fn divergence_rotates_each_station() {
        let t = seedling(|s| s.foliage.divergence_angle = std::f32::consts::FRAC_PI_2);
        close(
            t.leaves()[1].heading,
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * t.leaves()[0].heading,
        );
    }

    #[test]
    fn main_axis_sampling_survives_subdivision_and_branch_divergence() {
        let mut whole = seedling(|s| s.foliage.divergence_angle = 0.7);
        whole.grow(&OpenAir, whole.root(), SlotIndex::Main, Vec3::Y, 1.0);
        let mut split = seedling(|s| {
            s.foliage.divergence_angle = 0.7;
            s.architecture.divergence_angle = 1.8;
        });
        let a = split
            .grow(&OpenAir, split.root(), SlotIndex::Main, Vec3::Y, 0.4)
            .unwrap();
        split.grow(&OpenAir, a, SlotIndex::Main, Vec3::Y, 0.6);
        assert_eq!(whole.leaves().len(), 4);
        assert_eq!(split.leaves().len(), 4);
        for (a, b) in whole.leaves().iter().zip(split.leaves()) {
            assert_eq!(a.id, b.id);
            close(a.attachment, b.attachment);
            close(a.heading, b.heading);
        }
    }

    #[test]
    fn bent_and_lateral_axes_transport_frames_and_restart_spacing() {
        let mut t = seedling(|_| {});
        let main = t
            .grow(&OpenAir, t.root(), SlotIndex::Main, Vec3::X, 0.5)
            .unwrap();
        let lateral = t
            .grow(&OpenAir, t.root(), SlotIndex::Lateral(0), Vec3::Z, 0.5)
            .unwrap();
        assert_eq!(t.leaves().len(), 4);
        let main_leaf = t.leaves().iter().find(|l| l.node == main).unwrap();
        assert_eq!(main_leaf.id.station, 2);
        let lat = t.leaves().iter().find(|l| l.node == lateral).unwrap();
        assert_eq!(lat.id.station, 0);
        assert_eq!(lat.id.axis, lateral);
        assert_eq!(lat.distance, 0.25);
        for l in t.leaves() {
            assert!(l.normal.is_finite());
            assert!((l.heading.length() - 1.0).abs() < 1e-6);
            assert!((l.left.length() - 1.0).abs() < 1e-6);
            assert!((l.normal.length() - 1.0).abs() < 1e-6);
            assert!(l.heading.dot(l.left).abs() < 1e-6);
        }
    }

    #[test]
    fn radius_cutoff_is_strict_and_uses_local_taper() {
        let equal = seedling(|s| s.foliage.max_radius = s.thickness.tip_radius);
        assert!(equal.leaves().is_empty());
        let mut t = seedling(|s| s.foliage.max_radius = 0.05);
        let child = t
            .grow(&OpenAir, t.root(), SlotIndex::Main, Vec3::Y, 1.0)
            .unwrap();
        t.node_mut(t.root()).radius = 0.1;
        t.node_mut(child).radius = 0.02;
        t.update_leaves();
        assert_eq!(t.leaves().len(), 1);
        let l = &t.leaves()[0];
        assert_eq!(l.distance, 0.75);
        assert_eq!(l.id.station, 3);
        assert!(((l.attachment - Vec3::Y * 1.75).length() - 0.04).abs() < 1e-6);
    }

    #[test]
    fn mutations_keep_snapshots_and_leaf_ownership_consistent() {
        let mut t = seedling(|s| s.foliage.max_radius = 0.05);
        let root = t.root();
        let a = t
            .grow(&OpenAir, root, SlotIndex::Lateral(0), Vec3::X, 1.0)
            .unwrap();
        let b = t.grow(&OpenAir, a, SlotIndex::Main, Vec3::X, 0.5).unwrap();
        let snapshot = t.clone();
        t.shed(a);
        assert!(t.leaves().iter().all(|l| l.node != a && l.node != b));
        assert!(snapshot.leaves().iter().any(|l| l.node == b));
        let mut pruned = snapshot;
        pruned.prune(a);
        assert!(pruned.leaves().iter().all(|l| pruned.get(l.node).is_some()));
        assert!(pruned.leaves().iter().all(|l| l.node != a && l.node != b));
        let before = pruned.leaves().to_vec();
        pruned.pin_radius(root, 0.1);
        pruned.update_radii();
        assert!(pruned.leaves().is_empty());
        pruned.unpin(root);
        pruned.update_radii();
        assert_eq!(pruned.leaves(), before);
    }

    #[test]
    fn disabled_zero_size_and_zero_length_produce_no_attachments() {
        for edit in [
            (|s: &mut TreeSpecies| s.foliage.enabled = false) as fn(&mut TreeSpecies),
            |s| s.foliage.max_radius = 0.0,
            |s| s.foliage.blade_width = 0.0,
            |s| s.foliage.blade_length = 0.0,
        ] {
            assert!(seedling(edit).leaves().is_empty());
        }
        let mut t = seedling(|_| {});
        let a = t
            .grow(&OpenAir, t.root(), SlotIndex::Main, Vec3::Y, 0.0)
            .unwrap();
        assert!(t.leaves().iter().all(|l| l.node != a));
        let conifer = Tree::new(Arc::new(TreeSpecies::conifer()), 0, Vec3::ZERO).unwrap();
        assert!(conifer.leaves().is_empty());
    }

    #[test]
    fn junction_stations_belong_only_to_the_distal_internode() {
        let mut t = seedling(|s| s.foliage.spacing = 2.0);
        assert!(t.leaves().is_empty());
        let a = t
            .grow(&OpenAir, t.root(), SlotIndex::Main, Vec3::Y, 1.0)
            .unwrap();
        assert_eq!(t.leaves().len(), 1);
        assert_eq!(t.leaves()[0].node, a);
        assert_eq!(t.leaves()[0].distance, 0.0);
        assert_eq!(t.leaves()[0].id.station, 0);
    }

    #[test]
    fn appearance_dimensions_and_failed_growth_do_not_move_attachments() {
        let mut t = seedling(|_| {});
        let other = seedling(|s| {
            s.foliage.appearance = "custom-transparent-png".into();
            s.foliage.blade_width = 0.7;
            s.foliage.blade_length = 1.2;
            s.foliage.petiole_length = 0.0;
        });
        assert_eq!(t.leaves(), other.leaves());
        let before = t.leaves().to_vec();
        assert_eq!(
            t.grow(&|_: Vec3| true, t.root(), SlotIndex::Main, Vec3::Y, 1.0),
            None
        );
        assert_eq!(t.leaves(), before);
    }

    #[test]
    fn seed_changes_phase_but_not_density_and_growth_does_not_move_old_leaves() {
        let mut t = seedling(|_| {});
        let before = t.leaves().to_vec();
        t.grow(&OpenAir, t.root(), SlotIndex::Lateral(0), Vec3::X, 1.0);
        assert_eq!(&t.leaves()[..before.len()], before);
        let other = Tree::new(Arc::clone(t.species()), 43, Vec3::ZERO).unwrap();
        assert_eq!(other.leaves().len(), before.len());
        assert_ne!(other.leaves()[0].heading, before[0].heading);
    }
}

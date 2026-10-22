//! The species-agnostic tree node.
//!
//! One [`TreeNode`] is one internode (the stem segment between two nodes) plus
//! the buds at its tip. Its child slots hold either another node or a bud:
//! an empty child slot *is* a bud (Hart 2020), so buds need no separate
//! storage and "this bud grows" is just "this slot becomes a child".

use std::sync::Arc;

use glam::{Quat, Vec3};

use crate::species::TreeSpecies;

/// Index of a node in its [`Tree`](crate::Tree)'s arena.
///
/// Ids are never reused within a tree, so an id held by the edit history or a
/// constraint can never silently start pointing at a different branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub(crate) u32);

impl NodeId {
    /// Position in the arena, for indexing per-node arrays such as the
    /// output of [`Tree::fold_up`](crate::Tree::fold_up).
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// The only decision growth makes about a bud.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudFate {
    /// Alive but not growing this step (no markers, or too little vigor).
    Dormant,
    /// Won space and vigor this step; will grow a shoot.
    Active,
    /// Dead; will never grow. Kept so the slot is not mistaken for a free one.
    Aborted,
}

/// A bud: an unoccupied child slot that may grow into a new node.
///
/// The per-step fields (`markers`, `light`, `vigor`) are scratch written by
/// the simulator's sensing and vigor stages and reset at the start of a step.
#[derive(Clone, Debug, PartialEq)]
pub struct Bud {
    /// Default growth direction (unit), set from phyllotaxis when the bud forms.
    pub direction: Vec3,
    /// Whether the bud grows, waits or is dead this step.
    pub fate: BudFate,
    /// Number of attraction markers this bud won this step.
    pub marker_count: u32,
    /// Sum of unit vectors from the bud to its markers. Normalized, it is the
    /// environment direction E of H' = αH + βT + γE. Storing the sum rather
    /// than marker ids keeps the node fixed-size.
    pub marker_direction_sum: Vec3,
    /// Light exposure Q = b·L^κ at the bud this step.
    pub light: f32,
    /// Vigor allocated to the bud this step.
    pub vigor: f32,
}

impl Bud {
    /// A dormant bud pointing along `direction`, with cleared scratch fields.
    pub fn new(direction: Vec3) -> Self {
        Bud {
            direction,
            fate: BudFate::Dormant,
            marker_count: 0,
            marker_direction_sum: Vec3::ZERO,
            light: 0.0,
            vigor: 0.0,
        }
    }

    /// Unit direction toward this bud's markers, if it won any.
    pub fn environment_direction(&self) -> Option<Vec3> {
        (self.marker_count > 0)
            .then(|| self.marker_direction_sum.try_normalize())
            .flatten()
    }
}

/// What occupies one child position of a node.
#[derive(Clone, Debug, PartialEq)]
pub enum Slot {
    /// Nothing here: an unused lateral slot (alternate branching uses one of
    /// two), a pruned branch, or any slot of a stump (see [`TreeNode::stump`]).
    Empty,
    /// A live or aborted bud that has not grown.
    Bud(Bud),
    /// The bud grew into this node.
    Child(NodeId),
}

impl Slot {
    /// The child node, if the bud here has grown.
    pub fn child(&self) -> Option<NodeId> {
        match self {
            Slot::Child(id) => Some(*id),
            _ => None,
        }
    }

    /// The bud, if this slot still holds one.
    pub fn bud(&self) -> Option<&Bud> {
        match self {
            Slot::Bud(b) => Some(b),
            _ => None,
        }
    }

    /// Mutable access to the bud, if this slot still holds one.
    pub fn bud_mut(&mut self) -> Option<&mut Bud> {
        match self {
            Slot::Bud(b) => Some(b),
            _ => None,
        }
    }
}

/// Names one child slot of a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SlotIndex {
    /// The terminal bud / continuing axis.
    Main,
    /// Lateral 0 or 1. Higher indices make [`TreeNode::slot`] panic.
    Lateral(u8),
}

/// One internode of a tree. Holds no species parameters itself: anything
/// species-specific is read through [`TreeNode::species`].
///
/// Geometry is stored in world space, so no turtle transforms are re-walked
/// when reading positions; a frame transform happens only when a node is added.
#[derive(Clone, Debug)]
pub struct TreeNode {
    /// The shared, immutable parameters of the species this node grows by.
    pub species: Arc<TreeSpecies>,
    /// `None` only for the root.
    pub parent: Option<NodeId>,

    /// World-space position of the node, i.e. the tip of the internode where
    /// its buds sit. The base is the parent's position (see [`TreeNode::base`]).
    pub position: Vec3,
    /// Unit growth direction of the internode.
    pub heading: Vec3,
    /// Unit vector perpendicular to `heading` pointing at lateral bud 0. It is
    /// rotated by the divergence angle at each node, which is what produces
    /// phyllotaxis. Together with `heading` it fixes the full frame.
    pub lateral: Vec3,
    /// Length of the internode, from [`TreeNode::base`] to `position`.
    pub length: f32,
    /// Current radius from the pipe model, set by
    /// [`Tree::update_radii`](crate::Tree::update_radii). Never decreases, and a shed branch
    /// leaves a stump that keeps its radius, so trunks keep the thickness of
    /// branches that were later shed.
    pub radius: f32,

    /// 0 for the trunk; a lateral on an order-n axis starts an order n+1 axis.
    pub order: u16,
    /// Growth step in which this node was created; age is `tree.step() - birth_step`.
    pub birth_step: u32,

    /// Light gathered by this node's whole subtree (tip-to-base pass).
    pub light: f32,
    /// Vigor flowing into this node (base-to-tip pass).
    pub vigor: f32,
    /// Number of internodes in this node's subtree, including itself.
    /// Shedding compares `light / internode_count` to a threshold.
    pub internode_count: u32,

    /// The continuing axis: a terminal bud, or the next internode.
    pub main: Slot,
    /// Lateral (axillary) slots. Alternate branching uses only `laterals[0]`.
    pub laterals: [Slot; 2],

    /// True if this node is the remaining base of a shed branch (see
    /// [`Tree::shed`](crate::Tree::shed)). A stump has only empty slots, so it
    /// never grows, and it cannot be shed again. It keeps its radius, so it
    /// still contributes its cross-section to the pipe model below it.
    pub stump: bool,
}

impl TreeNode {
    /// A new node whose buds are placed by its species' phyllotaxis.
    ///
    /// The lateral vector moves along the tree as in Hart 2020 §9.3: the
    /// parent's lateral is rotated by the divergence angle about the parent's
    /// heading, then carried by the rotation from the parent's heading to this
    /// node's. For the root, pass its own heading and any perpendicular vector.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        species: Arc<TreeSpecies>,
        parent: Option<NodeId>,
        position: Vec3,
        heading: Vec3,
        parent_heading: Vec3,
        parent_lateral: Vec3,
        length: f32,
        order: u16,
        birth_step: u32,
    ) -> Self {
        let arch = &species.architecture;
        let heading = heading.normalize();
        let parent_heading = parent_heading.normalize();
        let q_div = Quat::from_axis_angle(parent_heading, arch.divergence_angle);
        let q_head = Quat::from_rotation_arc(parent_heading, heading);
        let lateral = q_head * q_div * parent_lateral;
        // Re-orthonormalize against the drift of repeated rotations.
        let lateral = (lateral - heading * lateral.dot(heading))
            .try_normalize()
            .unwrap_or_else(|| heading.any_orthonormal_vector());

        // b_main = cos δ·h − sin δ·l ; b_lat = cos ψ·h ± sin ψ·l
        let (sin_d, cos_d) = arch.deflection_angle.sin_cos();
        let (sin_b, cos_b) = arch.branching_angle.sin_cos();
        let main = Slot::Bud(Bud::new(cos_d * heading - sin_d * lateral));
        let mut laterals = [Slot::Empty, Slot::Empty];
        laterals[0] = Slot::Bud(Bud::new(cos_b * heading + sin_b * lateral));
        if arch.branching.lateral_count() == 2 {
            laterals[1] = Slot::Bud(Bud::new(cos_b * heading - sin_b * lateral));
        }

        let radius = species.thickness.tip_radius;
        TreeNode {
            species,
            parent,
            position,
            heading,
            lateral,
            length,
            radius,
            order,
            birth_step,
            light: 0.0,
            vigor: 0.0,
            internode_count: 1,
            main,
            laterals,
            stump: false,
        }
    }

    /// World-space start of the internode.
    pub fn base(&self) -> Vec3 {
        self.position - self.heading * self.length
    }

    /// Third axis of the node frame, completing (heading, lateral, binormal).
    pub fn binormal(&self) -> Vec3 {
        self.heading.cross(self.lateral)
    }

    /// The slot named by `index`.
    ///
    /// Panics if `index` is `Lateral(i)` with `i > 1`.
    pub fn slot(&self, index: SlotIndex) -> &Slot {
        match index {
            SlotIndex::Main => &self.main,
            SlotIndex::Lateral(i) => &self.laterals[i as usize],
        }
    }

    /// Mutable access to the slot named by `index`; panics like [`TreeNode::slot`].
    pub fn slot_mut(&mut self, index: SlotIndex) -> &mut Slot {
        match index {
            SlotIndex::Main => &mut self.main,
            SlotIndex::Lateral(i) => &mut self.laterals[i as usize],
        }
    }

    /// The slots this node's species uses: main, then one or two laterals.
    pub fn slots(&self) -> impl Iterator<Item = (SlotIndex, &Slot)> {
        let lateral_count = self.species.architecture.branching.lateral_count();
        std::iter::once((SlotIndex::Main, &self.main)).chain(
            self.laterals[..lateral_count]
                .iter()
                .enumerate()
                .map(|(i, s)| (SlotIndex::Lateral(i as u8), s)),
        )
    }

    /// Ids of child nodes, continuing axis first.
    pub fn children(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.slots().filter_map(|(_, s)| s.child())
    }

    /// Buds still attached to this node.
    pub fn buds(&self) -> impl Iterator<Item = (SlotIndex, &Bud)> {
        self.slots().filter_map(|(i, s)| s.bud().map(|b| (i, b)))
    }

    /// True if no child grows from this node: nothing has grown yet, or
    /// everything that grew was pruned or shed (a stump is terminal).
    pub fn is_terminal(&self) -> bool {
        self.children().next().is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::BranchingPattern;

    const EPS: f32 = 1e-5;

    /// A root-like node: no change of heading, `hint` as the prior lateral.
    fn node(species: TreeSpecies, heading: Vec3, hint: Vec3) -> TreeNode {
        child(species, heading, hint, heading)
    }

    fn child(
        species: TreeSpecies,
        parent_heading: Vec3,
        parent_lateral: Vec3,
        heading: Vec3,
    ) -> TreeNode {
        TreeNode::new(
            Arc::new(species),
            None,
            Vec3::ZERO,
            heading,
            parent_heading,
            parent_lateral,
            1.0,
            0,
            0,
        )
    }

    fn opposite() -> TreeSpecies {
        let mut s = TreeSpecies::default();
        s.architecture.branching = BranchingPattern::Opposite;
        s
    }

    #[test]
    fn frame_is_orthonormal() {
        let h = Vec3::new(1.0, 2.0, -0.5).normalize();
        let l = h.any_orthonormal_vector();
        // Includes a child along the parent's lateral (ψ = 90°) and one that
        // turns fully back on its parent.
        for heading in [h, l, -l, h + l, -h, Vec3::Y, Vec3::new(0.3, -1.0, 0.2)] {
            let n = child(TreeSpecies::default(), h, l, heading);
            assert!((n.heading.length() - 1.0).abs() < EPS);
            assert!((n.lateral.length() - 1.0).abs() < EPS);
            assert!(n.heading.dot(n.lateral).abs() < EPS);
            assert!((n.binormal().length() - 1.0).abs() < EPS);
        }
    }

    #[test]
    fn lateral_is_transported_as_in_hart() {
        // Hart 2020 §9.3: rotate l by the divergence angle about the previous
        // heading, then by the rotation from the previous to the new heading.
        let mut s = TreeSpecies::default();
        s.architecture.divergence_angle = 0.0;
        // A lateral grown at ψ = 90° along l = X: turning Y onto X takes X to −Y.
        let n = child(s.clone(), Vec3::Y, Vec3::X, Vec3::X);
        assert!((n.lateral + Vec3::Y).length() < EPS);
        // A turn about the lateral itself leaves the lateral unchanged.
        let n = child(s.clone(), Vec3::Y, Vec3::X, Vec3::new(0.0, 1.0, 1.0));
        assert!((n.lateral - Vec3::X).length() < EPS);

        let s = TreeSpecies::default();
        let phi = s.architecture.divergence_angle;
        let heading = Vec3::new(0.4, 1.0, -0.3).normalize();
        let n = child(s, Vec3::Y, Vec3::X, heading);
        let expected = Quat::from_rotation_arc(Vec3::Y, heading)
            * Quat::from_axis_angle(Vec3::Y, phi)
            * Vec3::X;
        assert!((n.lateral - expected).length() < EPS);
    }

    #[test]
    fn bud_directions_are_unit_and_at_species_angles() {
        let s = opposite();
        let (psi, delta) = (
            s.architecture.branching_angle,
            s.architecture.deflection_angle,
        );
        let n = node(s, Vec3::new(0.3, 1.0, 0.2), Vec3::X);
        let main = n.main.bud().unwrap().direction;
        assert!((main.length() - 1.0).abs() < EPS);
        assert!((main.angle_between(n.heading) - delta).abs() < 1e-4);
        for slot in &n.laterals {
            let d = slot.bud().unwrap().direction;
            assert!((d.length() - 1.0).abs() < EPS);
            assert!((d.angle_between(n.heading) - psi).abs() < 1e-4);
        }
    }

    #[test]
    fn opposite_laterals_mirror_about_heading() {
        let n = node(opposite(), Vec3::Y, Vec3::X);
        let a = n.laterals[0].bud().unwrap().direction;
        let b = n.laterals[1].bud().unwrap().direction;
        // Same component along the heading, opposite components across it.
        assert!((a.dot(n.heading) - b.dot(n.heading)).abs() < EPS);
        let across = |d: Vec3| d - n.heading * d.dot(n.heading);
        assert!((across(a) + across(b)).length() < EPS);
    }

    #[test]
    fn main_bud_deflects_away_from_lateral() {
        let n = node(TreeSpecies::default(), Vec3::Y, Vec3::X);
        let main = n.main.bud().unwrap().direction;
        let lat = n.laterals[0].bud().unwrap().direction;
        assert!(main.dot(n.lateral) < 0.0 && lat.dot(n.lateral) > 0.0);
        // Deflection stays in the heading-lateral plane.
        assert!(main.dot(n.binormal()).abs() < EPS);
    }

    #[test]
    fn lateral_rotates_by_divergence_angle() {
        let s = TreeSpecies::default();
        let phi = s.architecture.divergence_angle;
        let n = node(s, Vec3::Y, Vec3::X);
        // Signed angle from the hint to the lateral, measured about the heading.
        let signed = Vec3::X
            .cross(n.lateral)
            .dot(n.heading)
            .atan2(Vec3::X.dot(n.lateral));
        assert!((signed.rem_euclid(std::f32::consts::TAU) - phi).abs() < 1e-4);
    }

    #[test]
    fn alternate_branching_leaves_second_lateral_empty() {
        let n = node(TreeSpecies::default(), Vec3::Y, Vec3::X);
        assert_eq!(n.laterals[1], Slot::Empty);
        assert_eq!(n.slots().count(), 2);
        assert_eq!(node(opposite(), Vec3::Y, Vec3::X).slots().count(), 3);
    }

    #[test]
    fn environment_direction_is_normalized_marker_sum() {
        let mut b = Bud::new(Vec3::Y);
        assert_eq!(b.environment_direction(), None);
        b.marker_count = 2;
        b.marker_direction_sum = Vec3::X + Vec3::Z;
        let e = b.environment_direction().unwrap();
        assert!((e - Vec3::new(1.0, 0.0, 1.0).normalize()).length() < EPS);
        // Markers on opposite sides cancel: no preferred direction.
        b.marker_direction_sum = Vec3::ZERO;
        assert_eq!(b.environment_direction(), None);
    }
}

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
    /// two), or a branch that was shed or pruned.
    Empty,
    Bud(Bud),
    Child(NodeId),
}

impl Slot {
    pub fn child(&self) -> Option<NodeId> {
        match self {
            Slot::Child(id) => Some(*id),
            _ => None,
        }
    }

    pub fn bud(&self) -> Option<&Bud> {
        match self {
            Slot::Bud(b) => Some(b),
            _ => None,
        }
    }

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
    /// Lateral 0 or 1.
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
    pub length: f32,
    /// Current radius from the pipe model. Never decreases, so trunks keep the
    /// thickness of branches that were later shed.
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
}

impl TreeNode {
    /// A new node whose buds are placed by its species' phyllotaxis.
    ///
    /// `lateral_hint` is the parent's lateral vector (or any vector for the
    /// root); it is rotated by the divergence angle and re-orthogonalized
    /// against `heading`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        species: Arc<TreeSpecies>,
        parent: Option<NodeId>,
        position: Vec3,
        heading: Vec3,
        lateral_hint: Vec3,
        length: f32,
        order: u16,
        birth_step: u32,
    ) -> Self {
        let arch = &species.architecture;
        let heading = heading.normalize();
        let lateral = Quat::from_axis_angle(heading, arch.divergence_angle) * lateral_hint;
        // A hint (nearly) parallel to the heading leaves only rounding error
        // after projection, which would normalize to a non-perpendicular
        // vector, so fall back to an arbitrary perpendicular instead.
        let projected = lateral - heading * lateral.dot(heading);
        let lateral = if projected.length_squared() > 1e-6 * lateral.length_squared() {
            projected.normalize()
        } else {
            heading.any_orthonormal_vector()
        };

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

    pub fn slot(&self, index: SlotIndex) -> &Slot {
        match index {
            SlotIndex::Main => &self.main,
            SlotIndex::Lateral(i) => &self.laterals[i as usize],
        }
    }

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

    /// True if nothing has grown from this node yet.
    pub fn is_terminal(&self) -> bool {
        self.children().next().is_none()
    }
}

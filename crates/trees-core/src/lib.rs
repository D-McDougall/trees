//! Core data structures for a self-organizing tree generator.
//!
//! The tree graph is the single source of truth (see the design document):
//! the growth simulator and user edits change it, and all geometry is derived
//! from it. Three types carry it:
//!
//! - [`TreeSpecies`]: an immutable record of species-specific parameters,
//!   shared by every node of a tree through an `Arc`.
//! - [`TreeNode`]: one internode, species-agnostic. Its child slots are either
//!   another node or a [`Bud`], so buds need no separate storage.
//! - [`Tree`]: an arena that owns the nodes and hands out [`NodeId`]s.
//!   It also stores [`Leaf`] attachments independently of branch buds.
//!
//! The host world is seen only through the [`Habitat`] trait, which growth
//! consults so branches do not grow into occupied space.

#![warn(missing_docs)]

pub mod foliage;
pub mod habitat;
pub mod node;
pub mod petiole;
pub mod species;
pub mod tree;

pub use foliage::{Leaf, LeafId};
pub use habitat::{Habitat, OpenAir};
pub use node::{Bud, BudFate, NodeId, Slot, SlotIndex, TreeNode};
pub use petiole::{Petiole, PetioleError, PetioleOptions, PetioleSample, solve_petiole};
pub use species::{
    Architecture, BranchingPattern, Foliage, Gravimorphism, PetioleResponse, Shadow, Shedding,
    SpaceColonization, SpeciesError, Thickness, TreeSpecies, Tropism, Vigor, VigorModel,
};
pub use tree::Tree;

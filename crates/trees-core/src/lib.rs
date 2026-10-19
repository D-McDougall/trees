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

#![warn(missing_docs)]

pub mod node;
pub mod species;
pub mod tree;

pub use node::{Bud, BudFate, NodeId, Slot, SlotIndex, TreeNode};
pub use species::{
    Architecture, BranchingPattern, Gravimorphism, Shadow, Shedding, SpaceColonization,
    SpeciesError, Thickness, TreeSpecies, Tropism, Vigor, VigorModel,
};
pub use tree::Tree;

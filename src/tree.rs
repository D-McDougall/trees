//! The tree graph: an arena of [`TreeNode`]s.
//!
//! Nodes live in one `Vec` and refer to each other by [`NodeId`] rather than
//! by pointers. That gives Rust a single owner (no `Rc<RefCell<…>>` cycles),
//! makes a render snapshot a plain `clone()`, and keeps whole-tree passes
//! cache-friendly.
//!
//! Nodes are only ever appended, and always after their parent, so index
//! order is a valid topological order: a forward scan visits parents before
//! children (base to tip) and a reverse scan visits children before parents
//! (tip to base). The folds below rely on that instead of recursing.

use std::sync::Arc;

use glam::Vec3;

use crate::node::{NodeId, Slot, SlotIndex, TreeNode};
use crate::species::{SpeciesError, TreeSpecies};

#[derive(Clone, Debug)]
pub struct Tree {
    species: Arc<TreeSpecies>,
    /// Seed for all position-hashed randomness, so the same species, seed and
    /// edit history always reproduce the same tree.
    seed: u64,
    /// Number of completed growth steps (seasons).
    step: u32,
    /// `None` marks a pruned or shed node; its id is retired, not reused.
    nodes: Vec<Option<TreeNode>>,
    live: usize,
}

impl Tree {
    /// A seedling: one vertical internode with its buds, standing at `origin`.
    pub fn new(species: Arc<TreeSpecies>, seed: u64, origin: Vec3) -> Result<Self, SpeciesError> {
        species.validate()?;
        let length = species.architecture.internode_length;
        let root = TreeNode::new(
            Arc::clone(&species),
            None,
            origin + Vec3::Y * length,
            Vec3::Y,
            Vec3::X,
            length,
            0,
            0,
        );
        Ok(Tree {
            species,
            seed,
            step: 0,
            nodes: vec![Some(root)],
            live: 1,
        })
    }

    pub fn species(&self) -> &Arc<TreeSpecies> {
        &self.species
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn step(&self) -> u32 {
        self.step
    }

    /// Marks the end of a growth step.
    pub fn advance_step(&mut self) {
        self.step += 1;
    }

    pub fn root(&self) -> NodeId {
        NodeId(0)
    }

    /// Number of live nodes.
    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    pub fn get(&self, id: NodeId) -> Option<&TreeNode> {
        self.nodes.get(id.index())?.as_ref()
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut TreeNode> {
        self.nodes.get_mut(id.index())?.as_mut()
    }

    /// Panics if `id` was pruned; use [`Tree::get`] when that is possible.
    pub fn node(&self, id: NodeId) -> &TreeNode {
        self.get(id).expect("node was pruned")
    }

    pub fn node_mut(&mut self, id: NodeId) -> &mut TreeNode {
        self.get_mut(id).expect("node was pruned")
    }

    /// Live nodes in base-to-tip order (every parent before its children).
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (NodeId, &TreeNode)> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| n.as_ref().map(|n| (NodeId(i as u32), n)))
    }

    /// Grows the bud in `slot` of `parent` into a new internode.
    ///
    /// Returns `None` if the slot holds no bud. The new node gets fresh buds
    /// placed by phyllotaxis and inherits the parent's species.
    pub fn grow(
        &mut self,
        parent: NodeId,
        slot: SlotIndex,
        direction: Vec3,
        length: f32,
    ) -> Option<NodeId> {
        let id = NodeId(u32::try_from(self.nodes.len()).expect("too many nodes"));
        let step = self.step;
        let p = self.get_mut(parent)?;
        p.slot(slot).bud()?;
        *p.slot_mut(slot) = Slot::Child(id);
        let order = match slot {
            SlotIndex::Main => p.order,
            SlotIndex::Lateral(_) => p.order + 1,
        };
        let heading = direction.normalize();
        let child = TreeNode::new(
            Arc::clone(&p.species),
            Some(parent),
            p.position + heading * length,
            heading,
            p.lateral,
            length,
            order,
            step,
        );
        self.nodes.push(Some(child));
        self.live += 1;
        Some(id)
    }

    /// Removes the subtree rooted at `id` and empties its parent's slot.
    /// Returns how many nodes were removed. The root cannot be pruned.
    pub fn prune(&mut self, id: NodeId) -> usize {
        let Some(parent) = self.get(id).and_then(|n| n.parent) else {
            return 0;
        };
        let p = self.node_mut(parent);
        for slot in std::iter::once(&mut p.main).chain(p.laterals.iter_mut()) {
            if slot.child() == Some(id) {
                *slot = Slot::Empty;
            }
        }
        let mut stack = vec![id];
        let mut removed = 0;
        while let Some(n) = stack.pop() {
            if let Some(node) = self.nodes[n.index()].take() {
                stack.extend(node.children());
                removed += 1;
            }
        }
        self.live -= removed;
        removed
    }

    /// Tip-to-base fold: computes a value per node from the node and its
    /// children's values (light gathering, radii, internode counts).
    ///
    /// The result is indexed by `NodeId::index()`; pruned ids hold `None`.
    pub fn fold_up<T>(
        &self,
        mut f: impl FnMut(&TreeNode, &mut dyn Iterator<Item = &T>) -> T,
    ) -> Vec<Option<T>> {
        let mut out: Vec<Option<T>> = std::iter::repeat_with(|| None)
            .take(self.nodes.len())
            .collect();
        for (id, node) in self.iter().rev() {
            let value = {
                let mut kids = node
                    .children()
                    .map(|c| out[c.index()].as_ref().expect("child before parent"));
                f(node, &mut kids)
            };
            out[id.index()] = Some(value);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::BranchingPattern;

    fn tree(species: TreeSpecies) -> Tree {
        Tree::new(Arc::new(species), 42, Vec3::ZERO).unwrap()
    }

    #[test]
    fn seedling_has_species_buds() {
        let t = tree(TreeSpecies::default());
        assert_eq!(t.node(t.root()).buds().count(), 2);

        let mut maple = TreeSpecies::default();
        maple.architecture.branching = BranchingPattern::Opposite;
        let t = tree(maple);
        assert_eq!(t.node(t.root()).buds().count(), 3);
    }

    #[test]
    fn grow_turns_bud_into_child_and_shares_species() {
        let mut t = tree(TreeSpecies::default());
        let root = t.root();
        let dir = t.node(root).main.bud().unwrap().direction;
        let a = t.grow(root, SlotIndex::Main, dir, 1.0).unwrap();
        let b = t.grow(root, SlotIndex::Lateral(0), Vec3::X, 1.0).unwrap();
        assert!(
            t.grow(root, SlotIndex::Main, dir, 1.0).is_none(),
            "slot already used"
        );
        assert_eq!(t.node(root).children().collect::<Vec<_>>(), vec![a, b]);
        assert_eq!(t.node(a).order, 0);
        assert_eq!(t.node(b).order, 1);
        assert!(Arc::ptr_eq(&t.node(a).species, t.species()));
        let n = t.node(a);
        assert!(n.heading.dot(n.lateral).abs() < 1e-5);
    }

    #[test]
    fn fold_up_counts_and_prune_removes_subtree() {
        let mut t = tree(TreeSpecies::default());
        let root = t.root();
        let a = t.grow(root, SlotIndex::Main, Vec3::Y, 1.0).unwrap();
        let b = t.grow(a, SlotIndex::Main, Vec3::Y, 1.0).unwrap();
        t.grow(a, SlotIndex::Lateral(0), Vec3::X, 1.0).unwrap();
        t.grow(b, SlotIndex::Main, Vec3::Y, 1.0).unwrap();

        let counts = t.fold_up(|_, kids| 1 + kids.sum::<u32>());
        assert_eq!(counts[root.index()], Some(5));

        assert_eq!(t.prune(a), 4);
        assert_eq!(t.len(), 1);
        assert!(t.get(b).is_none());
        assert_eq!(t.node(root).main, Slot::Empty);
        assert_eq!(t.fold_up(|_, kids| 1 + kids.sum::<u32>())[0], Some(1));
    }

    #[test]
    fn validation_rejects_bad_weights() {
        let mut s = TreeSpecies::conifer();
        assert!(s.validate().is_ok());
        s.tropism.environment = 0.5;
        assert!(Tree::new(Arc::new(s), 0, Vec3::ZERO).is_err());
    }
}

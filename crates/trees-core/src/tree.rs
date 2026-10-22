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

use crate::habitat::Habitat;
use crate::node::{NodeId, Slot, SlotIndex, TreeNode};
use crate::species::{SpeciesError, TreeSpecies};

/// One tree: its species, its seed, and the arena of its nodes.
///
/// The root is created with the tree and can never be pruned, so a tree is
/// never empty and [`Tree::root`] is always valid.
#[derive(Clone, Debug)]
pub struct Tree {
    species: Arc<TreeSpecies>,
    /// Seed for all position-hashed randomness, so the same species, seed and
    /// edit history always reproduce the same tree.
    seed: u64,
    /// Number of completed growth steps (seasons).
    step: u32,
    /// `None` marks a pruned node or one removed by shedding; its id is
    /// retired, not reused. A shed branch's own node stays, as a stump.
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

    /// The species every node of this tree grows by.
    pub fn species(&self) -> &Arc<TreeSpecies> {
        &self.species
    }

    /// Seed for this tree's position-hashed randomness.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Number of completed growth steps; new nodes record it as `birth_step`.
    pub fn step(&self) -> u32 {
        self.step
    }

    /// Marks the end of a growth step.
    pub fn advance_step(&mut self) {
        self.step += 1;
    }

    /// The first internode, grown by [`Tree::new`].
    pub fn root(&self) -> NodeId {
        NodeId(0)
    }

    /// Number of live nodes.
    pub fn len(&self) -> usize {
        self.live
    }

    /// Always false, since the root cannot be pruned; present to pair with [`Tree::len`].
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// The node with `id`, or `None` if it was pruned or shed.
    pub fn get(&self, id: NodeId) -> Option<&TreeNode> {
        self.nodes.get(id.index())?.as_ref()
    }

    /// Mutable access to the node with `id`, or `None` if it was pruned or shed.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut TreeNode> {
        self.nodes.get_mut(id.index())?.as_mut()
    }

    /// Panics if `id` was pruned; use [`Tree::get`] when that is possible.
    pub fn node(&self, id: NodeId) -> &TreeNode {
        self.get(id).expect("node was pruned")
    }

    /// Mutable access to a live node; panics like [`Tree::node`].
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
    /// Returns `None` if `parent` was pruned, the slot holds no bud, or
    /// `habitat` reports the new node's tip as occupied; a blocked bud stays
    /// in its slot, so it can grow later in another direction. Panics if
    /// `slot` is `Lateral(i)` with `i > 1`.
    /// The new node gets fresh buds placed by phyllotaxis and inherits the
    /// parent's species. `direction` need not be unit length but must not be zero.
    pub fn grow(
        &mut self,
        habitat: &(impl Habitat + ?Sized),
        parent: NodeId,
        slot: SlotIndex,
        direction: Vec3,
        length: f32,
    ) -> Option<NodeId> {
        let id = NodeId(u32::try_from(self.nodes.len()).expect("too many nodes"));
        let step = self.step;
        let p = self.get_mut(parent)?;
        p.slot(slot).bud()?;
        let heading = direction.normalize();
        let position = p.position + heading * length;
        if habitat.occupied(position) {
            return None;
        }
        *p.slot_mut(slot) = Slot::Child(id);
        let order = match slot {
            SlotIndex::Main => p.order,
            SlotIndex::Lateral(_) => p.order + 1,
        };
        let child = TreeNode::new(
            Arc::clone(&p.species),
            Some(parent),
            position,
            heading,
            p.heading,
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
    ///
    /// This is the user's edit, so unlike [`Tree::shed`] it leaves no stump,
    /// and it can remove a stump.
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
        self.remove_subtrees(vec![id])
    }

    /// Sheds the branch at `id` (self-pruning), leaving node `id` as a stump.
    ///
    /// Everything that grew from `id` is removed, and `id` itself is kept
    /// with [`TreeNode::stump`] set and all its slots emptied, so it never
    /// regrows. Stumps are kept for appearance (a shed branch leaves a
    /// visible stub) and for the pipe model: the stump keeps its radius, so
    /// the trunk below stays as thick as the branches it once carried.
    ///
    /// The stump is shortened to just reach out of its parent's bark: with θ
    /// the angle between the two headings, its length becomes
    /// `parent.radius / sin θ`, the distance along its heading from the
    /// parent's axis to the surface of a cylinder of the parent's radius.
    /// It is never lengthened, which also covers θ near 0 (a shed continuing
    /// axis), where that distance goes to infinity. The stump's base stays
    /// at the parent's tip; its tip moves in.
    ///
    /// Returns how many nodes were removed, not counting the stump. Returns
    /// 0 and changes nothing if `id` is missing, the root, or already a stump.
    pub fn shed(&mut self, id: NodeId) -> usize {
        let Some(node) = self.get(id).filter(|n| !n.stump) else {
            return 0;
        };
        let Some(parent) = node.parent.map(|p| self.node(p)) else {
            return 0;
        };
        let sin = node.heading.cross(parent.heading).length();
        let length = if parent.radius < node.length * sin {
            parent.radius / sin
        } else {
            node.length
        };

        let node = self.node_mut(id);
        let children: Vec<NodeId> = node.children().collect();
        let base = node.base();
        node.position = base + node.heading * length;
        node.length = length;
        node.main = Slot::Empty;
        node.laterals = [Slot::Empty, Slot::Empty];
        node.internode_count = 1;
        node.stump = true;
        self.remove_subtrees(children)
    }

    /// Removes the subtrees rooted at `roots`, retiring their ids, and
    /// returns how many nodes were removed. Callers fix up the parents' slots.
    fn remove_subtrees(&mut self, mut stack: Vec<NodeId>) -> usize {
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
    use crate::habitat::OpenAir;
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
        let a = t.grow(&OpenAir, root, SlotIndex::Main, dir, 1.0).unwrap();
        let b = t
            .grow(&OpenAir, root, SlotIndex::Lateral(0), Vec3::X, 1.0)
            .unwrap();
        assert!(
            t.grow(&OpenAir, root, SlotIndex::Main, dir, 1.0).is_none(),
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
        let a = t
            .grow(&OpenAir, root, SlotIndex::Main, Vec3::Y, 1.0)
            .unwrap();
        let b = t.grow(&OpenAir, a, SlotIndex::Main, Vec3::Y, 1.0).unwrap();
        t.grow(&OpenAir, a, SlotIndex::Lateral(0), Vec3::X, 1.0)
            .unwrap();
        t.grow(&OpenAir, b, SlotIndex::Main, Vec3::Y, 1.0).unwrap();

        let counts = t.fold_up(|_, kids| 1 + kids.sum::<u32>());
        assert_eq!(counts[root.index()], Some(5));

        assert_eq!(t.prune(a), 4);
        assert_eq!(t.len(), 1);
        assert!(t.get(b).is_none());
        assert_eq!(t.node(root).main, Slot::Empty);
        assert_eq!(t.fold_up(|_, kids| 1 + kids.sum::<u32>())[0], Some(1));
    }

    /// Grows every bud for `steps` steps along its default direction.
    fn bushy(steps: u32) -> Tree {
        let mut t = tree(TreeSpecies::default());
        for _ in 0..steps {
            let buds: Vec<_> = t
                .iter()
                .flat_map(|(id, n)| n.buds().map(move |(s, b)| (id, s, b.direction)))
                .collect();
            for (id, slot, dir) in buds {
                t.grow(&OpenAir, id, slot, dir, 1.0);
            }
            t.advance_step();
        }
        t
    }

    #[test]
    fn seedling_stands_on_origin() {
        let origin = Vec3::new(3.0, -1.0, 2.0);
        let t = Tree::new(Arc::new(TreeSpecies::default()), 0, origin).unwrap();
        let root = t.node(t.root());
        assert!((root.base() - origin).length() < 1e-6);
        assert_eq!(root.heading, Vec3::Y);
    }

    #[test]
    fn internodes_connect_and_record_order_and_age() {
        let t = bushy(4);
        for (id, n) in t.iter() {
            let Some(p) = n.parent else { continue };
            let p = t.node(p);
            assert!(p.children().any(|c| c == id));
            // Each internode starts exactly at its parent's tip.
            assert!((n.base() - p.position).length() < 1e-5);
            let is_main = p.main.child() == Some(id);
            assert_eq!(n.order, p.order + u16::from(!is_main));
            assert!(n.birth_step >= p.birth_step);
        }
    }

    #[test]
    fn fold_up_matches_recursion_after_pruning() {
        fn count(t: &Tree, id: NodeId) -> u32 {
            1 + t.node(id).children().map(|c| count(t, c)).sum::<u32>()
        }
        let mut t = bushy(5);
        let victim = t.iter().nth(3).unwrap().0;
        t.prune(victim);
        let folded = t.fold_up(|_, kids| 1 + kids.sum::<u32>());
        for (id, _) in t.iter() {
            assert_eq!(folded[id.index()], Some(count(&t, id)));
        }
        assert_eq!(folded[t.root().index()], Some(t.len() as u32));
    }

    #[test]
    fn fold_up_pipe_model_conserves_cross_section() {
        // rⁿ = Σ r_iⁿ (Pałubicki 2009), so the root's rⁿ is the number of
        // tips times r_tipⁿ, for any pipe exponent.
        let t = bushy(4);
        let tips = t.iter().filter(|(_, n)| n.is_terminal()).count() as f32;
        let r_tip = t.species().thickness.tip_radius;
        for n in [2.0f32, 2.5, 3.0] {
            let radii = t.fold_up(|node, kids| {
                let sum: f32 = kids.map(|r: &f32| r.powf(n)).sum();
                if node.is_terminal() {
                    r_tip
                } else {
                    sum.powf(1.0 / n)
                }
            });
            let root = radii[t.root().index()].unwrap();
            assert!((root.powf(n) / (tips * r_tip.powf(n)) - 1.0).abs() < 1e-4);
            for (id, node) in t.iter() {
                let r = radii[id.index()].unwrap();
                assert!(node.children().all(|c| radii[c.index()].unwrap() <= r));
            }
        }
    }

    #[test]
    fn ids_are_never_reused_after_prune() {
        let mut t = tree(TreeSpecies::default());
        let root = t.root();
        let a = t
            .grow(&OpenAir, root, SlotIndex::Main, Vec3::Y, 1.0)
            .unwrap();
        t.prune(a);
        // The pruned slot is empty, not a bud, so it cannot regrow.
        assert!(
            t.grow(&OpenAir, root, SlotIndex::Main, Vec3::Y, 1.0)
                .is_none()
        );
        let b = t
            .grow(&OpenAir, root, SlotIndex::Lateral(0), Vec3::X, 1.0)
            .unwrap();
        assert_ne!(a, b);
        assert!(t.get(a).is_none());
        assert_eq!(t.prune(root), 0, "root cannot be pruned");
    }

    /// A trunk `root → a → b` with a lateral `l` on `a` that has two
    /// internodes of its own, at 60° to the trunk. Returns `[a, b, l]`.
    fn forked() -> (Tree, [NodeId; 3]) {
        let mut t = tree(TreeSpecies::default());
        let root = t.root();
        let a = t
            .grow(&OpenAir, root, SlotIndex::Main, Vec3::Y, 1.0)
            .unwrap();
        let b = t.grow(&OpenAir, a, SlotIndex::Main, Vec3::Y, 1.0).unwrap();
        let dir = Vec3::new(3.0f32.sqrt(), 1.0, 0.0);
        let l = t
            .grow(&OpenAir, a, SlotIndex::Lateral(0), dir, 1.0)
            .unwrap();
        let m = t.grow(&OpenAir, l, SlotIndex::Main, dir, 1.0).unwrap();
        t.grow(&OpenAir, m, SlotIndex::Lateral(0), Vec3::Z, 1.0)
            .unwrap();
        t.node_mut(a).radius = 0.1;
        t.node_mut(l).radius = 0.05;
        (t, [a, b, l])
    }

    #[test]
    fn shed_removes_descendants_and_leaves_a_stump() {
        let (mut t, [a, _, l]) = forked();
        let before = t.len();
        let m = t.node(l).main.child().unwrap();
        assert_eq!(t.shed(l), 2);
        assert_eq!(t.len(), before - 2);
        assert!(t.get(m).is_none());

        let stump = t.node(l);
        assert!(stump.stump && stump.is_terminal());
        assert_eq!(stump.radius, 0.05, "radius is kept");
        assert_eq!(stump.internode_count, 1);
        assert!(stump.slots().all(|(_, s)| *s == Slot::Empty));
        assert!(stump.laterals.iter().all(|s| *s == Slot::Empty));
        assert_eq!(t.node(a).laterals[0], Slot::Child(l), "parent keeps it");
        assert!(
            t.grow(&OpenAir, l, SlotIndex::Main, Vec3::Y, 1.0).is_none(),
            "a stump never regrows"
        );
    }

    #[test]
    fn stump_just_reaches_parent_bark() {
        let (mut t, [a, _, l]) = forked();
        let base = t.node(l).base();
        let heading = t.node(l).heading;
        t.shed(l);
        let stump = t.node(l);
        // 60° from the trunk: r / sin 60°.
        let expected = 0.1 / 60f32.to_radians().sin();
        assert!((stump.length - expected).abs() < 1e-5);
        assert!(stump.length <= 1.0);
        assert!((stump.base() - base).length() < 1e-5, "base stays put");
        assert!((stump.position - (base + heading * expected)).length() < 1e-5);
        // The tip is exactly the parent's radius away from the parent's axis.
        let p = t.node(a);
        let off = stump.position - p.position;
        assert!(((off - p.heading * off.dot(p.heading)).length() - 0.1).abs() < 1e-5);
    }

    #[test]
    fn stump_is_never_lengthened() {
        // Shedding the continuing axis: parallel headings, so r / sin θ is
        // unbounded and the original length is kept.
        let (mut t, [a, b, _]) = forked();
        t.shed(b);
        assert_eq!(t.node(b).length, 1.0);
        // A parent thicker than the branch is long also keeps the length.
        let (mut t, [_, _, l]) = forked();
        t.node_mut(a).radius = 5.0;
        t.shed(l);
        assert_eq!(t.node(l).length, 1.0);
    }

    #[test]
    fn shed_refuses_root_stumps_and_missing_nodes() {
        let (mut t, [_, _, l]) = forked();
        let root = t.root();
        assert_eq!(t.shed(root), 0, "root cannot be shed");
        assert!(!t.node(root).stump);
        t.shed(l);
        let (len, stump) = (t.len(), t.node(l).clone());
        assert_eq!(t.shed(l), 0, "a stump cannot be shed again");
        assert_eq!(t.len(), len);
        assert_eq!(t.node(l).length, stump.length);
        assert_eq!(t.node(l).position, stump.position);
        let m = NodeId(l.0 + 1);
        assert!(t.get(m).is_none());
        assert_eq!(t.shed(m), 0, "already removed");
        assert_eq!(t.shed(NodeId(999)), 0, "never existed");
        // A user prune still removes the stump itself.
        assert_eq!(t.prune(l), 1);
    }

    #[test]
    fn ids_are_never_reused_after_shed() {
        let (mut t, [_, b, l]) = forked();
        let m = t.node(l).main.child().unwrap();
        t.shed(l);
        let c = t.grow(&OpenAir, b, SlotIndex::Main, Vec3::Y, 1.0).unwrap();
        assert!(c.index() > m.index(), "fresh id, not the shed one");
        assert!(t.get(m).is_none());
    }

    #[test]
    fn fold_up_sees_stump_radius() {
        // A pipe-model fold that uses each node's stored radius at the tips:
        // the stump's cross-section still reaches the trunk.
        let (mut t, [a, b, l]) = forked();
        t.prune(b);
        t.shed(l);
        let radii = t.fold_up(|node, kids| {
            let sum: f32 = kids.map(|r: &f32| r * r).sum();
            if node.is_terminal() {
                node.radius
            } else {
                sum.sqrt()
            }
        });
        assert_eq!(radii[l.index()], Some(0.05));
        assert!((radii[a.index()].unwrap() - 0.05).abs() < 1e-6);
        assert!((radii[t.root().index()].unwrap() - 0.05).abs() < 1e-6);
        let counts = t.fold_up(|_, kids| 1 + kids.sum::<u32>());
        assert_eq!(counts[t.root().index()], Some(3));
    }

    #[test]
    fn grow_refuses_occupied_space_and_keeps_the_bud() {
        let mut t = tree(TreeSpecies::default());
        let root = t.root();
        let tip_y = t.node(root).position.y;
        // A ceiling just above the seedling's tip.
        let ceiling = move |p: Vec3| p.y > tip_y + 0.5;
        assert!(
            t.grow(&ceiling, root, SlotIndex::Main, Vec3::Y, 1.0)
                .is_none()
        );
        assert!(t.node(root).main.bud().is_some(), "blocked bud stays put");
        assert_eq!(t.len(), 1);
        // The same bud can still grow sideways under the ceiling.
        let a = t
            .grow(&ceiling, root, SlotIndex::Main, Vec3::X, 1.0)
            .unwrap();
        assert!(!ceiling(t.node(a).position));
    }

    #[test]
    fn validation_rejects_bad_weights() {
        let mut s = TreeSpecies::conifer();
        assert!(s.validate().is_ok());
        s.tropism.environment = 0.5;
        assert!(Tree::new(Arc::new(s), 0, Vec3::ZERO).is_err());
    }
}

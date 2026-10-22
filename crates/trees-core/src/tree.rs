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

use std::collections::BTreeMap;
use std::sync::Arc;

use glam::Vec3;

use crate::foliage::{Leaf, place_leaves};
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
    leaves: Vec<Leaf>,
    /// The user's width pins: absolute radii that [`Tree::update_radii`]
    /// holds exactly. Only live, non-stump nodes are pinned.
    pins: BTreeMap<NodeId, f32>,
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
        let mut tree = Tree {
            species,
            seed,
            step: 0,
            nodes: vec![Some(root)],
            live: 1,
            leaves: Vec::new(),
            pins: BTreeMap::new(),
        };
        tree.update_leaves();
        Ok(tree)
    }

    /// Stored leaf attachments and initial frames, in axis/station/pair order.
    /// Refreshed by growth, prune, shed and radius updates. Direct edits through
    /// [`Tree::get_mut`] or [`Tree::node_mut`] require [`Tree::update_radii`]
    /// (or [`Tree::update_leaves`] if radii are already current).
    pub fn leaves(&self) -> &[Leaf] {
        &self.leaves
    }

    /// Rebuilds stored foliage from the current skeleton and radii, without
    /// changing buds or branches. Longay 2014 C.1.1 supplies the thin-branch
    /// rule; distance sampling and stable identities are project conventions.
    pub fn update_leaves(&mut self) {
        self.leaves = place_leaves(self);
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
    /// After geometry edits, call [`Tree::update_radii`] to refresh foliage too.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut TreeNode> {
        self.nodes.get_mut(id.index())?.as_mut()
    }

    /// Panics if `id` was pruned; use [`Tree::get`] when that is possible.
    pub fn node(&self, id: NodeId) -> &TreeNode {
        self.get(id).expect("node was pruned")
    }

    /// Mutable access to a live node; panics like [`Tree::node`].
    /// After geometry edits, call [`Tree::update_radii`] to refresh foliage too.
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

    /// The slots leading from the root to `id`, or `None` if `id` was pruned
    /// or shed. The root's path is empty.
    ///
    /// Unlike a [`NodeId`], a path names the same branch in a tree regrown
    /// with a different seed or species, wherever such a branch exists.
    pub fn slot_path(&self, id: NodeId) -> Option<Vec<SlotIndex>> {
        let mut path = Vec::new();
        let mut node = self.get(id)?;
        let mut id = id;
        while let Some(parent) = node.parent {
            node = self.node(parent);
            let (slot, _) = node
                .slots()
                .find(|(_, s)| s.child() == Some(id))
                .expect("parent holds its child");
            path.push(slot);
            id = parent;
        }
        path.reverse();
        Some(path)
    }

    /// The node reached by following `path` from the root, or `None` if
    /// some slot on the way holds no child; see [`Tree::slot_path`].
    pub fn node_at(&self, path: &[SlotIndex]) -> Option<NodeId> {
        path.iter().try_fold(self.root(), |id, &slot| match slot {
            SlotIndex::Lateral(i) if i > 1 => None,
            _ => self.node(id).slot(slot).child(),
        })
    }

    /// Radius at the base of a straight tapered internode. A main child shares
    /// its parent's tip ring; a root or lateral axis starts at its own radius.
    /// Shared by branch geometry and foliage so attachments stay on the bark.
    /// Panics for a missing node, like [`Tree::node`].
    pub fn base_radius(&self, id: NodeId) -> f32 {
        let node = self.node(id);
        match node.parent.map(|p| self.node(p)) {
            Some(parent) if parent.main.child() == Some(id) => parent.radius,
            _ => node.radius,
        }
    }

    /// Grows the bud in `slot` of `parent` into a new internode.
    ///
    /// Returns `None` if `parent` was pruned, the slot holds no bud, or
    /// `habitat` reports the new node's tip as occupied; a blocked bud stays
    /// in its slot, so it can grow later in another direction. Panics if
    /// `slot` is `Lateral(i)` with `i > 1`.
    /// The new node gets fresh buds placed by phyllotaxis and inherits the
    /// parent's species. Returns `None` for a zero/non-finite direction, a
    /// negative/non-finite length, or a non-finite resulting position; no bud
    /// is consumed. Zero length is allowed but produces no leaf attachments.
    pub fn grow(
        &mut self,
        habitat: &(impl Habitat + ?Sized),
        parent: NodeId,
        slot: SlotIndex,
        direction: Vec3,
        length: f32,
    ) -> Option<NodeId> {
        if !length.is_finite() || length < 0.0 || !direction.is_finite() {
            return None;
        }
        let heading = direction.try_normalize()?;
        let id = NodeId(u32::try_from(self.nodes.len()).expect("too many nodes"));
        let step = self.step;
        let p = self.get_mut(parent)?;
        p.slot(slot).bud()?;
        let position = p.position + heading * length;
        if !position.is_finite() || habitat.occupied(position) {
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
        self.update_leaves();
        Some(id)
    }

    /// Removes the subtree rooted at `id` and empties its parent's slot.
    /// Returns how many nodes were removed.
    ///
    /// This is the user's edit, so unlike [`Tree::shed`] it leaves no stump.
    /// Returns 0 and changes nothing if `id` is missing, the root, or a stump:
    /// stumps are permanent, since they are the pipe model's memory of shed
    /// branches (see [`Tree::update_radii`]). Pruning an ancestor still
    /// removes the stumps inside that limb.
    pub fn prune(&mut self, id: NodeId) -> usize {
        let Some(parent) = self.get(id).filter(|n| !n.stump).and_then(|n| n.parent) else {
            return 0;
        };
        let p = self.node_mut(parent);
        for slot in std::iter::once(&mut p.main).chain(p.laterals.iter_mut()) {
            if slot.child() == Some(id) {
                *slot = Slot::Empty;
            }
        }
        let removed = self.remove_subtrees(vec![id]);
        self.update_leaves();
        removed
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
        self.pins.remove(&id);
        let removed = self.remove_subtrees(children);
        self.update_leaves();
        removed
    }

    /// Pins node `id` to radius `r` (Longay 2014 §5.3.2), replacing any pin
    /// it had. The pin is absolute: [`Tree::update_radii`] gives the node
    /// exactly `r` however the tree grows, by solving its subtree's pipe
    /// exponent anew on every call.
    ///
    /// Returns false and changes nothing if `id` is missing or a stump, or
    /// `r` is not a positive finite number. Pruning or shedding the node or
    /// an ancestor drops the pin.
    pub fn pin_radius(&mut self, id: NodeId, r: f32) -> bool {
        if !(r.is_finite() && r > 0.0) || self.get(id).is_none_or(|n| n.stump) {
            return false;
        }
        self.pins.insert(id, r);
        true
    }

    /// Removes the pin on `id`, returning its radius if it had one.
    pub fn unpin(&mut self, id: NodeId) -> Option<f32> {
        self.pins.remove(&id)
    }

    /// The width pins as (node, radius) pairs, in id order.
    pub fn pins(&self) -> impl Iterator<Item = (NodeId, f32)> + '_ {
        self.pins.iter().map(|(&id, &r)| (id, r))
    }

    /// The pipe exponent [`Tree::update_radii`] uses inside the subtree of
    /// the pinned node `id`, or `None` if `id` is not pinned. This is the
    /// species' exponent when no exponent makes the pin hold.
    ///
    /// Solved from the tree as it is now, so it is current even before the
    /// next [`Tree::update_radii`].
    pub fn pin_exponent(&self, id: NodeId) -> Option<f32> {
        let &r = self.pins.get(&id)?;
        let n = pipe_exponent(&self.boundary(id), r);
        Some(n.map_or(self.species.thickness.pipe_exponent, |n| n as f32))
    }

    /// Sets every node's radius by the pipe model, rⁿ = Σ r_iⁿ (Hart 2020
    /// §9.8), in one tip-to-base pass, holding every width pin exactly.
    ///
    /// A living node with no children has the species' tip radius. A stump
    /// keeps the radius it had when it was shed and is never written, so the
    /// cross-section of a shed branch still reaches the trunk below it. Since
    /// stumps are permanent, they are Pałubicki 2009 §4.5's "memory of past
    /// branches" with no extra bookkeeping, and without pins a radius never
    /// decreases as the tree grows and sheds.
    ///
    /// Each pinned node P bounds a subtree, whose boundary elements are its
    /// living tips, its stumps and the pinned nodes below it, each with its
    /// radius r_i. Inside that subtree n is solved numerically so that
    /// Σ r_iⁿ = r_Pⁿ (Longay 2014 Eq. 5.2; boundary radii differ, so his
    /// closed form Eq. 5.3 does not apply), and above it P counts as a tip of
    /// radius r_P. Outside all pins n is the species' pipe exponent. A solved
    /// n is not limited to the species' range of 2 to 3, but it is capped at
    /// 64, which a pin only needs when it is within about 1% of its thickest
    /// boundary element; the pin still holds then.
    ///
    /// No n > 0 solves a pin no thicker than its thickest boundary element,
    /// or one whose subtree has fewer than two. Such a subtree uses the
    /// species' n, and the pinned node gets the larger of its pin and its
    /// thickest child, so a pin can widen a branch but never pinch it.
    pub fn update_radii(&mut self) {
        let thickness = &self.species.thickness;
        let species_n = f64::from(thickness.pipe_exponent);
        let tip = thickness.tip_radius;
        // The exponent solved inside each pin's subtree, if one exists.
        let solved: BTreeMap<NodeId, Option<f64>> = self
            .pins
            .iter()
            .map(|(&id, &r)| (id, pipe_exponent(&self.boundary(id), r)))
            .collect();
        // The exponent each node's parent sums it with (base to tip): that of
        // the nearest pinned ancestor's subtree, or the species'.
        let mut exponent = vec![species_n; self.nodes.len()];
        for (id, node) in self.iter() {
            if let Some(p) = node.parent {
                exponent[id.index()] = match solved.get(&p) {
                    Some(n) => n.unwrap_or(species_n),
                    None => exponent[p.index()],
                };
            }
        }
        // Each node's radius and its (r / r_tip)ⁿ in its own exponent, tip to
        // base. Scaling by the tip radius keeps large exponents in range.
        let mut radius = vec![0.0f32; self.nodes.len()];
        let mut power = vec![0.0f64; self.nodes.len()];
        for (id, node) in self.iter().rev() {
            let (i, n) = (id.index(), exponent[id.index()]);
            // A stump is checked first: it is terminal too.
            let r = if node.stump {
                node.radius
            } else if let Some(&pin) = self.pins.get(&id) {
                if solved[&id].is_some() {
                    pin
                } else {
                    node.children()
                        .map(|c| radius[c.index()])
                        .fold(pin, f32::max)
                }
            } else if node.is_terminal() {
                tip
            } else {
                // Not pinned, so its children share its exponent.
                power[i] = node.children().map(|c| power[c.index()]).sum();
                radius[i] = root_up(power[i], n, tip);
                continue;
            };
            radius[i] = r;
            power[i] = scaled_power(r, n, tip);
        }
        for (id, r) in radius.into_iter().enumerate() {
            if let Some(node) = &mut self.nodes[id]
                && !node.stump
            {
                node.radius = r;
            }
        }
        self.update_leaves();
    }

    /// Radii of the boundary elements of the subtree below the pinned node
    /// `pinned`: its living tips, its stumps and the pinned nodes below it.
    fn boundary(&self, pinned: NodeId) -> Vec<f32> {
        let tip = self.species.thickness.tip_radius;
        let mut radii = Vec::new();
        let mut stack: Vec<NodeId> = self.node(pinned).children().collect();
        while let Some(id) = stack.pop() {
            let node = self.node(id);
            if node.stump {
                radii.push(node.radius);
            } else if let Some(&r) = self.pins.get(&id) {
                radii.push(r);
            } else if node.is_terminal() {
                radii.push(tip);
            } else {
                stack.extend(node.children());
            }
        }
        radii
    }

    /// Removes the subtrees rooted at `roots`, retiring their ids and their
    /// pins, and returns how many nodes were removed. Callers fix up the
    /// parents' slots.
    fn remove_subtrees(&mut self, mut stack: Vec<NodeId>) -> usize {
        let mut removed = 0;
        while let Some(n) = stack.pop() {
            if let Some(node) = self.nodes[n.index()].take() {
                self.pins.remove(&n);
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

/// Upper end of the bracket for a solved pipe exponent.
const MAX_PIPE_EXPONENT: f64 = 64.0;

/// The pipe exponent n > 0 with Σ r_iⁿ = targetⁿ (Longay 2014 Eq. 5.2), or
/// `None` if there is none: fewer than two radii, or `target` no larger than
/// the largest. Capped at [`MAX_PIPE_EXPONENT`].
///
/// With x_i = ln(r_i / target) < 0, f(n) = Σ e^(n·x_i) − 1 falls strictly
/// from k − 1 > 0 at n = 0 to −1, so the root is unique. Newton–Raphson
/// finds it, falling back to bisection whenever a step leaves the bracket.
/// Dividing by `target` means nothing can overflow.
fn pipe_exponent(radii: &[f32], target: f32) -> Option<f64> {
    let target = f64::from(target);
    let logs: Vec<f64> = radii
        .iter()
        .map(|&r| (f64::from(r) / target).ln())
        .collect();
    if logs.len() < 2 || logs.iter().any(|&x| x >= 0.0) {
        return None;
    }
    let f = |n: f64| logs.iter().map(|&x| (n * x).exp()).sum::<f64>() - 1.0;
    if f(MAX_PIPE_EXPONENT) >= 0.0 {
        return Some(MAX_PIPE_EXPONENT);
    }
    // Eq. 5.3, n = ln k / ln(target / r), with r the radii's geometric mean,
    // is exact when all radii are equal and a good start otherwise.
    let mean = logs.iter().sum::<f64>() / logs.len() as f64;
    let (mut lo, mut hi) = (0.0, MAX_PIPE_EXPONENT);
    let mut n = ((logs.len() as f64).ln() / -mean).clamp(lo, hi);
    for _ in 0..100 {
        let (value, slope) = logs.iter().fold((-1.0, 0.0), |(v, d), &x| {
            let e = (n * x).exp();
            (v + e, d + x * e)
        });
        if value == 0.0 {
            break;
        } else if value > 0.0 {
            lo = n;
        } else {
            hi = n;
        }
        let newton = n - value / slope;
        let next = if (lo..=hi).contains(&newton) {
            newton
        } else {
            0.5 * (lo + hi)
        };
        let step = (next - n).abs();
        n = next;
        if step <= 1e-12 * n.max(1.0) {
            break;
        }
    }
    Some(n)
}

/// (r / tip)ⁿ: a cross-section in units of the tip's, in the pipe model.
fn scaled_power(r: f32, n: f64, tip: f32) -> f64 {
    (f64::from(r) / f64::from(tip)).powf(n)
}

/// The smallest `f32` r with [`scaled_power`]`(r) ≥ power`. Rounding up,
/// not to nearest, means a stump frozen at r still contributes at least the
/// cross-section it carried, so shedding can never thin the nodes below it
/// by an ulp.
fn root_up(power: f64, n: f64, tip: f32) -> f32 {
    let r = (f64::from(tip) * power.powf(n.recip())) as f32;
    if scaled_power(r, n, tip) < power {
        r.next_up()
    } else {
        r
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
    fn invalid_growth_keeps_buds_and_foliage_unchanged() {
        let mut t = tree(TreeSpecies::default());
        let before = t.leaves().to_vec();
        for (direction, length) in [
            (Vec3::ZERO, 1.0),
            (Vec3::splat(f32::NAN), 1.0),
            (Vec3::splat(f32::INFINITY), 1.0),
            (Vec3::Y, -1.0),
            (Vec3::Y, f32::NAN),
            (Vec3::Y, f32::INFINITY),
        ] {
            assert_eq!(
                t.grow(&OpenAir, t.root(), SlotIndex::Main, direction, length),
                None
            );
            assert_eq!(t.len(), 1);
            assert!(t.node(t.root()).main.bud().is_some());
            assert_eq!(t.leaves(), before);
        }
        assert!(
            t.grow(&OpenAir, t.root(), SlotIndex::Main, Vec3::Y, 1.0)
                .is_some()
        );
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
            grow_all(&mut t);
        }
        t
    }

    /// One step in which every bud grows along its default direction.
    fn grow_all(t: &mut Tree) {
        let buds: Vec<_> = t
            .iter()
            .flat_map(|(id, n)| n.buds().map(move |(s, b)| (id, s, b.direction)))
            .collect();
        for (id, slot, dir) in buds {
            t.grow(&OpenAir, id, slot, dir, 1.0);
        }
        t.advance_step();
    }

    #[test]
    fn slot_paths_round_trip_and_survive_regrowth() {
        let mut t = bushy(4);
        assert_eq!(t.slot_path(t.root()), Some(vec![]));
        for (id, _) in t.iter() {
            assert_eq!(t.node_at(&t.slot_path(id).unwrap()), Some(id));
        }
        let (a, b) = (SlotIndex::Main, SlotIndex::Lateral(0));
        let id = t.node_at(&[a, b, a]).unwrap();
        // The same path names the same branch in an identically grown tree,
        // even after an unrelated prune retired other ids.
        let other = bushy(4);
        assert_eq!(other.node_at(&[a, b, a]), Some(id));
        t.prune(t.node_at(&[b]).unwrap());
        assert_eq!(t.node_at(&[a, b, a]), Some(id));
        assert_eq!(t.node_at(&[b]), None, "pruned");
        assert_eq!(t.node_at(&[SlotIndex::Lateral(1)]), None, "unused slot");
        assert_eq!(t.node_at(&[SlotIndex::Lateral(7)]), None, "no such slot");
        assert_eq!(t.slot_path(NodeId(9999)), None);
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
    fn shedding_the_main_child_promotes_no_lateral() {
        let (mut t, [a, b, l]) = forked();
        t.shed(b);
        let p = t.node(a);
        assert_eq!(p.main, Slot::Child(b), "the stump stays the continuation");
        assert_eq!(p.laterals[0], Slot::Child(l));
        assert_eq!(t.node(l).order, p.order + 1);
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
    }

    #[test]
    fn prune_refuses_stumps_but_not_their_ancestors() {
        let (mut t, [a, _, l]) = forked();
        t.shed(l);
        let len = t.len();
        assert_eq!(t.prune(l), 0, "stumps are never removed");
        assert_eq!(t.len(), len);
        assert!(t.node(l).stump);
        assert_eq!(t.node(a).laterals[0], Slot::Child(l));
        // Pruning the limb that carries the stump removes it with the limb.
        assert_eq!(t.prune(a), 3);
        assert!(t.get(l).is_none());
    }

    fn approx(a: f32, b: f32) -> bool {
        (a / b - 1.0).abs() < 1e-4
    }

    #[test]
    fn update_radii_conserves_cross_section_with_stumps() {
        // Root rⁿ = (living tips)·r_tipⁿ + Σ stump rⁿ.
        let mut t = bushy(5);
        t.update_radii();
        let victims: Vec<_> = t
            .iter()
            .filter(|(_, n)| n.order == 1)
            .map(|(id, _)| id)
            .collect();
        for id in victims.into_iter().step_by(3) {
            t.shed(id);
        }
        t.update_radii();
        let n = t.species().thickness.pipe_exponent;
        let r_tip = t.species().thickness.tip_radius;
        let tips = t
            .iter()
            .filter(|(_, x)| x.is_terminal() && !x.stump)
            .count() as f32;
        let stumps: f32 = t
            .iter()
            .filter(|(_, x)| x.stump)
            .map(|(_, x)| x.radius.powf(n))
            .sum();
        assert!(stumps > 0.0);
        let root = t.node(t.root()).radius;
        assert!(approx(root.powf(n), tips * r_tip.powf(n) + stumps));
        for (_, node) in t.iter() {
            let sum: f32 = node.children().map(|c| t.node(c).radius.powf(n)).sum();
            if !node.is_terminal() {
                assert!(approx(node.radius.powf(n), sum));
            }
        }
    }

    #[test]
    fn update_radii_keeps_stump_radius() {
        let (mut t, [_, _, l]) = forked();
        t.shed(l);
        t.update_radii();
        assert_eq!(t.node(l).radius, 0.05);
    }

    #[test]
    fn radii_never_decrease_through_shedding_and_growth() {
        let mut t = bushy(4);
        t.update_radii();
        for round in 0..6u32 {
            let before: Vec<Option<f32>> = t
                .nodes
                .iter()
                .map(|n| n.as_ref().map(|n| n.radius))
                .collect();
            // Shed a few living, non-root branches, picked by position in the arena.
            let candidates: Vec<_> = t
                .iter()
                .filter(|(id, n)| !n.stump && *id != t.root())
                .map(|(id, _)| id)
                .collect();
            for id in candidates.iter().skip(round as usize + 2).step_by(7) {
                t.shed(*id);
            }
            t.update_radii();
            grow_all(&mut t);
            t.update_radii();
            for (id, node) in t.iter() {
                if let Some(Some(old)) = before.get(id.index()) {
                    assert!(
                        node.radius >= *old,
                        "node {id:?} thinned in round {round}: {old} -> {}",
                        node.radius
                    );
                }
            }
        }
        assert!(t.iter().any(|(_, n)| n.stump));
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
    fn pin_holds_exactly_through_growth() {
        let mut t = bushy(3);
        let a = t.node(t.root()).main.child().unwrap();
        assert!(t.pin_radius(a, 0.3));
        t.update_radii();
        assert!(pipe_exponent(&t.boundary(a), 0.3).is_some());
        assert_eq!(t.node(a).radius, 0.3);
        for _ in 0..3 {
            grow_all(&mut t);
            t.update_radii();
            assert_eq!(t.node(a).radius, 0.3);
        }
        assert_eq!(t.pins().collect::<Vec<_>>(), vec![(a, 0.3)]);
    }

    #[test]
    fn root_pin_reproduces_longay_eq_5_3() {
        // With M equal tips and the pin at the root, n = log M / (log d_b −
        // log d_e) for base and tip diameters d_b and d_e.
        let mut t = bushy(4);
        let (root, r_root) = (t.root(), 0.5);
        let r_tip = t.species().thickness.tip_radius;
        let tips_below = t.fold_up(|node, kids| u32::from(node.is_terminal()) + kids.sum::<u32>());
        let m = f64::from(tips_below[root.index()].unwrap());
        let d = |r: f32| (2.0 * f64::from(r)).ln();
        let expected = m.ln() / (d(r_root) - d(r_tip));
        assert!(t.pin_radius(root, r_root));
        let n = pipe_exponent(&t.boundary(root), r_root).unwrap();
        assert!((n - expected).abs() < 1e-9);
        assert!(!(2.0..=3.0).contains(&n), "a solved n is not clamped: {n}");
        t.update_radii();
        assert_eq!(t.node(root).radius, r_root);
        for (id, node) in t.iter().skip(1) {
            let tips = f64::from(tips_below[id.index()].unwrap());
            let r = f64::from(r_tip) * tips.powf(n.recip());
            assert!(approx(node.radius, r as f32));
        }
    }

    #[test]
    fn pin_exponent_reports_solved_or_species_n() {
        let mut t = bushy(4);
        let root = t.root();
        assert_eq!(t.pin_exponent(root), None, "not pinned");
        t.pin_radius(root, 0.5);
        let solved = pipe_exponent(&t.boundary(root), 0.5).unwrap();
        assert_eq!(t.pin_exponent(root), Some(solved as f32));
        // Too thin to solve: the subtree keeps the species' n.
        t.pin_radius(root, 0.01);
        let species_n = t.species().thickness.pipe_exponent;
        assert_eq!(t.pin_exponent(root), Some(species_n));
        t.unpin(root);
        assert_eq!(t.pin_exponent(root), None);
    }

    #[test]
    fn nested_pins_both_hold() {
        let mut t = bushy(5);
        let root = t.root();
        let q = t.node(root).laterals[0].child().unwrap();
        assert!(!t.node(q).is_terminal());
        assert!(t.pin_radius(root, 0.6) && t.pin_radius(q, 0.15));
        t.update_radii();
        assert_eq!(t.node(root).radius, 0.6);
        assert_eq!(t.node(q).radius, 0.15);
        // The lower pin is a boundary element of the upper one, and the
        // root's subtree still obeys the pipe model with its own n.
        assert!(t.boundary(root).contains(&0.15));
        let n = pipe_exponent(&t.boundary(root), 0.6).unwrap();
        let sum: f64 = t
            .node(root)
            .children()
            .map(|c| f64::from(t.node(c).radius).powf(n))
            .sum();
        assert!((sum / 0.6f64.powf(n) - 1.0).abs() < 1e-5);
        // Inside q's subtree the n solved for q holds instead.
        let n_q = pipe_exponent(&t.boundary(q), 0.15).unwrap();
        assert!((n - n_q).abs() > 0.1);
        let sum: f64 = t
            .node(q)
            .children()
            .map(|c| f64::from(t.node(c).radius).powf(n_q))
            .sum();
        assert!((sum / 0.15f64.powf(n_q) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn pin_thinner_than_its_child_is_clamped() {
        let mut t = bushy(4);
        t.update_radii();
        let unpinned = t.clone();
        let x = t.node(t.root()).main.child().unwrap();
        assert!(t.pin_radius(x, 0.01));
        t.update_radii();
        let thickest = t
            .node(x)
            .children()
            .map(|c| t.node(c).radius)
            .fold(0.0, f32::max);
        assert!(thickest > 0.01);
        assert_eq!(t.node(x).radius, thickest);
        // The subtree keeps the species' n, so only the pinned node and the
        // root it feeds change.
        for (id, node) in t.iter().filter(|&(id, _)| id != x && id != t.root()) {
            assert_eq!(node.radius, unpinned.node(id).radius);
        }
        // A pin on a single tip has nothing to solve and just holds.
        let tip = t.iter().find(|(_, n)| n.is_terminal()).unwrap().0;
        assert!(t.pin_radius(tip, 0.04));
        t.update_radii();
        assert_eq!(t.node(tip).radius, 0.04);
    }

    #[test]
    fn pins_are_dropped_with_their_nodes_and_refused_when_invalid() {
        let (mut t, [a, b, l]) = forked();
        let m = t.node(l).main.child().unwrap();
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(!t.pin_radius(a, bad));
        }
        assert!(t.pin_radius(a, 0.2) && t.pin_radius(b, 0.1));
        assert!(t.pin_radius(l, 0.1) && t.pin_radius(m, 0.05));
        t.shed(l);
        assert_eq!(t.pins().map(|(id, _)| id).collect::<Vec<_>>(), vec![a, b]);
        assert!(!t.pin_radius(l, 0.1), "a stump cannot be pinned");
        assert!(!t.pin_radius(m, 0.1), "a removed node cannot be pinned");
        assert!(!t.pin_radius(NodeId(999), 0.1));
        t.prune(b);
        assert_eq!(t.pins().collect::<Vec<_>>(), vec![(a, 0.2)]);
        assert_eq!(t.unpin(a), Some(0.2));
        assert_eq!(t.unpin(a), None);
        // Pruning an ancestor drops the pins below it.
        let (mut t, [a, b, l]) = forked();
        t.pin_radius(b, 0.1);
        t.pin_radius(l, 0.1);
        t.prune(a);
        assert_eq!(t.pins().count(), 0);
    }

    #[test]
    fn stumps_are_boundary_elements_of_a_pin() {
        let (mut t, [a, b, l]) = forked();
        t.shed(l);
        let root = t.root();
        let mut boundary = t.boundary(root);
        boundary.sort_by(f32::total_cmp);
        assert_eq!(boundary, vec![0.02, 0.05]);
        assert!(t.pin_radius(root, 0.07));
        t.update_radii();
        // a carries everything below the pin, so it matches the pin.
        assert!(approx(t.node(a).radius, 0.07));
        assert_eq!(t.node(l).radius, 0.05);
        assert_eq!(t.node(b).radius, 0.02);
        let n = pipe_exponent(&boundary, 0.07).unwrap();
        let sum = 0.02f64.powf(n) + 0.05f64.powf(n);
        assert!((sum / 0.07f64.powf(n) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn solver_converges_far_outside_the_species_range() {
        // Near-equal widest radii keep a large n well conditioned in f32.
        for (radii, n) in [
            ([0.02f32, 0.05, 0.03, 0.02], 0.3f64),
            ([0.02, 0.05, 0.03, 0.02], 1.5),
            ([0.02, 0.05, 0.03, 0.02], 4.0),
            ([0.02, 0.05, 0.03, 0.02], 12.0),
            ([1.0, 0.7, 1.0, 0.4], 40.0),
        ] {
            let target = radii
                .iter()
                .map(|&r| f64::from(r).powf(n))
                .sum::<f64>()
                .powf(n.recip()) as f32;
            let solved = pipe_exponent(&radii, target).unwrap();
            let residual: f64 = radii
                .iter()
                .map(|&r| (f64::from(r) / f64::from(target)).powf(solved))
                .sum::<f64>()
                - 1.0;
            assert!(residual.abs() < 1e-9, "n = {n}: residual {residual}");
            assert!((solved / n - 1.0).abs() < 1e-3, "n = {n}: solved {solved}");
        }
        let equal = [1.0f32; 4];
        for n in [1.5f64, 4.0] {
            let target = 4f64.powf(n.recip()) as f32;
            assert!((pipe_exponent(&equal, target).unwrap() - n).abs() < 1e-6);
        }
        assert_eq!(pipe_exponent(&[0.02], 0.1), None, "one element");
        assert_eq!(pipe_exponent(&[], 0.1), None);
        assert_eq!(
            pipe_exponent(&[0.02, 0.05], 0.05),
            None,
            "no wider than the widest"
        );
        assert_eq!(pipe_exponent(&[1.0, 1.0], 1.0001), Some(MAX_PIPE_EXPONENT));
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

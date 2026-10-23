//! Occupancy-aware space colonization, without light or vigor simulation.
//!
//! Call [`sense`] to update bud scratch fields, or [`grow_step`] to also grow
//! one internode per marker-supported bud, steered by species tropism. Competition reads one
//! immutable tree state before any scratch fields or geometry change. Habitat
//! implementations must likewise provide a consistent view during each call.

use std::collections::BTreeMap;

use glam::{DVec3, Vec3};

use crate::{
    BudFate, Habitat, NodeId, RestrictedJitter, RestrictedJitterError, SlotIndex, Tree,
    blend_directions, gravitropic_target,
};

/// World-origin marker grid configuration. Randomness uses [`Tree::seed`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SamplingConfig {
    /// Width of a cubic sampling cell, finite and positive.
    pub cell_size: f32,
    /// Displacement from cell center as a fraction of cell size, in `[0, 0.5)`.
    pub jitter: f32,
}

/// Sensing result for a non-aborted bud, ordered by node ID then main/lateral slot.
#[derive(Clone, Debug, PartialEq)]
pub struct BudGrowth {
    /// Node carrying the bud.
    pub node: NodeId,
    /// Child slot carrying the bud.
    pub slot: SlotIndex,
    /// Number of unoccupied markers won by this bud.
    pub marker_count: u32,
    /// Sum of unit vectors toward markers (each marker has equal weight).
    pub direction_sum: Vec3,
    /// Normalized direction sum, or `None` for no markers or cancellation.
    pub preferred_direction: Option<Vec3>,
}

struct Cone {
    node: NodeId,
    slot: SlotIndex,
    origin: DVec3,
    axis: Option<DVec3>,
    distance: f64,
    cosine: f64,
}

impl Cone {
    fn contains(&self, point: Vec3) -> Option<f64> {
        let offset = point.as_dvec3() - self.origin;
        let distance = offset.length();
        // The apex has no direction and never attracts a bud.
        (distance > 0.0
            && distance <= self.distance
            && offset.dot(self.axis?) >= distance * self.cosine)
            .then_some(distance)
    }
}

/// Assign markers and refresh bud counts, sums and fates without growing.
///
/// Perception is a spherical sector: distance and half-angle boundaries are
/// inclusive; the apex is excluded. A conservative enclosing AABB is queried
/// for each bud, then filtered to the exact sector. Markers on or inside any
/// live node's occupancy sphere (including stumps) or occupied by `habitat`
/// are removed. Each node uses its own species' occupancy radius.
///
/// Closest eligible bud wins; exact distance ties prefer smaller node IDs,
/// then Main, Lateral(0), Lateral(1). Aborted buds never compete. Invalid bud
/// directions remain dormant. Empty/cancelling results remain dormant; other
/// buds become active without consulting light or vigor. No geometry, light,
/// vigor or step counter changes. Sampling errors leave the tree unchanged.
/// Work scales with queried cells and candidate-marker/node pairs.
pub fn sense(
    tree: &mut Tree,
    habitat: &(impl Habitat + ?Sized),
    config: SamplingConfig,
) -> Result<Vec<BudGrowth>, RestrictedJitterError> {
    let grid = RestrictedJitter::new(config.cell_size, config.jitter, tree.seed())?;
    let mut cones = Vec::new();
    let mut spheres = Vec::new();
    for (id, node) in tree.iter() {
        spheres.push((
            node.position.as_dvec3(),
            f64::from(node.species.space.occupancy_radius),
        ));
        for (slot, bud) in node.buds() {
            if bud.fate != BudFate::Aborted {
                cones.push(Cone {
                    node: id,
                    slot,
                    origin: node.position.as_dvec3(),
                    axis: bud.direction.as_dvec3().try_normalize(),
                    distance: f64::from(node.species.space.perception_distance),
                    cosine: f64::from(node.species.space.perception_angle).cos(),
                });
            }
        }
    }
    // Position bits identify the same sampler marker across overlapping queries.
    // Ordered storage also fixes floating-point summation order across runs.
    let mut claims: BTreeMap<[u32; 3], (Vec3, usize, f64)> = BTreeMap::new();
    for (index, cone) in cones.iter().enumerate() {
        if cone.axis.is_none() {
            continue;
        }
        let origin = cone.origin.as_vec3();
        let radius = Vec3::splat(cone.distance as f32);
        // Sampler boxes are half-open; expand by one representable value so
        // markers exactly on the positive distance boundary are included.
        let min = (origin - radius).to_array().map(f32::next_down);
        let max = (origin + radius).to_array().map(f32::next_up);
        for point in grid.points_in_aabb(Vec3::from_array(min), Vec3::from_array(max))? {
            let Some(distance) = cone.contains(point) else {
                continue;
            };
            if habitat.occupied(point)
                || spheres
                    .iter()
                    .any(|(center, radius)| point.as_dvec3().distance(*center) <= *radius)
            {
                continue;
            }
            let key = point
                .to_array()
                .map(|v| if v == 0.0 { 0 } else { v.to_bits() });
            let claim = claims.entry(key).or_insert((point, index, distance));
            if distance < claim.2 {
                *claim = (point, index, distance);
            }
        }
    }
    let mut sums = vec![DVec3::ZERO; cones.len()];
    let mut counts = vec![0u32; cones.len()];
    for (point, index, _) in claims.values() {
        sums[*index] += (point.as_dvec3() - cones[*index].origin).normalize();
        counts[*index] += 1;
    }
    let mut result = Vec::with_capacity(cones.len());
    for (index, cone) in cones.iter().enumerate() {
        let direction_sum = sums[index].as_vec3();
        let preferred_direction = direction_sum.try_normalize();
        let bud = tree
            .node_mut(cone.node)
            .slot_mut(cone.slot)
            .bud_mut()
            .unwrap();
        bud.marker_count = counts[index];
        bud.marker_direction_sum = direction_sum;
        bud.fate = if preferred_direction.is_some() {
            BudFate::Active
        } else {
            BudFate::Dormant
        };
        result.push(BudGrowth {
            node: cone.node,
            slot: cone.slot,
            marker_count: counts[index],
            direction_sum,
            preferred_direction,
        });
    }
    Ok(result)
}

/// Sense once, then grow one species-length internode per active bud.
///
/// New buds and nodes participate only on the next call. Marker support is
/// required even when the environmental weight is zero. The normalized marker
/// direction is blended with the bud heading and local gravitropic target;
/// sensing scratch fields retain their environmental meaning. Only a main-slot
/// continuation on order zero uses the main set-point; all other growth uses
/// the lateral set-point. Markers are never endpoints. [`Tree::grow`] checks habitat at each
/// proposed endpoint; a blocked bud remains dormant. Advances the step once
/// even if nothing grows. A cancelling blend is passed through as zero and
/// leaves the bud dormant. Direct calls to [`Tree::grow`] retain caller-supplied
/// directions. Returns IDs of successfully added nodes.
///
/// ```
/// use std::sync::Arc;
/// use glam::Vec3;
/// use trees_core::{Tree, TreeSpecies, OpenAir, SamplingConfig, grow_step};
/// let mut tree = Tree::new(Arc::new(TreeSpecies::default()), 42, Vec3::ZERO)?;
/// let added = grow_step(&mut tree, &OpenAir, SamplingConfig { cell_size: 0.5, jitter: 0.25 })?;
/// assert_eq!(tree.step(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn grow_step(
    tree: &mut Tree,
    habitat: &(impl Habitat + ?Sized),
    config: SamplingConfig,
) -> Result<Vec<NodeId>, RestrictedJitterError> {
    let sensed = sense(tree, habitat, config)?;
    let mut added = Vec::new();
    for bud in sensed {
        if let Some(environment) = bud.preferred_direction {
            let parent = tree.node(bud.node);
            let heading = parent.slot(bud.slot).bud().unwrap().direction;
            let weights = &parent.species.tropism;
            let inclination = if parent.order == 0 && bud.slot == SlotIndex::Main {
                weights.main_set_point
            } else {
                weights.lateral_set_point
            };
            let target = gravitropic_target(heading, inclination, tree.seed(), bud.node, bud.slot);
            let direction = blend_directions(heading, target, environment, weights);
            let length = parent.species.architecture.internode_length;
            if let Some(id) = tree.grow(habitat, bud.node, bud.slot, direction, length) {
                added.push(id);
            } else {
                tree.node_mut(bud.node)
                    .slot_mut(bud.slot)
                    .bud_mut()
                    .unwrap()
                    .fate = BudFate::Dormant;
            }
        }
    }
    tree.advance_step();
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bud, OpenAir, Slot, TreeSpecies};
    use std::sync::Arc;

    const CONFIG: SamplingConfig = SamplingConfig {
        cell_size: 1.0,
        jitter: 0.0,
    };

    fn tree(seed: u64) -> Tree {
        let mut species = TreeSpecies::default();
        species.space.occupancy_radius = 0.1;
        species.space.perception_distance = 3.0;
        species.space.perception_angle = 0.5;
        let mut tree = Tree::new(Arc::new(species), seed, Vec3::ZERO).unwrap();
        let root = tree.root();
        let node = tree.node_mut(root);
        node.position = Vec3::splat(0.5);
        node.main = Slot::Bud(Bud::new(Vec3::X));
        node.laterals = [Slot::Empty, Slot::Empty];
        tree
    }

    #[test]
    fn cone_boundaries_and_apex() {
        let cone = Cone {
            node: NodeId(0),
            slot: SlotIndex::Main,
            origin: DVec3::ZERO,
            axis: Some(DVec3::X),
            distance: 5.0,
            cosine: 0.6,
        };
        assert!(cone.contains(Vec3::new(3.0, 4.0, 0.0)).is_some());
        assert!(cone.contains(Vec3::new(5.0, 0.0, 0.0)).is_some());
        assert!(cone.contains(Vec3::new(3.0, 4.001, 0.0)).is_none());
        assert!(cone.contains(Vec3::new(5.001, 0.0, 0.0)).is_none());
        assert!(cone.contains(-Vec3::X).is_none());
        assert!(cone.contains(Vec3::ZERO).is_none());
        let wide = Cone {
            cosine: -0.6,
            ..cone
        };
        assert!(wide.contains(Vec3::new(-3.0, 4.0, 0.0)).is_some());
    }

    #[test]
    fn exact_distance_boundary_is_sampled_and_units_have_equal_weight() {
        let mut tree = tree(1);
        let result = sense(&mut tree, &OpenAir, CONFIG).unwrap();
        // Includes (3.5, .5, .5), exactly distance 3 from the apex.
        assert_eq!(result[0].marker_count, 7);
        assert!((result[0].preferred_direction.unwrap() - Vec3::X).length() < 1e-6);
        let expected_x = 3.0 + 4.0 * 2.0 / 5f32.sqrt();
        assert!((result[0].direction_sum.x - expected_x).abs() < 1e-6);
    }

    #[test]
    fn habitat_and_all_live_node_spheres_filter_markers() {
        let mut tree = tree(1);
        let root = tree.root();
        let child = tree
            .grow(&OpenAir, root, SlotIndex::Main, Vec3::X, 2.0)
            .unwrap();
        tree.node_mut(root).main = Slot::Bud(Bud::new(Vec3::X));
        let node = tree.node_mut(child);
        node.main = Slot::Empty;
        node.laterals = [Slot::Empty, Slot::Empty];
        node.stump = true;
        let result = sense(
            &mut tree,
            &|p: Vec3| p.x >= 3.0 || p.y != 0.5 || p.z != 0.5,
            CONFIG,
        )
        .unwrap();
        // x=2.5 is occupied by the stump; x=3.5 by the habitat.
        assert_eq!(result[0].marker_count, 1);
        assert_eq!(result[0].preferred_direction, Some(Vec3::X));
    }

    #[test]
    fn closest_claim_and_stable_slot_ties_exclude_aborted_buds() {
        let mut tree = tree(1);
        let root = tree.root();
        tree.node_mut(root).laterals[0] = Slot::Bud(Bud::new(Vec3::X));
        let tied = sense(&mut tree, &OpenAir, CONFIG).unwrap();
        assert_eq!(tied[0].marker_count, 7);
        assert_eq!(tied[1].marker_count, 0);
        let child = tree
            .grow(&OpenAir, root, SlotIndex::Main, Vec3::X, 1.0)
            .unwrap();
        tree.node_mut(child).laterals = [Slot::Empty, Slot::Empty];
        tree.node_mut(child).main = Slot::Bud(Bud::new(Vec3::X));
        let results = sense(&mut tree, &|p: Vec3| p != Vec3::new(2.5, 0.5, 0.5), CONFIG).unwrap();
        assert_eq!(results[0].marker_count, 0);
        assert_eq!(results[1].marker_count, 1);
        tree.node_mut(child).main.bud_mut().unwrap().fate = BudFate::Aborted;
        let results = sense(&mut tree, &OpenAir, CONFIG).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].marker_count > 0);
    }

    #[test]
    fn empty_results_clear_stale_fields_and_errors_are_atomic() {
        let mut tree = tree(1);
        sense(&mut tree, &OpenAir, CONFIG).unwrap();
        let before = tree.node(tree.root()).main.clone();
        assert!(
            sense(
                &mut tree,
                &OpenAir,
                SamplingConfig {
                    cell_size: 0.0,
                    ..CONFIG
                }
            )
            .is_err()
        );
        assert_eq!(tree.node(tree.root()).main, before);
        let result = sense(&mut tree, &|_: Vec3| true, CONFIG).unwrap();
        assert_eq!(result[0].marker_count, 0);
        assert_eq!(result[0].preferred_direction, None);
        let bud = tree.node(tree.root()).main.bud().unwrap();
        assert_eq!(bud.fate, BudFate::Dormant);
        assert_eq!(bud.marker_direction_sum, Vec3::ZERO);
    }

    #[test]
    fn repeatable_seed_and_growth_uses_snapshot_and_internode_length() {
        let config = SamplingConfig {
            jitter: 0.25,
            ..CONFIG
        };
        let mut a = tree(42);
        let mut b = a.clone();
        assert_eq!(
            sense(&mut a, &OpenAir, config),
            sense(&mut b, &OpenAir, config)
        );
        assert_ne!(
            sense(&mut a, &OpenAir, config),
            sense(&mut tree(43), &OpenAir, config)
        );
        let added = grow_step(&mut a, &OpenAir, config).unwrap();
        assert_eq!(added.len(), 1); // New buds do not grow in the same step.
        let node = a.node(added[0]);
        assert!((node.position.distance(a.node(a.root()).position) - 1.0).abs() < 1e-6);
        assert_eq!(a.step(), 1);
        assert!(
            grow_step(&mut b, &|_: Vec3| true, config)
                .unwrap()
                .is_empty()
        );
    }

    fn set_weights(tree: &mut Tree, straightness: f32, gravitropism: f32, environment: f32) {
        let root = tree.root();
        let species = Arc::make_mut(&mut tree.node_mut(root).species);
        species.tropism.straightness = straightness;
        species.tropism.gravitropism = gravitropism;
        species.tropism.environment = environment;
    }

    #[test]
    fn environment_only_matches_explicit_growth_and_replays() {
        let mut actual = tree(42);
        set_weights(&mut actual, 0.0, 0.0, 1.0);
        let mut replay = actual.clone();
        let mut expected = actual.clone();
        let sensed = sense(&mut expected, &OpenAir, CONFIG).unwrap();
        let bud = &sensed[0];
        let length = expected
            .node(bud.node)
            .species
            .architecture
            .internode_length;
        let child = expected
            .grow(
                &OpenAir,
                bud.node,
                bud.slot,
                bud.preferred_direction.unwrap(),
                length,
            )
            .unwrap();
        let added = grow_step(&mut actual, &OpenAir, CONFIG).unwrap();
        assert_eq!(added, grow_step(&mut replay, &OpenAir, CONFIG).unwrap());
        assert_eq!(
            actual.node(added[0]).position,
            expected.node(child).position
        );
        for (id, node) in actual.iter() {
            assert_eq!(node.position, replay.node(id).position);
            assert_eq!(node.heading, replay.node(id).heading);
        }
    }

    #[test]
    fn steering_preserves_environmental_fields_and_marker_gating() {
        for (straightness, gravitropism, expected) in [(1.0, 0.0, Vec3::X), (0.0, 1.0, Vec3::Y)] {
            let mut tree = tree(1);
            set_weights(&mut tree, straightness, gravitropism, 0.0);
            let root = tree.root();
            Arc::make_mut(&mut tree.node_mut(root).species)
                .tropism
                .main_set_point = 0.0;
            assert_eq!(
                sense(&mut tree, &OpenAir, CONFIG).unwrap()[0].preferred_direction,
                Some(Vec3::X)
            );
            // Endpoint rejection retains the bud and its environmental scratch data.
            let endpoint = tree.node(root).position + expected;
            let sensed = sense(&mut tree, &|p: Vec3| p == endpoint, CONFIG).unwrap();
            assert!(
                grow_step(&mut tree, &|p: Vec3| p == endpoint, CONFIG)
                    .unwrap()
                    .is_empty()
            );
            let bud = tree.node(root).main.bud().unwrap();
            assert_eq!(bud.fate, BudFate::Dormant);
            assert_eq!(bud.marker_direction_sum, sensed[0].direction_sum);
            let added = grow_step(&mut tree, &OpenAir, CONFIG).unwrap();
            assert_eq!(added.len(), 1);
            assert!((tree.node(added[0]).heading - expected).length() < 1e-6);
            assert!(
                (tree
                    .node(added[0])
                    .position
                    .distance(tree.node(root).position)
                    - 1.0)
                    .abs()
                    < 1e-6
            );
            assert_eq!(tree.step(), 2);
        }
        let mut tree = tree(1);
        set_weights(&mut tree, 0.0, 1.0, 0.0);
        assert!(
            grow_step(&mut tree, &|_: Vec3| true, CONFIG)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            tree.node(tree.root()).main.bud().unwrap().fate,
            BudFate::Dormant
        );
        assert_eq!(tree.step(), 1);
    }

    #[test]
    fn prospective_axis_order_selects_set_point() {
        for (order, slot, expected) in [
            (0, SlotIndex::Main, Vec3::Y),
            (0, SlotIndex::Lateral(0), -Vec3::Y),
            (0, SlotIndex::Lateral(1), -Vec3::Y),
            (1, SlotIndex::Main, -Vec3::Y),
            (1, SlotIndex::Lateral(0), -Vec3::Y),
        ] {
            let mut tree = tree(1);
            set_weights(&mut tree, 0.0, 1.0, 0.0);
            let root = tree.root();
            let node = tree.node_mut(root);
            node.order = order;
            node.main = Slot::Empty;
            *node.slot_mut(slot) = Slot::Bud(Bud::new(Vec3::X));
            let species = Arc::make_mut(&mut node.species);
            species.tropism.main_set_point = 0.0;
            species.tropism.lateral_set_point = std::f32::consts::PI;
            species.architecture.branching = crate::BranchingPattern::Opposite;
            let added = grow_step(&mut tree, &OpenAir, CONFIG).unwrap();
            assert_eq!(added.len(), 1);
            assert_eq!(tree.node(added[0]).heading, expected);
            assert_eq!(
                tree.node(added[0]).order,
                order + u16::from(slot != SlotIndex::Main)
            );
        }
    }

    #[test]
    fn cancellation_is_dormant_and_direct_growth_keeps_caller_direction() {
        let mut tree = tree(1);
        set_weights(&mut tree, 0.5, 0.5, 0.0);
        let root = tree.root();
        let node = tree.node_mut(root);
        node.main = Slot::Bud(Bud::new(-Vec3::Y));
        Arc::make_mut(&mut node.species).tropism.main_set_point = 0.0;
        assert!(grow_step(&mut tree, &OpenAir, CONFIG).unwrap().is_empty());
        let bud = tree.node(root).main.bud().unwrap();
        assert!(bud.marker_count > 0);
        assert_eq!(bud.fate, BudFate::Dormant);
        assert_eq!(tree.step(), 1);
        let child = tree
            .grow(&OpenAir, root, SlotIndex::Main, Vec3::Z, 2.0)
            .unwrap();
        assert_eq!(tree.node(child).heading, Vec3::Z);
    }
}

//! Raw sampler markers in bounded perception AABBs around live buds.
use std::collections::BTreeSet;

use bevy::prelude::*;
use trees_core::{BudFate, RestrictedJitter, Tree};

use crate::{branches::GrownTree, editor::TreeParams};

const MAX_MARKERS: usize = 4_000;
const MAX_REGIONS: usize = 16;

/// Display-only state: never participates in regrowth or width pins.
#[derive(Resource, Default)]
pub struct MarkerDebug {
    pub enabled: bool,
    points: Vec<Vec3>,
}

fn points(tree: &Tree, params: &TreeParams) -> Vec<Vec3> {
    let Ok(grid) = RestrictedJitter::new(params.point_spacing, params.jitter, tree.seed()) else {
        return Vec::new();
    };
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for (_, node) in tree
        .iter()
        .filter(|(_, n)| n.buds().any(|(_, b)| b.fate != BudFate::Aborted))
        .take(MAX_REGIONS)
    {
        let radius = glam::Vec3::splat(node.species.space.perception_distance);
        let Ok(query) = grid.points_in_aabb(node.position - radius, node.position + radius) else {
            continue;
        };
        for point in query {
            if seen.insert(point.to_array().map(f32::to_bits)) {
                result.push(Vec3::from_array(point.to_array()));
                if result.len() == MAX_MARKERS {
                    return result;
                }
            }
        }
    }
    result
}

pub fn show(
    tree: Option<Res<GrownTree>>,
    params: Res<TreeParams>,
    mut debug: ResMut<MarkerDebug>,
    mut gizmos: Gizmos,
) {
    if !debug.enabled {
        return;
    }
    let Some(tree) = tree else {
        return;
    };
    if debug.is_changed() || tree.is_changed() {
        debug.points = points(&tree.0, &params);
    }
    let color = Color::srgb(1.0, 0.65, 0.15);
    for &point in &debug.points {
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            gizmos.line(point - axis * 0.025, point + axis * 0.025, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_repeatable_queries_do_not_modify_geometry() {
        let params = TreeParams::default();
        let tree = crate::grow_tree(&trees_core::OpenAir, &params).unwrap();
        let before: Vec<_> = tree.iter().map(|(_, n)| n.position).collect();
        let a = points(&tree, &params);
        assert!(!a.is_empty());
        assert!(a.len() <= MAX_MARKERS);
        assert_eq!(a, points(&tree, &params));
        assert_eq!(
            before,
            tree.iter().map(|(_, n)| n.position).collect::<Vec<_>>()
        );
    }
}

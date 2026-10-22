//! Temporary visualization of stored leaf placements, without leaf textures.
//!
//! One world unit is interpreted as one metre for this debug marker only.
//! Each disk has a four-inch diameter and is centered on the initial blade.
//! All disks share one batched mesh and a two-sided, unlit material.

use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use trees_core::Tree;

use crate::branches::ShownTree;

const DISK_RADIUS: f32 = 4.0 * 0.0254 / 2.0;
const DISK_SIDES: usize = 16;

/// A viewer-only switch; toggling it does not change species or regrow trees.
#[derive(Resource)]
pub struct LeafDebug {
    pub enabled: bool,
}

impl Default for LeafDebug {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Component)]
pub struct LeafDebugView;

pub fn setup(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.spawn((
        LeafDebugView,
        Mesh3d::default(),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.25, 0.95, 0.35),
            unlit: true,
            cull_mode: None,
            ..default()
        })),
        Visibility::Visible,
        NotShadowCaster,
    ));
}

/// Runs after the displayed (width-pinned) tree has been updated.
pub fn show_leaf_debug(
    tree: Option<Res<ShownTree>>,
    settings: Res<LeafDebug>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut view: Single<(&mut Mesh3d, &mut Visibility), With<LeafDebugView>>,
) {
    let (mesh, visibility) = &mut *view;
    if settings.is_changed() {
        **visibility = if settings.enabled {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if let Some(tree) = tree.filter(|tree| tree.is_changed()) {
        mesh.0 = meshes.add(disk_mesh(&tree.0));
    }
}

fn disk_mesh(tree: &Tree) -> Mesh {
    let mut positions = Vec::with_capacity(tree.leaves().len() * (DISK_SIDES + 1));
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut indices = Vec::with_capacity(tree.leaves().len() * DISK_SIDES * 3);
    let f = &tree.species().foliage;
    for leaf in tree.leaves() {
        let center = leaf.blade_base(f.petiole_length) + leaf.heading * (f.blade_length * 0.5);
        let base = u32::try_from(positions.len()).expect("too many debug vertices");
        positions.push(center.to_array());
        normals.push(leaf.normal.to_array());
        for i in 0..DISK_SIDES {
            let angle = std::f32::consts::TAU * i as f32 / DISK_SIDES as f32;
            let (sin, cos) = angle.sin_cos();
            let position = center + DISK_RADIUS * (cos * leaf.heading + sin * leaf.left);
            positions.push(position.to_array());
            normals.push(leaf.normal.to_array());
            indices.extend([
                base,
                base + 1 + i as u32,
                base + 1 + ((i + 1) % DISK_SIDES) as u32,
            ]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_indices(Indices::U32(indices))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;
    use std::sync::Arc;
    use trees_core::TreeSpecies;

    #[test]
    fn disks_have_four_inch_diameter_and_match_blade_frames() {
        let tree = Tree::new(Arc::new(TreeSpecies::default()), 42, Vec3::ZERO).unwrap();
        let mesh = disk_mesh(&tree);
        let Some(VertexAttributeValues::Float32x3(vertices)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("missing positions")
        };
        let f = &tree.species().foliage;
        assert_eq!(vertices.len(), tree.leaves().len() * (DISK_SIDES + 1));
        for (leaf, disk) in tree
            .leaves()
            .iter()
            .zip(vertices.as_chunks::<{ DISK_SIDES + 1 }>().0)
        {
            let center = Vec3::from_array(disk[0]);
            let expected =
                leaf.blade_base(f.petiole_length) + leaf.heading * (f.blade_length * 0.5);
            assert!(center.distance(expected) < 1e-6);
            for vertex in &disk[1..] {
                let offset = Vec3::from_array(*vertex) - center;
                assert!((offset.length() * 2.0 - 0.1016).abs() < 1e-6);
                assert!(offset.dot(leaf.normal).abs() < 1e-6);
            }
        }
        let Some(Indices::U32(indices)) = mesh.indices() else {
            panic!("missing indices")
        };
        for triangle in indices.as_chunks::<3>().0 {
            let [a, b, c] = triangle.map(|i| Vec3::from_array(vertices[i as usize]));
            let normal = tree.leaves()[triangle[0] as usize / (DISK_SIDES + 1)].normal;
            assert!((b - a).cross(c - a).dot(normal) > 0.0);
        }
    }

    #[test]
    fn debug_toggle_does_not_replace_geometry_and_tree_updates_do() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<LeafDebug>()
            .add_systems(Startup, setup)
            .add_systems(Update, show_leaf_debug);
        let tree = Tree::new(Arc::new(TreeSpecies::default()), 0, Vec3::ZERO).unwrap();
        app.insert_resource(ShownTree(tree));
        app.update();
        let handle = app
            .world_mut()
            .query_filtered::<&Mesh3d, With<LeafDebugView>>()
            .single(app.world())
            .unwrap()
            .0
            .clone();
        app.world_mut().resource_mut::<LeafDebug>().enabled = false;
        app.update();
        let mut query = app
            .world_mut()
            .query_filtered::<(&Mesh3d, &Visibility), With<LeafDebugView>>();
        let (mesh, visibility) = query.single(app.world()).unwrap();
        assert_eq!(mesh.0, handle);
        assert_eq!(*visibility, Visibility::Hidden);
        let mut species = TreeSpecies::default();
        species.foliage.enabled = false;
        app.insert_resource(ShownTree(
            Tree::new(Arc::new(species), 0, Vec3::ZERO).unwrap(),
        ));
        app.update();
        let mesh = app
            .world_mut()
            .query_filtered::<&Mesh3d, With<LeafDebugView>>()
            .single(app.world())
            .unwrap();
        assert_ne!(mesh.0, handle);
        assert_eq!(
            app.world()
                .resource::<Assets<Mesh>>()
                .get(&mesh.0)
                .unwrap()
                .count_vertices(),
            0
        );
        app.world_mut().resource_mut::<LeafDebug>().enabled = true;
        app.update();
        let visibility = app
            .world_mut()
            .query_filtered::<&Visibility, With<LeafDebugView>>()
            .single(app.world())
            .unwrap();
        assert_eq!(*visibility, Visibility::Visible);
    }
}

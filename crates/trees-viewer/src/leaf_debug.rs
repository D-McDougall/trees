//! Temporary visualization of stored leaf placements, without leaf textures.
//!
//! One world unit is interpreted as one metre for this debug marker only.
//! Disks default to four inches across and scale with blade width/length,
//! keeping the bottom-center blade base fixed at the petiole endpoint.
//! All disks share one batched mesh and a two-sided, unlit material.

use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use trees_core::Tree;

use crate::branches::ShownTree;

const DISK_SIDES: usize = 16;
const TWIG_SIDES: usize = 6;
const TWIG_RADIUS: f32 = 0.0015;

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

#[derive(Component)]
pub struct PetioleView;

pub fn setup(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.spawn((
        PetioleView,
        Mesh3d::default(),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.45, 0.32, 0.22),
            ..default()
        })),
        Visibility::Visible,
    ));
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
    mut twig_view: Single<
        (&mut Mesh3d, &mut Visibility),
        (With<PetioleView>, Without<LeafDebugView>),
    >,
) {
    let (mesh, visibility) = &mut *view;
    if settings.is_changed() {
        **visibility = if settings.enabled {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    *twig_view.1 = **visibility;
    if let Some(tree) = tree.filter(|tree| tree.is_changed()) {
        mesh.0 = meshes.add(disk_mesh(&tree.0));
        twig_view.0.0 = meshes.add(petiole_mesh(&tree.0));
    }
}

fn disk_mesh(tree: &Tree) -> Mesh {
    let mut positions = Vec::with_capacity(tree.leaves().len() * (DISK_SIDES + 1));
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut uvs = Vec::with_capacity(positions.capacity());
    let mut indices = Vec::with_capacity(tree.leaves().len() * DISK_SIDES * 3);
    let f = &tree.species().foliage;
    // Shared unit-circle template: no need to repeat trigonometry per leaf.
    let perimeter: [(f32, f32); DISK_SIDES] =
        std::array::from_fn(|i| (std::f32::consts::TAU * i as f32 / DISK_SIDES as f32).sin_cos());
    for leaf in tree.leaves() {
        let center = leaf.blade_base(f.petiole_length) + leaf.heading * (f.blade_length * 0.5);
        let base = u32::try_from(positions.len()).expect("too many debug vertices");
        positions.push(center.to_array());
        normals.push(leaf.normal.to_array());
        uvs.push([0.5, 0.5]);
        for (i, &(sin, cos)) in perimeter.iter().enumerate() {
            // Scale around the bottom-center pivot, not around the disk center.
            let position = center
                + cos * (f.blade_length * 0.5) * leaf.heading
                + sin * (f.blade_width * 0.5) * leaf.left;
            positions.push(position.to_array());
            normals.push(leaf.normal.to_array());
            // Image top is v=0; bottom center (u=0.5, v=1) is the blade base.
            // Positive L is image-left, so u decreases along it.
            uvs.push([0.5 - 0.5 * sin, 0.5 - 0.5 * cos]);
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
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// Batched solid stems following the same straight petiole as the leaf disks.
/// Start inside the supporting tube: its polygonal bark can lie slightly
/// inside the ideal circular surface used for the stored attachment.
fn petiole_mesh(tree: &Tree) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    for leaf in tree.leaves() {
        let node = tree.node(leaf.node);
        let start = node.base() + node.heading * leaf.distance;
        let end = leaf.blade_base(tree.species().foliage.petiole_length);
        let Some(axis) = (end - start).try_normalize() else {
            continue;
        };
        let left = axis.any_orthonormal_vector();
        let up = axis.cross(left);
        let base = u32::try_from(positions.len()).expect("too many petiole vertices");
        for center in [start, end] {
            for side in 0..TWIG_SIDES {
                let (sin, cos) =
                    (std::f32::consts::TAU * side as f32 / TWIG_SIDES as f32).sin_cos();
                let normal = cos * left + sin * up;
                positions.push((center + TWIG_RADIUS * normal).to_array());
                normals.push(normal.to_array());
            }
        }
        for side in 0..TWIG_SIDES as u32 {
            let next = (side + 1) % TWIG_SIDES as u32;
            let tip = base + TWIG_SIDES as u32;
            indices.extend([
                base + side,
                base + next,
                tip + side,
                base + next,
                tip + next,
                tip + side,
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
    fn stems_connect_branch_interiors_to_blade_bases() {
        for length in [0.0, 0.08, 0.5] {
            let mut species = TreeSpecies::default();
            species.foliage.petiole_length = length;
            let tree = Tree::new(Arc::new(species), 42, Vec3::ZERO).unwrap();
            let mesh = petiole_mesh(&tree);
            let Some(VertexAttributeValues::Float32x3(vertices)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("missing positions")
            };
            assert!(!tree.leaves().is_empty());
            assert_eq!(vertices.len(), tree.leaves().len() * 2 * TWIG_SIDES);
            for (leaf, stem) in tree
                .leaves()
                .iter()
                .zip(vertices.chunks_exact(2 * TWIG_SIDES))
            {
                let node = tree.node(leaf.node);
                let start = node.base() + node.heading * leaf.distance;
                let end = leaf.blade_base(length);
                for (ring, expected) in stem.chunks_exact(TWIG_SIDES).zip([start, end]) {
                    let center =
                        ring.iter().map(|v| Vec3::from_array(*v)).sum::<Vec3>() / TWIG_SIDES as f32;
                    assert!(center.distance(expected) < 1e-6);
                }
            }
            let Some(Indices::U32(indices)) = mesh.indices() else {
                panic!("missing indices")
            };
            let Some(VertexAttributeValues::Float32x3(normals)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!("missing normals")
            };
            for triangle in indices.chunks_exact(3) {
                let [a, b, c] =
                    std::array::from_fn(|i| Vec3::from_array(vertices[triangle[i] as usize]));
                assert!(
                    (b - a)
                        .cross(c - a)
                        .dot(Vec3::from_array(normals[triangle[0] as usize]))
                        > 0.0
                );
            }
        }
    }

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
    fn independent_scaling_keeps_bottom_center_rooted_to_petiole_endpoint() {
        let default = TreeSpecies::default();
        assert!((default.foliage.blade_length - 0.1016).abs() < 1e-7);
        assert!((default.foliage.blade_width - 0.1016).abs() < 1e-7);
        let original = Tree::new(Arc::new(default.clone()), 42, Vec3::ZERO).unwrap();
        for (length_scale, width_scale) in [(2.0, 0.5), (0.5, 3.0), (1.0, 1.0)] {
            let mut species = default.clone();
            species.foliage.blade_length *= length_scale;
            species.foliage.blade_width *= width_scale;
            let tree = Tree::new(Arc::new(species), 42, Vec3::ZERO).unwrap();
            assert_eq!(tree.leaves(), original.leaves());
            let mesh = disk_mesh(&tree);
            let Some(VertexAttributeValues::Float32x3(vertices)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("missing positions")
            };
            let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
            else {
                panic!("missing UVs")
            };
            let f = &tree.species().foliage;
            for (index, leaf) in tree.leaves().iter().enumerate() {
                let start = index * (DISK_SIDES + 1);
                let disk = &vertices[start..start + DISK_SIDES + 1];
                let base = leaf.blade_base(f.petiole_length);
                let bottom = Vec3::from_array(disk[1 + DISK_SIDES / 2]);
                assert!(bottom.distance(base) < 1e-6);
                let tip = Vec3::from_array(disk[1]);
                assert!(tip.distance(base + leaf.heading * f.blade_length) < 1e-6);
                let left = Vec3::from_array(disk[1 + DISK_SIDES / 4]);
                let right = Vec3::from_array(disk[1 + 3 * DISK_SIDES / 4]);
                assert!((left.distance(right) - f.blade_width).abs() < 1e-6);
                assert!(uvs[start + 1 + DISK_SIDES / 4][0].abs() < 1e-6);
                assert!((uvs[start + 1 + 3 * DISK_SIDES / 4][0] - 1.0).abs() < 1e-6);
                let bottom_uv = uvs[start + 1 + DISK_SIDES / 2];
                assert!((bottom_uv[0] - 0.5).abs() < 1e-6);
                assert!((bottom_uv[1] - 1.0).abs() < 1e-6);
            }
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

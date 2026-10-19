//! Minimal Bevy viewer: grows a small test tree with a fixed rule (there is
//! no growth simulator yet) and draws its wireframe mesh, slowly turning.
//!
//! Native: `cargo run -p trees-viewer`
//! Web:    `cd crates/trees-viewer && trunk serve`, then open http://127.0.0.1:8080

use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use trees_core::{Tree, TreeSpecies};
use trees_mesh::{MeshOptions, Topology, TreeMesh, build_mesh};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Trees Viewer".into(),
                // On wasm, size the canvas to its parent element.
                fit_canvas_to_parent: true,
                // On wasm, keep browser shortcuts such as F5 and Ctrl+R working.
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(Update, spin)
        .run();
}

/// Marks the test tree so it can be animated.
#[derive(Component)]
struct Spin;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground disc
    commands.spawn((
        Mesh3d(meshes.add(Circle::new(6.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.35, 0.55, 0.3))),
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));
    // Test tree
    let tree_mesh = build_mesh(&test_tree(), &MeshOptions::default());
    commands.spawn((
        Mesh3d(meshes.add(to_bevy_mesh(tree_mesh))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.85, 0.65, 0.45),
            unlit: true,
            ..default()
        })),
        Spin,
    ));
    commands.spawn((
        PointLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-4.0, 6.0, 16.0).looking_at(Vec3::new(0.0, 4.0, 0.0), Vec3::Y),
    ));
}

fn spin(time: Res<Time>, mut query: Query<&mut Transform, With<Spin>>) {
    for mut transform in &mut query {
        transform.rotate_y(0.3 * time.delta_secs());
    }
}

/// A stand-in until the growth simulator exists: every bud grows each step,
/// bent upward a little, with shorter internodes on higher-order branches.
fn test_tree() -> Tree {
    const STEPS: u32 = 7;
    const MAX_ORDER: u16 = 3;
    let species = Arc::new(TreeSpecies::default());
    let internode = species.architecture.internode_length;
    let mut tree = Tree::new(species, 0, glam::Vec3::ZERO).expect("default species is valid");
    for _ in 0..STEPS {
        let buds: Vec<_> = tree
            .iter()
            .filter(|(_, node)| node.order <= MAX_ORDER)
            .flat_map(|(id, node)| {
                node.buds()
                    .map(move |(slot, bud)| (id, slot, bud.direction, node.order))
            })
            .collect();
        for (id, slot, direction, order) in buds {
            let direction = direction + 0.3 * glam::Vec3::Y;
            tree.grow(id, slot, direction, internode * 0.8f32.powi(order.into()));
        }
        tree.advance_step();
    }
    tree
}

fn to_bevy_mesh(mesh: TreeMesh) -> Mesh {
    let topology = match mesh.topology {
        Topology::LineList => PrimitiveTopology::LineList,
        _ => unimplemented!("viewer does not draw {:?} yet", mesh.topology),
    };
    Mesh::new(topology, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, mesh.positions)
        .with_inserted_indices(Indices::U32(mesh.indices))
}

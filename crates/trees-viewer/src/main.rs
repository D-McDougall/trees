//! Minimal Bevy viewer: grows a small test tree with a fixed rule (there is
//! no growth simulator yet) and draws its tube mesh. Drag to orbit the
//! camera, right-drag to pan and scroll to zoom.
//!
//! Native: `cargo run -p trees-viewer`
//! Web:    `cd crates/trees-viewer && trunk serve`, then open http://127.0.0.1:8080

use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::camera_controller::pan_orbit_camera::prelude::PanOrbitCamera;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use trees_core::{Habitat, Tree, TreeSpecies};
use trees_mesh::{MeshOptions, MeshStyle, Topology, TreeMesh, TubeOptions, build_mesh};

mod camera;

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
        .add_plugins(camera::CameraControlsPlugin)
        .add_systems(Startup, setup)
        .run();
}

/// Where the test tree is planted.
const TREE_ROOT: Vec3 = Vec3::ZERO;

/// The viewer's world as the growth logic sees it: everything below the
/// ground plane is solid.
struct Scene {
    ground_height: f32,
}

impl Habitat for Scene {
    fn occupied(&self, point: glam::Vec3) -> bool {
        point.y < self.ground_height
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let scene = Scene { ground_height: 0.0 };
    // Ground disc
    commands.spawn((
        Mesh3d(meshes.add(Circle::new(6.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.35, 0.55, 0.3))),
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
            .with_translation(Vec3::Y * scene.ground_height),
    ));
    // Test tree
    let options = MeshOptions {
        style: MeshStyle::Tubes(TubeOptions::default()),
    };
    let tree_mesh = build_mesh(&test_tree(&scene), &options);
    commands.spawn((
        Mesh3d(meshes.add(to_bevy_mesh(tree_mesh))),
        MeshMaterial3d(materials.add(Color::srgb(0.45, 0.32, 0.22))),
    ));
    commands.spawn((
        PointLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));
    // Orbit about the tree's root.
    let eye = TREE_ROOT + Vec3::new(-5.0, 5.0, 22.0);
    commands.spawn((
        Camera3d::default(),
        PanOrbitCamera::default().with_initial_anchor_depth(eye.distance(TREE_ROOT).into()),
        Transform::from_translation(eye).looking_at(TREE_ROOT, Vec3::Y),
    ));
}

/// A stand-in until the growth simulator exists: every bud grows each step,
/// bent upward a little, with shorter internodes on higher-order branches.
/// Buds that would grow into `habitat`'s occupied space stay dormant.
fn test_tree(habitat: &impl Habitat) -> Tree {
    const STEPS: u32 = 7;
    const MAX_ORDER: u16 = 3;
    let species = Arc::new(TreeSpecies::default());
    let internode = species.architecture.internode_length;
    let mut tree = Tree::new(species, 0, TREE_ROOT).expect("default species is valid");
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
            tree.grow(
                habitat,
                id,
                slot,
                direction,
                internode * 0.8f32.powi(order.into()),
            );
        }
        tree.advance_step();
    }
    tree
}

fn to_bevy_mesh(mesh: TreeMesh) -> Mesh {
    let topology = match mesh.topology {
        Topology::LineList => PrimitiveTopology::LineList,
        Topology::TriangleList => PrimitiveTopology::TriangleList,
        _ => unimplemented!("viewer does not draw {:?} yet", mesh.topology),
    };
    let mut bevy_mesh = Mesh::new(topology, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, mesh.positions)
        .with_inserted_indices(Indices::U32(mesh.indices));
    if !mesh.normals.is_empty() {
        bevy_mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, mesh.normals);
    }
    if !mesh.uvs.is_empty() {
        bevy_mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, mesh.uvs);
    }
    bevy_mesh
}

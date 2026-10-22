//! Minimal Bevy viewer: grows a small test tree with a fixed rule (there is
//! no growth simulator yet) and draws its tube mesh. A side panel edits the
//! seed and species, regrowing the tree on every change, and lists the width
//! pins. Right-click a branch for a menu that pins its width. Drag to orbit
//! the camera, right-drag to pan and scroll to zoom.
//!
//! Native: `cargo run -p trees-viewer`
//! Web:    `cd crates/trees-viewer && trunk serve`, then open http://127.0.0.1:8080

use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraOutputMode, ClearColorConfig};
use bevy::camera_controller::pan_orbit_camera::prelude::PanOrbitCamera;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy_egui::{EguiGlobalSettings, EguiPlugin, PrimaryEguiContext};
use trees_core::{Habitat, SlotIndex, Tree, TreeSpecies};
use trees_mesh::{MeshOptions, MeshStyle, Topology, TreeMesh, TubeOptions, build_mesh};

use branches::{GrownTree, ShownTree, WidthPins};
use editor::TreeParams;

mod branches;
mod camera;
mod editor;
mod leaf_debug;
mod pick;

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
        .add_plugins(EguiPlugin::default())
        .add_plugins((
            camera::CameraControlsPlugin,
            editor::EditorPlugin,
            branches::BranchesPlugin,
        ))
        .init_resource::<leaf_debug::LeafDebug>()
        .add_systems(Startup, (setup, leaf_debug::setup))
        .add_systems(
            Update,
            (
                regrow.run_if(resource_changed::<TreeParams>),
                show_tree,
                leaf_debug::show_leaf_debug,
            )
                .chain(),
        )
        .run();
}

/// Where the test tree is planted.
const TREE_ROOT: Vec3 = Vec3::ZERO;

/// The viewer's world as the growth logic sees it: everything below the
/// ground plane is solid.
#[derive(Resource)]
struct Scene {
    ground_height: f32,
}

impl Habitat for Scene {
    fn occupied(&self, point: glam::Vec3) -> bool {
        point.y < self.ground_height
    }
}

/// Marks the entity showing the tree; [`show_tree`] replaces its mesh.
#[derive(Component)]
struct TreeView;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut egui_settings: ResMut<EguiGlobalSettings>,
) {
    let scene = Scene { ground_height: 0.0 };
    // Ground disc
    commands.spawn((
        Mesh3d(meshes.add(Circle::new(6.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.35, 0.55, 0.3))),
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
            .with_translation(Vec3::Y * scene.ground_height),
    ));
    // Dirt facing down, so the ground is not see-through from below.
    commands.spawn((
        Mesh3d(meshes.add(Circle::new(6.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 0.2, 0.12))),
        Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
            .with_translation(Vec3::Y * scene.ground_height),
        NotShadowCaster,
    ));
    // The tree's mesh is filled in by `show_tree`.
    commands.spawn((
        TreeView,
        Mesh3d::default(),
        MeshMaterial3d(materials.add(Color::srgb(0.45, 0.32, 0.22))),
    ));
    commands.insert_resource(scene);
    // Sunlight from high in the sky, so that every part of the tree is lit
    // however tall it grows, plus a soft fill for the sides facing away.
    commands.spawn((
        DirectionalLight {
            illuminance: 4_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 10.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight {
        brightness: 600.0,
        ..default()
    });
    // Orbit about the tree's root.
    let eye = TREE_ROOT + Vec3::new(-5.0, 5.0, 22.0);
    commands.spawn((
        Camera3d::default(),
        PanOrbitCamera::default().with_initial_anchor_depth(eye.distance(TREE_ROOT).into()),
        Transform::from_translation(eye).looking_at(TREE_ROOT, Vec3::Y),
    ));
    // The panel gets its own camera drawn over the scene, so that shrinking
    // the scene camera's viewport beside the panel does not clip the panel.
    egui_settings.auto_create_primary_context = false;
    commands.spawn((
        PrimaryEguiContext,
        Camera2d,
        RenderLayers::none(),
        Camera {
            order: 1,
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
    ));
}

/// Grows the tree from scratch with the current parameters. The panel only
/// produces valid species, but if one slips through the last tree stays.
fn regrow(mut commands: Commands, params: Res<TreeParams>, scene: Res<Scene>) {
    match test_tree(&*scene, params.species.clone(), params.seed) {
        Ok(tree) => commands.insert_resource(GrownTree(tree)),
        Err(e) => warn!("kept the last tree: {e}"),
    }
}

/// Applies the width pins to the grown tree and shows the result, whenever
/// either changes.
fn show_tree(
    mut commands: Commands,
    grown: Option<Res<GrownTree>>,
    pins: Res<WidthPins>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut view: Single<&mut Mesh3d, With<TreeView>>,
) {
    let Some(grown) = grown.filter(|g| g.is_changed() || pins.is_changed()) else {
        return;
    };
    let mut tree = grown.0.clone();
    pins.apply(&mut tree);
    let options = MeshOptions {
        style: MeshStyle::Tubes(TubeOptions::default()),
    };
    // Dropping the old handle frees the old mesh.
    view.0 = meshes.add(to_bevy_mesh(build_mesh(&tree, &options)));
    commands.insert_resource(ShownTree(tree));
}

/// A stand-in until the growth simulator exists: every bud grows each step,
/// bent upward a little and jittered by `seed`, with shorter internodes on
/// higher-order branches. Buds that would grow into `habitat`'s occupied
/// space stay dormant. Radii follow the pipe model after every step.
fn test_tree(
    habitat: &impl Habitat,
    species: TreeSpecies,
    seed: u64,
) -> Result<Tree, trees_core::SpeciesError> {
    const STEPS: u32 = 7;
    const MAX_ORDER: u16 = 3;
    const JITTER: f32 = 0.25;
    let internode = species.architecture.internode_length;
    let mut tree = Tree::new(Arc::new(species), seed, TREE_ROOT)?;
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
            let slot_number = match slot {
                SlotIndex::Main => 0,
                SlotIndex::Lateral(i) => i as u64 + 1,
            };
            let key = (id.index() as u64) << 2 | slot_number;
            let jitter = random_vector(seed, key) * JITTER;
            let direction = direction + 0.3 * glam::Vec3::Y + jitter;
            tree.grow(
                habitat,
                id,
                slot,
                direction,
                internode * 0.8f32.powi(order.into()),
            );
        }
        tree.advance_step();
        tree.update_radii();
    }
    Ok(tree)
}

/// A vector with components in [-1, 1), hashed from `seed` and `key` with
/// SplitMix64, so the same seed always gives the same tree.
fn random_vector(seed: u64, key: u64) -> glam::Vec3 {
    let mut state = seed ^ key.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // Top 24 bits as a float in [0, 1), mapped to [-1, 1).
        (z >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
    };
    glam::Vec3::new(next(), next(), next())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foliage_controls_change_placements_without_changing_branches() {
        let species = TreeSpecies::default();
        let original = test_tree(&trees_core::OpenAir, species.clone(), 42).unwrap();
        let mut edited = species;
        edited.foliage.spacing *= 0.5;
        edited.foliage.arrangement = trees_core::BranchingPattern::Opposite;
        edited.foliage.attachment_angle = 0.5;
        edited.foliage.petiole_length = 0.2;
        let changed = test_tree(&trees_core::OpenAir, edited, 42).unwrap();
        assert!(changed.leaves().len() > original.leaves().len());
        assert_eq!(original.len(), changed.len());
        for ((id_a, a), (id_b, b)) in original.iter().zip(changed.iter()) {
            assert_eq!(id_a, id_b);
            assert_eq!(a.position, b.position);
            assert_eq!(a.heading, b.heading);
            assert_eq!(a.radius, b.radius);
        }
    }
}

//! Bevy viewer for occupancy-aware space colonization and tube meshes. A side panel edits the
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
#[cfg(test)]
use trees_core::TreeSpecies;
use trees_core::{Habitat, Tree};
use trees_mesh::{MeshOptions, MeshStyle, Topology, TreeMesh, TubeOptions, build_mesh};

use branches::{GrownTree, ShownTree, WidthPins};
use editor::TreeParams;

mod branches;
mod camera;
mod editor;
mod leaf_debug;
mod marker_debug;
mod pick;
mod units;
mod ternary;

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
        .init_resource::<marker_debug::MarkerDebug>()
        .add_systems(Startup, (setup, leaf_debug::setup))
        .add_systems(
            Update,
            (
                regrow.run_if(resource_changed::<TreeParams>),
                show_tree,
                leaf_debug::show_leaf_debug,
                marker_debug::show,
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
    match grow_tree(&*scene, &params) {
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

/// Bounded synchronous preview using the core occupancy-aware API.
fn grow_tree(
    habitat: &impl Habitat,
    params: &TreeParams,
) -> Result<Tree, Box<dyn std::error::Error>> {
    let sampling = params.sampling()?;
    // Bound work even when parameters are supplied outside the panel.
    if params.species.space.perception_distance / sampling.cell_size > 4.001 {
        return Err("perception distance must not exceed four sampling cells".into());
    }
    let mut tree = Tree::new(Arc::new(params.species.clone()), params.seed, TREE_ROOT)?;
    for _ in 0..6 {
        let buds = tree.iter().map(|(_, n)| n.buds().count()).sum::<usize>();
        if tree.len() + buds > 256 {
            break;
        }
        let added = trees_core::grow_step(&mut tree, habitat, sampling)?;
        tree.update_radii();
        if added.is_empty() {
            break;
        }
    }
    Ok(tree)
}

#[cfg(test)]
fn test_tree(
    habitat: &impl Habitat,
    species: TreeSpecies,
    seed: u64,
) -> Result<Tree, Box<dyn std::error::Error>> {
    grow_tree(
        habitat,
        &TreeParams {
            species,
            seed,
            ..Default::default()
        },
    )
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

    fn geometry(tree: &Tree) -> Vec<(glam::Vec3, f32)> {
        tree.iter().map(|(_, n)| (n.position, n.radius)).collect()
    }

    #[test]
    fn sampler_validation_matches_core() {
        for (spacing, jitter) in [
            (0.0, 0.25),
            (f32::NAN, 0.25),
            (1.25, 0.5),
            (1.25, -0.1),
            (1.25, f32::INFINITY),
        ] {
            assert!(
                TreeParams {
                    point_spacing: spacing,
                    jitter,
                    ..Default::default()
                }
                .sampling()
                .is_err()
            );
        }
        for jitter in [0.0, 0.49] {
            assert!(
                TreeParams {
                    jitter,
                    ..Default::default()
                }
                .sampling()
                .is_ok()
            );
        }
    }

    #[test]
    fn parameter_changes_regrow_reproducibly_and_debug_preserves_tree_and_pins() {
        let mut app = App::new();
        app.init_resource::<TreeParams>()
            .init_resource::<marker_debug::MarkerDebug>()
            .init_resource::<WidthPins>()
            .insert_resource(Scene { ground_height: 0.0 })
            .add_systems(Update, regrow.run_if(resource_changed::<TreeParams>));
        app.update();
        let original = geometry(&app.world().resource::<GrownTree>().0);
        app.world_mut()
            .resource_mut::<WidthPins>()
            .0
            .push((vec![], 0.5));
        for (spacing, jitter) in [(1.5, 0.25), (1.25, 0.4)] {
            {
                let mut params = app.world_mut().resource_mut::<TreeParams>();
                params.point_spacing = spacing;
                params.jitter = jitter;
            }
            app.update();
            let changed = geometry(&app.world().resource::<GrownTree>().0);
            assert_ne!(original, changed);
            let expected = grow_tree(
                &Scene { ground_height: 0.0 },
                app.world().resource::<TreeParams>(),
            )
            .unwrap();
            assert_eq!(changed, geometry(&expected));
            for enabled in [true, false] {
                app.world_mut()
                    .resource_mut::<marker_debug::MarkerDebug>()
                    .enabled = enabled;
                app.update();
                assert_eq!(changed, geometry(&app.world().resource::<GrownTree>().0));
                assert_eq!(app.world().resource::<WidthPins>().0, vec![(vec![], 0.5)]);
            }
        }
        *app.world_mut().resource_mut::<TreeParams>() = TreeParams::default();
        app.update();
        assert_eq!(original, geometry(&app.world().resource::<GrownTree>().0));
    }

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

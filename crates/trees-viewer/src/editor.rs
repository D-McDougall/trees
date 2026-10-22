//! Side panel for editing the seed and species growth/foliage parameters, and for
//! listing the width pins. Any parameter change regrows the whole tree from
//! scratch.
//!
//! Every input is limited to the range [`TreeSpecies::validate`] accepts, so
//! the panel cannot produce an invalid species. Parameters the stand-in test
//! tree does not read yet are shown grayed out and cannot be edited.

use std::f32::consts::{PI, TAU};
use std::ops::RangeInclusive;

use bevy::camera::Viewport;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::egui::{self, DragValue};
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};
use trees_core::{BranchingPattern, SlotIndex, Tree, TreeSpecies, VigorModel};

use crate::branches::{BranchMenu, BranchPath, ShownTree, WidthPins, path_label};
use crate::leaf_debug::LeafDebug;

/// What the tree is grown from. Changing it triggers a regrow.
#[derive(Resource, Clone, Default, PartialEq)]
pub struct TreeParams {
    /// Seed for the tree's randomness.
    pub seed: u64,
    /// Species being edited.
    pub species: TreeSpecies,
}

/// Current width of the panel, in logical pixels. The panel is drawn on
/// egui's background layer, which egui does not count as "over the UI", so
/// the camera uses this to ignore the pointer over the panel.
#[derive(Resource, Default)]
pub struct PanelWidth(pub f32);

/// True if the pointer at `cursor` is over the panel or over an egui popup
/// such as an open drop-down menu, so the 3D view must ignore it.
pub fn pointer_over_panel(egui: &EguiWantsInput, panel: &PanelWidth, cursor: Option<Vec2>) -> bool {
    egui.wants_pointer_input() || cursor.is_some_and(|c| c.x < panel.0)
}

/// Width of the panel when the viewer opens, in logical pixels.
const PANEL_WIDTH: f32 = 300.0;

/// Adds the parameter panel and keeps the 3D view beside it.
pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TreeParams>()
            .init_resource::<PanelWidth>()
            .add_systems(EguiPrimaryContextPass, panel);
    }
}

#[allow(clippy::too_many_arguments)]
fn panel(
    mut contexts: EguiContexts,
    mut params: ResMut<TreeParams>,
    mut leaf_debug: ResMut<LeafDebug>,
    mut pins: ResMut<WidthPins>,
    tree: Option<Res<ShownTree>>,
    mut menu: ResMut<BranchMenu>,
    mut panel_width: ResMut<PanelWidth>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut camera: Single<&mut Camera, With<Camera3d>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    // Edit a copy and write it back only if it differs, so that redrawing the
    // panel does not count as a change.
    let mut edited = params.clone();
    let mut edited_pins = pins.clone();
    let mut debug_enabled = leaf_debug.enabled;
    let mut root = egui::Ui::new(
        ctx.clone(),
        "viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    let width = egui::Panel::left("parameters")
        .default_size(PANEL_WIDTH)
        .resizable(true)
        .show(&mut root, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                tree_ui(ui, &mut edited);
                ui.checkbox(&mut debug_enabled, "Debug leaf disks");
                ui.weak("4-inch disks at blade centers (1 unit = 1 metre).");
                if let Some(tree) = tree.as_ref() {
                    ui.label(format!("{} leaf placements", tree.0.leaves().len()));
                }
                let tree = tree.as_deref().map(|t| &t.0);
                let open = menu.target.as_deref();
                if let Some(path) = pins_ui(ui, tree, open, &mut edited_pins) {
                    if menu.target.as_ref() == Some(&path) {
                        menu.target = None;
                    } else {
                        menu.open_at_branch(path);
                    }
                }
                ui.separator();
                species_ui(ui, &mut edited.species);
            });
        })
        .response
        .rect
        .width();
    if debug_enabled != leaf_debug.enabled {
        leaf_debug.enabled = debug_enabled;
    }
    panel_width.0 = width;
    if edited != *params {
        *params = edited;
    }
    if edited_pins != *pins {
        *pins = edited_pins;
    }

    // Draw the scene in the space right of the panel, so the tree stays
    // centred in what is visible.
    let scale = window.scale_factor();
    let left = ((width * scale) as u32).min(window.physical_width().saturating_sub(1));
    camera.viewport = Some(Viewport {
        physical_position: UVec2::new(left, 0),
        physical_size: UVec2::new(window.physical_width() - left, window.physical_height()),
        ..default()
    });
    Ok(())
}

fn tree_ui(ui: &mut egui::Ui, params: &mut TreeParams) {
    ui.heading("Tree");
    egui::Grid::new("tree").num_columns(2).show(ui, |ui| {
        ui.label("Seed");
        ui.add(DragValue::new(&mut params.seed));
        ui.end_row();

        // A mockup: presets are not wired up yet, so the menu is empty.
        ui.label("Preset");
        egui::ComboBox::from_id_salt("preset")
            .selected_text(&params.species.name)
            .show_ui(ui, |ui| {
                ui.weak("No presets yet");
            });
        ui.end_row();
    });
    ui.weak("Grayed-out parameters have no effect until the growth simulator exists.");
}

/// The list of width pins. A pin is grayed out while its branch is missing
/// from the tree (or is a stump), and its radius is highlighted when the
/// pin is too thin to hold and its node is widened.
///
/// Returns the branch whose label was clicked, to open or close its menu;
/// `open` is the branch whose menu is open now.
fn pins_ui(
    ui: &mut egui::Ui,
    tree: Option<&Tree>,
    open: Option<&[SlotIndex]>,
    pins: &mut WidthPins,
) -> Option<BranchPath> {
    let tree = tree?;
    let mut clicked = None;
    egui::CollapsingHeader::new("Width pins")
        .default_open(true)
        .show(ui, |ui| {
            if pins.0.is_empty() {
                ui.weak("No pins.");
                return;
            }
            let mut unpin = None;
            egui::Grid::new("width pins").num_columns(4).show(ui, |ui| {
                ui.strong("Branch");
                ui.strong("Radius");
                ui.strong("n");
                ui.end_row();
                for (i, (path, r)) in pins.0.iter().enumerate() {
                    let id = tree.node_at(path);
                    match id.and_then(|id| Some((id, tree.pin_exponent(id)?))) {
                        Some((id, n)) => {
                            let label = ui
                                .selectable_label(open == Some(path), format!("#{}", id.index()))
                                .on_hover_text(format!("{}: click to show", path_label(path)));
                            if label.clicked() {
                                clicked = Some(path.clone());
                            }
                            let shown = tree.node(id).radius;
                            if shown > *r {
                                ui.colored_label(
                                    egui::Color32::from_rgb(230, 160, 40),
                                    format!("{r:.4}"),
                                )
                                .on_hover_text(format!("Too thin to hold: widened to {shown:.4}"));
                            } else {
                                ui.label(format!("{r:.4}"));
                            }
                            ui.label(format!("{n:.2}"));
                        }
                        None => {
                            ui.weak(path_label(path))
                                .on_hover_text("This branch is not in the current tree.");
                            ui.weak(format!("{r:.4}"));
                            ui.weak("–");
                        }
                    }
                    if ui.button("Unpin").clicked() {
                        unpin = Some(i);
                    }
                    ui.end_row();
                }
            });
            if let Some(i) = unpin {
                pins.0.remove(i);
            }
        });
    clicked
}

fn species_ui(ui: &mut egui::Ui, s: &mut TreeSpecies) {
    section(ui, "Architecture", |ui| {
        let a = &mut s.architecture;
        ui.label("Branching");
        egui::ComboBox::from_id_salt("branching")
            .selected_text(format!("{:?}", a.branching))
            .show_ui(ui, |ui| {
                for pattern in [BranchingPattern::Alternate, BranchingPattern::Opposite] {
                    ui.selectable_value(&mut a.branching, pattern, format!("{pattern:?}"));
                }
            });
        ui.end_row();
        angle(ui, "Divergence φ", &mut a.divergence_angle, 0.0..=360.0);
        angle(ui, "Branching ψ", &mut a.branching_angle, 0.0..=180.0);
        angle(ui, "Deflection δ", &mut a.deflection_angle, 0.0..=180.0);
        number(ui, "Internode length", &mut a.internode_length, 0.01..=10.0);
    });

    section(ui, "Foliage", |ui| {
        let f = &mut s.foliage;
        ui.label("Leaves enabled");
        ui.checkbox(&mut f.enabled, "");
        ui.end_row();
        ui.label("Arrangement");
        egui::ComboBox::from_id_salt("foliage arrangement")
            .selected_text(format!("{:?}", f.arrangement))
            .show_ui(ui, |ui| {
                for pattern in [BranchingPattern::Alternate, BranchingPattern::Opposite] {
                    ui.selectable_value(&mut f.arrangement, pattern, format!("{pattern:?}"));
                }
            });
        ui.end_row();
        number(ui, "Leaf spacing", &mut f.spacing, 0.01..=10.0);
        number(ui, "Maximum branch radius", &mut f.max_radius, 0.0..=1.0);
        angle(ui, "Leaf divergence", &mut f.divergence_angle, 0.0..=360.0);
        angle(ui, "Attachment angle", &mut f.attachment_angle, 0.0..=180.0);
        number(ui, "Petiole length", &mut f.petiole_length, 0.0..=2.0);
        number(ui, "Blade length", &mut f.blade_length, 0.0..=2.0);
        number(ui, "Blade width", &mut f.blade_width, 0.0..=2.0);
    });

    section(ui, "Vigor", |ui| {
        ui.disable();
        let v = &mut s.vigor;
        ui.label("Model");
        let priority = matches!(v.model, VigorModel::Priority { .. });
        egui::ComboBox::from_id_salt("vigor model")
            .selected_text(if priority {
                "Priority"
            } else {
                "Borchert–Honda"
            })
            .show_ui(ui, |ui| {
                if ui.selectable_label(!priority, "Borchert–Honda").clicked() {
                    v.model = VigorModel::BorchertHonda;
                }
                if ui.selectable_label(priority, "Priority").clicked() && !priority {
                    v.model = VigorModel::Priority {
                        w_max: 1.0,
                        w_min: 0.0,
                        top_fraction: 0.5,
                    };
                }
            });
        ui.end_row();
        if let VigorModel::Priority {
            w_max,
            w_min,
            top_fraction,
        } = &mut v.model
        {
            number(ui, "w_min", w_min, 0.0..=*w_max);
            number(ui, "w_max", w_max, *w_min..=10.0);
            number(ui, "Top fraction", top_fraction, 0.01..=1.0);
        }
        number(ui, "Apical control λ", &mut v.apical_control, 0.0..=1.0);
        number(ui, "Light exponent κ", &mut v.light_exponent, 0.0..=5.0);
        number(
            ui,
            "Root vigor scale α",
            &mut v.root_vigor_scale,
            0.01..=10.0,
        );
        number(
            ui,
            "Activation threshold",
            &mut v.activation_threshold,
            0.0..=10.0,
        );
        count(ui, "Max shoot length", &mut v.max_shoot_length, 1..=20);
    });

    section(ui, "Tropism", |ui| {
        ui.disable();
        let t = &mut s.tropism;
        angle(
            ui,
            "Lateral set-point θ",
            &mut t.lateral_set_point,
            0.0..=180.0,
        );
        angle(ui, "Main set-point", &mut t.main_set_point, 0.0..=180.0);
        number(ui, "Straightness α", &mut t.straightness, 0.0..=1.0);
        number(ui, "Gravitropism β", &mut t.gravitropism, 0.0..=1.0);
        // These must sum to 1; a triangle picker will replace them, so
        // they stay read-only until then.
        number(ui, "Environment γ", &mut t.environment, 0.0..=1.0);
    });

    section(ui, "Gravimorphism", |ui| {
        ui.disable();
        let g = &mut s.gravimorphism;
        number(ui, "Vertical axis", &mut g.vertical_axis, 0.01..=10.0);
        number(ui, "Horizontal axis", &mut g.horizontal_axis, 0.01..=10.0);
        number(ui, "Vertical offset", &mut g.vertical_offset, -10.0..=10.0);
    });

    section(ui, "Space colonization", |ui| {
        ui.disable();
        let c = &mut s.space;
        number(
            ui,
            "Occupancy radius ρ",
            &mut c.occupancy_radius,
            0.01..=c.perception_distance - 0.01,
        );
        angle(
            ui,
            "Perception angle θ",
            &mut c.perception_angle,
            1.0..=179.0,
        );
        number(
            ui,
            "Perception distance r",
            &mut c.perception_distance,
            c.occupancy_radius + 0.01..=50.0,
        );
        number(
            ui,
            "Markers per vigor",
            &mut c.auto_markers_per_vigor,
            0.0..=100.0,
        );
    });

    section(ui, "Shadow", |ui| {
        ui.disable();
        let sh = &mut s.shadow;
        number(ui, "Full light C", &mut sh.full_light, 0.01..=10.0);
        number(ui, "Strength a", &mut sh.strength, 0.0..=10.0);
        number(ui, "Falloff β", &mut sh.falloff, 1.01..=10.0);
        count(ui, "Depth q_max", &mut sh.depth, 0..=50);
    });

    section(ui, "Shedding", |ui| {
        ui.disable();
        let sh = &mut s.shedding;
        ui.label("Enabled");
        ui.checkbox(&mut sh.enabled, "");
        ui.end_row();
        number(
            ui,
            "Light per internode",
            &mut sh.light_per_internode_threshold,
            0.0..=1.0,
        );
    });

    section(ui, "Thickness", |ui| {
        let th = &mut s.thickness;
        number(ui, "Tip radius", &mut th.tip_radius, 0.001..=1.0);
        number(ui, "Pipe exponent n", &mut th.pipe_exponent, 2.0..=3.0);
    });
}

/// A collapsible group of label/value rows.
fn section(ui: &mut egui::Ui, title: &str, rows: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(title)
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new(title).num_columns(2).show(ui, rows);
        });
}

fn number(ui: &mut egui::Ui, label: &str, value: &mut f32, range: RangeInclusive<f32>) {
    ui.label(label);
    let speed = (range.end() - range.start()) / 200.0;
    ui.add(DragValue::new(value).range(range).speed(speed));
    ui.end_row();
}

fn count(ui: &mut egui::Ui, label: &str, value: &mut u32, range: RangeInclusive<u32>) {
    ui.label(label);
    ui.add(DragValue::new(value).range(range));
    ui.end_row();
}

/// An angle stored in radians, edited in degrees.
fn angle(ui: &mut egui::Ui, label: &str, radians: &mut f32, degrees: RangeInclusive<f32>) {
    ui.label(label);
    // 180f32.to_radians() rounds to just above π, which validation rejects.
    let to_radians = |d: f32| (d.to_radians()).min(if d <= 180.0 { PI } else { TAU });
    let mut value = radians.to_degrees();
    let (lo, hi) = (to_radians(*degrees.start()), to_radians(*degrees.end()));
    let response = ui.add(
        DragValue::new(&mut value)
            .range(degrees)
            .speed(0.5)
            .suffix("°"),
    );
    // Only write back real edits, so the degree round trip never nudges a value.
    if response.changed() {
        *radians = to_radians(value).clamp(lo, hi);
    }
    ui.end_row();
}

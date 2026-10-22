//! Side panel for editing the seed and every species parameter. Any change
//! regrows the whole tree from scratch; an invalid species leaves the last
//! good tree in place and shows why in the panel.

use std::ops::RangeInclusive;

use bevy::camera::Viewport;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::egui::{self, DragValue};
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};
use trees_core::{BranchingPattern, TreeSpecies, VigorModel};

/// What the tree is grown from. Changing it triggers a regrow.
#[derive(Resource, Clone, Default, PartialEq)]
pub struct TreeParams {
    /// Seed for the tree's randomness.
    pub seed: u64,
    /// Species being edited; may be invalid while the user types.
    pub species: TreeSpecies,
}

/// Why the current [`TreeParams`] could not grow a tree, if they couldn't.
#[derive(Resource, Default)]
pub struct GrowError(pub Option<String>);

/// Width of the panel when the viewer opens, in logical pixels.
const PANEL_WIDTH: f32 = 300.0;

/// Adds the parameter panel and keeps the 3D view beside it.
pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TreeParams>()
            .init_resource::<GrowError>()
            .add_systems(EguiPrimaryContextPass, panel);
    }
}

fn panel(
    mut contexts: EguiContexts,
    mut params: ResMut<TreeParams>,
    error: Res<GrowError>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut camera: Single<&mut Camera, With<Camera3d>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    // Edit a copy and write it back only if it differs, so that redrawing the
    // panel does not count as a change.
    let mut edited = params.clone();
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
                parameters_ui(ui, &mut edited, error.0.as_deref());
            });
        })
        .response
        .rect
        .width();
    if edited != *params {
        *params = edited;
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

fn parameters_ui(ui: &mut egui::Ui, params: &mut TreeParams, error: Option<&str>) {
    ui.heading("Tree");
    egui::Grid::new("tree").num_columns(2).show(ui, |ui| {
        ui.label("Seed");
        ui.add(DragValue::new(&mut params.seed));
        ui.end_row();

        ui.label("Preset");
        let presets = [TreeSpecies::default(), TreeSpecies::conifer()];
        egui::ComboBox::from_id_salt("preset")
            .selected_text(&params.species.name)
            .show_ui(ui, |ui| {
                for preset in presets {
                    let name = preset.name.clone();
                    if ui
                        .selectable_label(params.species == preset, name)
                        .clicked()
                    {
                        params.species = preset;
                    }
                }
            });
        ui.end_row();
    });
    if let Some(error) = error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    ui.separator();
    species_ui(ui, &mut params.species);
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
        angle(ui, "Divergence φ", &mut a.divergence_angle, 360.0);
        angle(ui, "Branching ψ", &mut a.branching_angle, 180.0);
        angle(ui, "Deflection δ", &mut a.deflection_angle, 180.0);
        number(ui, "Internode length", &mut a.internode_length, 0.01..=10.0);
    });

    section(ui, "Vigor", |ui| {
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
            number(ui, "w_max", w_max, 0.0..=10.0);
            number(ui, "w_min", w_min, 0.0..=10.0);
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
        let t = &mut s.tropism;
        angle(ui, "Lateral set-point θ", &mut t.lateral_set_point, 180.0);
        angle(ui, "Main set-point", &mut t.main_set_point, 180.0);
        number(ui, "Straightness α", &mut t.straightness, 0.0..=1.0);
        number(ui, "Gravitropism β", &mut t.gravitropism, 0.0..=1.0);
        number(ui, "Environment γ", &mut t.environment, 0.0..=1.0);
        ui.label("");
        let sum = t.straightness + t.gravitropism + t.environment;
        if ui
            .add_enabled(sum > 0.0, egui::Button::new("Normalize weights"))
            .clicked()
        {
            t.straightness /= sum;
            t.gravitropism /= sum;
            t.environment = 1.0 - t.straightness - t.gravitropism;
        }
        ui.end_row();
    });

    section(ui, "Gravimorphism", |ui| {
        let g = &mut s.gravimorphism;
        number(ui, "Vertical axis", &mut g.vertical_axis, 0.01..=10.0);
        number(ui, "Horizontal axis", &mut g.horizontal_axis, 0.01..=10.0);
        number(ui, "Vertical offset", &mut g.vertical_offset, -10.0..=10.0);
    });

    section(ui, "Space colonization", |ui| {
        let c = &mut s.space;
        number(
            ui,
            "Occupancy radius ρ",
            &mut c.occupancy_radius,
            0.01..=20.0,
        );
        angle(ui, "Perception angle θ", &mut c.perception_angle, 179.0);
        number(
            ui,
            "Perception distance r",
            &mut c.perception_distance,
            0.01..=50.0,
        );
        number(
            ui,
            "Markers per vigor",
            &mut c.auto_markers_per_vigor,
            0.0..=100.0,
        );
    });

    section(ui, "Shadow", |ui| {
        let sh = &mut s.shadow;
        number(ui, "Full light C", &mut sh.full_light, 0.01..=10.0);
        number(ui, "Strength a", &mut sh.strength, 0.0..=10.0);
        number(ui, "Falloff β", &mut sh.falloff, 1.01..=10.0);
        count(ui, "Depth q_max", &mut sh.depth, 0..=50);
    });

    section(ui, "Shedding", |ui| {
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
        number(ui, "Pipe exponent n", &mut th.pipe_exponent, 2.0..=3.0);
        number(ui, "Tip radius", &mut th.tip_radius, 0.001..=1.0);
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
fn angle(ui: &mut egui::Ui, label: &str, radians: &mut f32, max_degrees: f32) {
    ui.label(label);
    let mut degrees = radians.to_degrees();
    let response = ui.add(
        DragValue::new(&mut degrees)
            .range(0.0..=max_degrees)
            .speed(0.5)
            .suffix("°"),
    );
    // Only write back real edits, so the degree round trip never nudges a value.
    if response.changed() {
        *radians = degrees.to_radians();
    }
    ui.end_row();
}

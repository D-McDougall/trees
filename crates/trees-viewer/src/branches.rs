//! The branch context menu and the width pins.
//!
//! The viewer regrows its tree from scratch on every parameter change, so
//! node ids are not stable. The menu and the pins name branches by their
//! slot path from the root instead ([`Tree::slot_path`]), and are looked up
//! again in each new tree; a pin whose branch no longer exists is kept, so
//! it comes back if the branch does.
//!
//! - Open the menu: right-click a branch (a press and release without
//!   dragging, so panning does not open it). The branch is outlined while
//!   its menu is open.
//! - Or click a pin in the panel's pin list: the menu opens beside its
//!   branch.
//! - Close it: right-click empty space, or press Escape.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::egui::{self, DragValue};
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};
use trees_core::{NodeId, SlotIndex, Tree};
use trees_mesh::base_ring_radius;

use crate::editor::{PanelWidth, pointer_over_panel};
use crate::pick::pick;

/// A branch, named by the slots leading to it from the root.
pub type BranchPath = Vec<SlotIndex>;

/// The branch whose context menu is open, and where the menu sits.
#[derive(Resource, Default)]
pub struct BranchMenu {
    /// The branch, or `None` while the menu is closed.
    pub target: Option<BranchPath>,
    /// Top-left corner of the menu, in logical pixels.
    pub at: Vec2,
    /// Set to have [`anchor_menu`] move the menu to its branch before it is
    /// next drawn, for a menu opened from somewhere other than the 3D view.
    pub anchor_at_branch: bool,
}

impl BranchMenu {
    /// Opens the menu on `path`, beside the branch on screen.
    pub fn open_at_branch(&mut self, path: BranchPath) {
        self.target = Some(path);
        self.anchor_at_branch = true;
    }
}

/// The user's width pins: each branch's radius, applied to every regrown
/// tree with [`Tree::pin_radius`].
#[derive(Resource, Default, Clone, PartialEq)]
pub struct WidthPins(pub Vec<(BranchPath, f32)>);

impl WidthPins {
    /// The radius `path` is pinned to, if it is pinned.
    pub fn get(&self, path: &[SlotIndex]) -> Option<f32> {
        self.0.iter().find(|(p, _)| p == path).map(|&(_, r)| r)
    }

    /// Pins `path` to `r`, replacing any pin it had.
    pub fn set(&mut self, path: &[SlotIndex], r: f32) {
        match self.0.iter_mut().find(|(p, _)| p == path) {
            Some((_, pin)) => *pin = r,
            None => self.0.push((path.to_vec(), r)),
        }
    }

    /// Removes the pin on `path`, if any.
    pub fn remove(&mut self, path: &[SlotIndex]) {
        self.0.retain(|(p, _)| p != path);
    }

    /// Pins `tree` and updates its radii.
    pub fn apply(&self, tree: &mut Tree) {
        for (path, r) in &self.0 {
            if let Some(id) = tree.node_at(path) {
                tree.pin_radius(id, *r);
            }
        }
        tree.update_radii();
    }
}

/// The tree as grown from the parameters, before width pins.
#[derive(Resource)]
pub struct GrownTree(pub Tree);

/// The tree on screen, with pins applied. Absent until the first growth.
#[derive(Resource)]
pub struct ShownTree(pub Tree);

/// A press that moves further than this, in logical pixels, is a drag.
const CLICK_SLOP: f32 = 4.0;

/// The width slider reaches this many times the unpinned trunk's radius.
const MAX_WIDTH_FACTOR: f32 = 20.0;

const SELECTED_COLOR: Color = Color::srgb(1.0, 0.85, 0.1);
const PIN_COLOR: Color = Color::srgb(0.1, 0.8, 1.0);
const WARNING_COLOR: egui::Color32 = egui::Color32::from_rgb(230, 160, 40);

/// Adds the branch context menu and draws its branch and the pins.
pub struct BranchesPlugin;

impl Plugin for BranchesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BranchMenu>()
            .init_resource::<WidthPins>()
            .add_systems(Update, (open_menu, anchor_menu, draw_markers).chain())
            .add_systems(EguiPrimaryContextPass, menu_window);
    }
}

/// Where the right button went down over the 3D view, and whether the
/// pointer has since moved far enough to make it a drag.
#[derive(Default)]
struct Press {
    start: Option<Vec2>,
    dragged: bool,
}

/// Opens the menu on the branch under a right-click, closes it on a
/// right-click that hits nothing or on Escape.
#[allow(clippy::too_many_arguments)]
fn open_menu(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    egui: Res<EguiWantsInput>,
    panel: Res<PanelWidth>,
    camera: Single<(&Camera, &GlobalTransform), With<Camera3d>>,
    tree: Option<Res<ShownTree>>,
    mut menu: ResMut<BranchMenu>,
    mut press: Local<Press>,
) {
    if keys.just_pressed(KeyCode::Escape) && menu.target.is_some() {
        menu.target = None;
    }
    let cursor = window.cursor_position();
    if buttons.just_pressed(MouseButton::Right) {
        *press = Press {
            start: cursor.filter(|_| !pointer_over_panel(&egui, &panel, cursor)),
            dragged: false,
        };
    }
    if let (Some(start), Some(now)) = (press.start, cursor)
        && now.distance(start) > CLICK_SLOP
    {
        press.dragged = true;
    }
    if !buttons.just_released(MouseButton::Right) {
        return;
    }
    let Press { start, dragged } = std::mem::take(&mut *press);
    let (Some(_), false, Some(cursor), Some(tree)) = (start, dragged, cursor, tree) else {
        return;
    };
    let (camera, transform) = *camera;
    let Ok(ray) = camera.viewport_to_world(transform, cursor) else {
        return;
    };
    menu.target = pick(&tree.0, ray.origin, *ray.direction).and_then(|id| tree.0.slot_path(id));
    menu.at = cursor;
    menu.anchor_at_branch = false;
}

/// Places a menu opened with [`BranchMenu::open_at_branch`] at the middle
/// of its branch as seen by the camera, kept inside the 3D view. A branch
/// behind the camera puts it in the middle of the view.
fn anchor_menu(
    camera: Single<(&Camera, &GlobalTransform), With<Camera3d>>,
    tree: Option<Res<ShownTree>>,
    mut menu: ResMut<BranchMenu>,
) {
    if !menu.anchor_at_branch {
        return;
    }
    let (Some(tree), Some(path)) = (tree, menu.target.as_deref()) else {
        return;
    };
    let (camera, transform) = *camera;
    let Some(view) = camera.logical_viewport_rect() else {
        return;
    };
    let Some(id) = tree.0.node_at(path) else {
        menu.target = None;
        return;
    };
    let node = tree.0.node(id);
    let middle = node.position - node.heading * (node.length * 0.5);
    let at = camera
        .world_to_viewport(transform, middle)
        .unwrap_or(view.center());
    menu.at = at.clamp(view.min, view.max);
    menu.anchor_at_branch = false;
}

/// Draws the open menu, and closes it if its branch left the tree.
fn menu_window(
    mut contexts: EguiContexts,
    mut menu: ResMut<BranchMenu>,
    mut pins: ResMut<WidthPins>,
    shown: Option<Res<ShownTree>>,
    grown: Option<Res<GrownTree>>,
) -> Result {
    let (Some(path), Some(shown), Some(grown)) = (menu.target.clone(), shown, grown) else {
        return Ok(());
    };
    if menu.anchor_at_branch {
        // Not placed yet; drawing it now would flash it at the old spot.
        return Ok(());
    }
    if shown.0.node_at(&path).is_none() {
        menu.target = None;
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    // Edit a copy and write it back only if it differs, so that redrawing
    // the menu does not count as a change.
    let mut edited = pins.clone();
    egui::Area::new("branch menu".into())
        .order(egui::Order::Foreground)
        .fixed_pos(menu.at.to_array())
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_max_width(260.0);
                menu_ui(ui, &shown.0, &grown.0, &path, &mut edited);
            });
        });
    if edited != *pins {
        *pins = edited;
    }
    Ok(())
}

/// The menu for the branch at `path` of `tree`: its radius, whether it is
/// pinned, and a width slider. Moving the slider pins the branch.
fn menu_ui(ui: &mut egui::Ui, tree: &Tree, grown: &Tree, path: &[SlotIndex], pins: &mut WidthPins) {
    let id = tree
        .node_at(path)
        .expect("menu closes when its branch goes");
    let node = tree.node(id);
    ui.strong(format!("Branch #{} ({})", id.index(), path_label(path)));
    ui.label(format!("Radius {:.4}", node.radius));
    if node.stump {
        ui.weak("A stump keeps the radius it had when it was shed.");
    }
    ui.add_enabled_ui(!node.stump, |ui| {
        let pin = pins.get(path);
        let mut pinned = pin.is_some();
        if ui.checkbox(&mut pinned, "Pinned").changed() {
            if pinned {
                pins.set(path, node.radius);
            } else {
                pins.remove(path);
            }
        }
        egui::CollapsingHeader::new("Adjust width")
            .id_salt("adjust width")
            .show(ui, |ui| {
                let tip = tree.species().thickness.tip_radius;
                let max = (grown.node(grown.root()).radius * MAX_WIDTH_FACTOR).max(tip * 2.0);
                let mut r = pin.unwrap_or(node.radius);
                let speed = r * 0.01;
                ui.horizontal(|ui| {
                    let slider = ui.add(
                        egui::Slider::new(&mut r, tip..=max)
                            .logarithmic(true)
                            .show_value(false),
                    );
                    let typed = ui.add(
                        DragValue::new(&mut r)
                            .range(tip..=max)
                            .speed(speed)
                            .max_decimals(4),
                    );
                    if slider.changed() || typed.changed() {
                        pins.set(path, r);
                    }
                });
            });
        if let Some(pin) = pin {
            if node.radius > pin {
                ui.colored_label(
                    WARNING_COLOR,
                    format!("Too thin to hold: widened to {:.4}", node.radius),
                );
            } else if let Some(n) = tree.pin_exponent(id) {
                ui.label(format!("Pipe exponent below: n = {n:.2}"));
            }
        }
    });
}

/// Rings around the internode whose menu is open and at the middle of
/// every pinned one, drawn a little outside the bark so they show over it.
/// They use the radii of the tube rings the mesh draws at each end
/// ([`base_ring_radius`]), not just the node's own radius, so they enclose
/// the tube even when a pin makes one end much thicker than the other.
fn draw_markers(
    mut gizmos: Gizmos,
    tree: Option<Res<ShownTree>>,
    menu: Res<BranchMenu>,
    pins: Res<WidthPins>,
) {
    let Some(tree) = tree else { return };
    let tree = &tree.0;
    for (path, _) in &pins.0 {
        if let Some(id) = tree.node_at(path) {
            let (node, [base, tip]) = (tree.node(id), ring_radii(tree, id));
            let middle = node.position - node.heading * (node.length * 0.5);
            ring(
                &mut gizmos,
                middle,
                node.heading,
                0.5 * (base + tip) * 1.4,
                PIN_COLOR,
            );
        }
    }
    if let Some(id) = menu.target.as_deref().and_then(|p| tree.node_at(p)) {
        let node = tree.node(id);
        let [base, tip] = ring_radii(tree, id).map(|r| r * 1.08);
        ring(&mut gizmos, node.base(), node.heading, base, SELECTED_COLOR);
        ring(
            &mut gizmos,
            node.position,
            node.heading,
            tip,
            SELECTED_COLOR,
        );
        for side in [
            node.lateral,
            node.binormal(),
            -node.lateral,
            -node.binormal(),
        ] {
            gizmos.line(
                node.base() + side * base,
                node.position + side * tip,
                SELECTED_COLOR,
            );
        }
    }
}

/// Radii of the tube rings at the base and tip of internode `id`.
fn ring_radii(tree: &Tree, id: NodeId) -> [f32; 2] {
    [base_ring_radius(tree, id), tree.node(id).radius]
}

/// A circle of `radius` about `center`, facing along `axis`.
fn ring(gizmos: &mut Gizmos, center: Vec3, axis: Vec3, radius: f32, color: Color) {
    let rotation = Quat::from_rotation_arc(Vec3::Z, axis);
    gizmos
        .circle(Isometry3d::new(center, rotation), radius, color)
        .resolution(24);
}

/// A short label for `path`, such as `M·L0·M`; the root is `root`.
pub fn path_label(path: &[SlotIndex]) -> String {
    if path.is_empty() {
        return "root".into();
    }
    let slots: Vec<String> = path
        .iter()
        .map(|s| match s {
            SlotIndex::Main => "M".into(),
            SlotIndex::Lateral(i) => format!("L{i}"),
        })
        .collect();
    slots.join("·")
}

//! Selecting branches and showing width pins.
//!
//! The viewer regrows its tree from scratch on every parameter change, so
//! node ids are not stable. The selection and the pins name branches by
//! their slot path from the root instead ([`Tree::slot_path`]), and are
//! looked up again in each new tree; a pin whose branch no longer exists is
//! kept, so it comes back if the branch does.
//!
//! - Select: click a branch (a press and release without dragging, so
//!   orbiting does not select). Clicking empty space clears the selection.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::input::EguiWantsInput;
use trees_core::{SlotIndex, Tree, TreeNode};

use crate::editor::{PanelWidth, pointer_over_panel};
use crate::pick::pick;

/// A branch, named by the slots leading to it from the root.
pub type BranchPath = Vec<SlotIndex>;

/// The selected branch, if any.
#[derive(Resource, Default)]
pub struct Selection(pub Option<BranchPath>);

/// The user's width pins: each branch's radius, applied to every regrown
/// tree with [`Tree::pin_radius`].
#[derive(Resource, Default, Clone, PartialEq)]
pub struct WidthPins(pub Vec<(BranchPath, f32)>);

impl WidthPins {
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

/// The tree on screen, with pins applied. Absent until the first growth.
#[derive(Resource)]
pub struct ShownTree(pub Tree);

/// A press that moves further than this, in logical pixels, is a drag.
const CLICK_SLOP: f32 = 4.0;

const SELECTED_COLOR: Color = Color::srgb(1.0, 0.85, 0.1);
const PIN_COLOR: Color = Color::srgb(0.1, 0.8, 1.0);

/// Adds branch selection and draws the selection and the pins.
pub struct BranchesPlugin;

impl Plugin for BranchesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selection>()
            .init_resource::<WidthPins>()
            .add_systems(Update, (select_on_click, draw_markers));
    }
}

/// Where the left button went down over the 3D view, and whether the
/// pointer has since moved far enough to make it a drag.
#[derive(Default)]
struct Press {
    start: Option<Vec2>,
    dragged: bool,
}

#[allow(clippy::too_many_arguments)]
fn select_on_click(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    egui: Res<EguiWantsInput>,
    panel: Res<PanelWidth>,
    camera: Single<(&Camera, &GlobalTransform), With<Camera3d>>,
    tree: Option<Res<ShownTree>>,
    mut selection: ResMut<Selection>,
    mut press: Local<Press>,
) {
    let cursor = window.cursor_position();
    if buttons.just_pressed(MouseButton::Left) {
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
    if !buttons.just_released(MouseButton::Left) {
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
    let picked = pick(&tree.0, ray.origin, *ray.direction).and_then(|id| tree.0.slot_path(id));
    if selection.0 != picked {
        selection.0 = picked;
    }
}

/// Rings around the selected internode and at the middle of every pinned
/// one, drawn a little outside the bark so they show over it.
fn draw_markers(
    mut gizmos: Gizmos,
    tree: Option<Res<ShownTree>>,
    selection: Res<Selection>,
    pins: Res<WidthPins>,
) {
    let Some(tree) = tree else { return };
    let tree = &tree.0;
    let ring = |gizmos: &mut Gizmos, node: &TreeNode, center: Vec3, scale: f32, color: Color| {
        let rotation = Quat::from_rotation_arc(Vec3::Z, node.heading);
        gizmos
            .circle(
                Isometry3d::new(center, rotation),
                node.radius * scale,
                color,
            )
            .resolution(24);
    };
    for (path, _) in &pins.0 {
        if let Some(id) = tree.node_at(path) {
            let node = tree.node(id);
            let middle = node.position - node.heading * (node.length * 0.5);
            ring(&mut gizmos, node, middle, 1.4, PIN_COLOR);
        }
    }
    if let Some(id) = selection.0.as_deref().and_then(|p| tree.node_at(p)) {
        let node = tree.node(id);
        let r = node.radius * 1.2;
        for end in [node.base(), node.position] {
            ring(&mut gizmos, node, end, 1.2, SELECTED_COLOR);
        }
        for side in [
            node.lateral,
            node.binormal(),
            -node.lateral,
            -node.binormal(),
        ] {
            gizmos.line(
                node.base() + side * r,
                node.position + side * r,
                SELECTED_COLOR,
            );
        }
    }
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

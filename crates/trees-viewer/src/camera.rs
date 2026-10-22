//! Mouse controls for Bevy's built-in pan-orbit camera.
//!
//! Bevy 0.20 ships the controller but leaves input mapping to the app; this
//! is a trimmed version of the input plugin in Bevy's `pan_orbit_camera`
//! examples. It skips picking: every motion is anchored on the view axis at
//! the controller's last anchor depth, so the camera orbits whatever is in the
//! middle of the screen. While orbiting, a gizmo marks that point.
//!
//! - Orbit: left mouse button + drag
//! - Pan: right or middle mouse button + drag
//! - Zoom: mouse wheel

use bevy::camera_controller::pan_orbit_camera::prelude::*;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::picking::PickingSystems;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

const ORBIT: MouseButton = MouseButton::Left;
const PAN: [MouseButton; 2] = [MouseButton::Right, MouseButton::Middle];

/// Adds the pan-orbit controller and drives every [`PanOrbitCamera`] from
/// the mouse.
pub struct CameraControlsPlugin;

impl Plugin for CameraControlsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(DefaultPanOrbitCameraPlugins).add_systems(
            PreUpdate,
            mouse_input
                .after(PickingSystems::Last)
                .before(PanOrbitCamera::update_camera_positions),
        );
    }
}

fn mouse_input(
    buttons: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut last_cursor: Local<Option<Vec2>>,
    mut cameras: Query<&mut PanOrbitCamera>,
) {
    // Pan stays pinned to the cursor only if it gets cursor deltas in
    // logical pixels, not raw device motion.
    let cursor = window.cursor_position();
    let drag = cursor
        .zip(*last_cursor)
        .map_or(Vec2::ZERO, |(now, then)| now - then);
    *last_cursor = cursor;
    let zoom = scroll.delta.y
        * match scroll.unit {
            MouseScrollUnit::Line => 40.0,
            MouseScrollUnit::Pixel => 1.0,
        };

    for mut camera in &mut cameras {
        let zoom_settled = camera.current_motion.is_zooming_only()
            && camera.current_motion.inputs().is_some_and(|inputs| {
                inputs.zoom_velocity_abs(camera.smoothing.zoom.mul_f32(2.0)) <= 0.0
            });
        if buttons.any_just_released(PAN.into_iter().chain([ORBIT])) || zoom_settled {
            camera.end_move();
        }

        // Only a pointer over the window may start a motion.
        if cursor.is_some() && !camera.is_actively_controlled() {
            if buttons.just_pressed(ORBIT) {
                camera.start_orbit(None);
            } else if buttons.any_just_pressed(PAN) {
                camera.start_pan(None);
            } else if zoom != 0.0 {
                camera.start_zoom(None);
            }
        }

        camera.send_screenspace_input(drag);
        camera.send_zoom_input(zoom);
    }
}

//! A three-way blend control. Coordinates are ordered top, bottom-left,
//! bottom-right; dragging outside the triangle projects onto its nearest edge.
//!
//! Add with `ui.add(TernaryPicker::new(&mut weights, labels))`. The widget
//! updates the borrowed weights in place and marks its response changed only
//! when an interaction changes them. It has no dependency on Bevy resources.
//! Labels and percentage readouts use the same coordinate order.
use bevy_egui::egui::{self, Align2, Color32, FontId, Pos2, Sense, Shape, Stroke, Vec2};

/// An egui widget for choosing three nonnegative weights that sum to one.
///
/// Clicking sets the blend directly; dragging keeps the dot inside the triangle.
pub struct TernaryPicker<'a> {
    value: &'a mut [f32; 3],
    labels: [&'a str; 3],
}

impl<'a> TernaryPicker<'a> {
    /// Borrow a blend and its corner labels (top, bottom-left, bottom-right).
    ///
    /// Input values must be finite, nonnegative, and sum to one. The widget
    /// preserves them until interaction; every edit satisfies that constraint.
    /// The returned response supports the usual egui `changed()` check.
    pub fn new(value: &'a mut [f32; 3], labels: [&'a str; 3]) -> Self {
        Self { value, labels }
    }
}

impl egui::Widget for TernaryPicker<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let width = ui.available_width().clamp(180.0, 280.0);
        let side = width - 24.0;
        let height = side * 3.0_f32.sqrt() / 2.0;
        let (rect, mut response) =
            ui.allocate_exact_size(Vec2::new(width, height + 66.0), Sense::click_and_drag());
        let vertices = [
            Pos2::new(rect.center().x, rect.top() + 24.0),
            Pos2::new(rect.left() + 12.0, rect.top() + 24.0 + height),
            Pos2::new(rect.right() - 12.0, rect.top() + 24.0 + height),
        ];
        if (response.clicked() || response.dragged())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let next = coordinates(pointer, vertices);
            if next != *self.value {
                *self.value = next;
                response.mark_changed();
            }
        }
        let painter = ui.painter();
        let visuals = ui.style().interact(&response);
        painter.add(Shape::convex_polygon(
            vertices.to_vec(),
            ui.visuals().extreme_bg_color,
            visuals.bg_stroke,
        ));
        // Lines of constant weight make the blend easier to read.
        for i in 0..3 {
            for fraction in [0.25, 0.5, 0.75] {
                let a = vertices[i].lerp(vertices[(i + 1) % 3], fraction);
                let b = vertices[i].lerp(vertices[(i + 2) % 3], fraction);
                painter.line_segment([a, b], ui.visuals().widgets.noninteractive.bg_stroke);
            }
        }
        let aligns = [Align2::CENTER_BOTTOM, Align2::LEFT_TOP, Align2::RIGHT_TOP];
        for i in 0..3 {
            let offset = if i == 0 { -5.0 } else { 5.0 };
            painter.text(
                vertices[i] + Vec2::new(0.0, offset),
                aligns[i],
                self.labels[i],
                FontId::proportional(12.0),
                ui.visuals().text_color(),
            );
        }
        let dot = vertices[0]
            + (vertices[1] - vertices[0]) * self.value[1]
            + (vertices[2] - vertices[0]) * self.value[2];
        painter.circle(
            dot,
            6.0,
            ui.visuals().selection.bg_fill,
            Stroke::new(2.0, Color32::WHITE),
        );
        painter.text(
            Pos2::new(rect.center().x, rect.bottom()),
            Align2::CENTER_BOTTOM,
            format!(
                "{:.1}% / {:.1}% / {:.1}%",
                self.value[0] * 100.0,
                self.value[1] * 100.0,
                self.value[2] * 100.0
            ),
            FontId::proportional(12.0),
            ui.visuals().text_color(),
        );
        response
    }
}

/// Convert a screen position to weights for an upright isosceles triangle.
///
/// `v` must contain top, bottom-left, bottom-right vertices with positive width
/// and height. Interior points use barycentric coordinates. Exterior points
/// are projected onto each edge segment and the closest projection wins,
/// including endpoints, so corners can be selected exactly.
fn coordinates(point: Pos2, v: [Pos2; 3]) -> [f32; 3] {
    let bottom = (point.y - v[0].y) / (v[1].y - v[0].y);
    let right = (point.x - v[0].x) / (v[2].x - v[1].x) + bottom / 2.0;
    let raw = [1.0 - bottom, bottom - right, right];
    if raw.iter().all(|w| *w >= 0.0) {
        return raw;
    }
    let mut best = [1.0, 0.0, 0.0];
    let mut distance = f32::INFINITY;
    for i in 0..3 {
        let j = (i + 1) % 3;
        let edge = v[j] - v[i];
        let t = ((point - v[i]).dot(edge) / edge.length_sq()).clamp(0.0, 1.0);
        let d = (point - (v[i] + edge * t)).length_sq();
        if d < distance {
            distance = d;
            best = [0.0; 3];
            best[i] = 1.0 - t;
            best[j] = t;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corners_center_and_outside_stay_on_simplex() {
        let v = [
            Pos2::new(0.5, 0.0),
            Pos2::new(0.0, 1.0),
            Pos2::new(1.0, 1.0),
        ];
        for i in 0..3 {
            let w = coordinates(v[i], v);
            assert_eq!(w[i], 1.0);
        }
        let center = coordinates(Pos2::new(0.5, 2.0 / 3.0), v);
        assert!(center.iter().all(|w| (*w - 1.0 / 3.0).abs() < 1e-6));
        for x in -10..=10 {
            for y in -10..=10 {
                let w = coordinates(Pos2::new(x as f32 / 3.0, y as f32 / 3.0), v);
                assert!(w.iter().all(|w| *w >= 0.0 && *w <= 1.0));
                assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn interior_and_edge_points_round_trip_in_screen_space() {
        let v = [
            Pos2::new(150.0, 25.0),
            Pos2::new(50.0, 200.0),
            Pos2::new(250.0, 200.0),
        ];
        for expected in [
            [0.2, 0.3, 0.5],
            [0.0, 0.5, 0.5],
            [0.5, 0.0, 0.5],
            [0.5, 0.5, 0.0],
        ] {
            let point = v[0] + (v[1] - v[0]) * expected[1] + (v[2] - v[0]) * expected[2];
            let actual = coordinates(point, v);
            for i in 0..3 {
                assert!((actual[i] - expected[i]).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn outside_points_project_onto_nearest_edge_or_corner() {
        let v = [
            Pos2::new(0.0, 0.0),
            Pos2::new(-1.0, 1.0),
            Pos2::new(1.0, 1.0),
        ];
        for (point, expected) in [
            (Pos2::new(0.0, -2.0), [1.0, 0.0, 0.0]),
            (Pos2::new(-3.0, 1.0), [0.0, 1.0, 0.0]),
            (Pos2::new(3.0, 1.0), [0.0, 0.0, 1.0]),
            (Pos2::new(0.0, 2.0), [0.0, 0.5, 0.5]),
            (Pos2::new(-1.0, 0.0), [0.5, 0.5, 0.0]),
            (Pos2::new(1.0, 0.0), [0.5, 0.0, 0.5]),
        ] {
            assert_eq!(coordinates(point, v), expected);
        }
    }
}

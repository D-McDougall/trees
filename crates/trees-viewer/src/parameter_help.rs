//! Literature-backed help shared with the readable parameter reference.
//!
//! Keep prose in the Markdown file so citations, implementation limitations and
//! usage advice have a single source of truth. Only `###` parameter sections
//! are exposed in tooltips; the introduction and bibliography stay in the guide.
use bevy_egui::egui;

const REFERENCE: &str = include_str!("../../../docs/species-parameters.md");

fn entry(label: &str) -> Option<&'static str> {
    REFERENCE.split("\n### ").skip(1).find_map(|section| {
        let (heading, body) = section.split_once("\n\n")?;
        (heading == label).then(|| body.split("\n#").next().unwrap_or(body).trim())
    })
}

fn contents(ui: &mut egui::Ui, label: &str, body: &str) {
    // Long explanations remain readable on short windows and can be scrolled
    // after moving the pointer from the label into the tooltip.
    ui.set_max_width(420.0);
    ui.style_mut().interaction.selectable_labels = true;
    ui.strong(label);
    egui::ScrollArea::vertical()
        .id_salt(("parameter help", label))
        .max_height((ui.ctx().viewport_rect().height() - 100.0).clamp(80.0, 360.0))
        .show(ui, |ui| {
            for paragraph in body.split("\n\n") {
                if paragraph.starts_with("Status:") || paragraph.starts_with("Source:") {
                    ui.weak(paragraph);
                } else {
                    ui.label(paragraph);
                }
            }
        });
}

/// Help on both normal and disabled controls. Disabling simulation must never
/// disable the explanation of what the parameter will eventually do.
pub fn attach(response: egui::Response, label: &str) -> egui::Response {
    let body = entry(label).unwrap_or_else(|| panic!("missing parameter help: {label}"));
    response
        .on_hover_ui(|ui| contents(ui, label, body))
        .on_disabled_hover_ui(|ui| contents(ui, label, body))
}

pub fn label(ui: &mut egui::Ui, text: &str) -> egui::Response {
    attach(ui.label(text), text)
}

/// Label a physical length explicitly, retaining the unadorned reference key.
pub fn length_label(ui: &mut egui::Ui, text: &str) -> egui::Response {
    attach(ui.label(format!("{text} (ft)")), text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reference_entry_has_substantial_help_status_and_source() {
        let mut labels = std::collections::BTreeSet::new();
        for section in REFERENCE.split("\n### ").skip(1) {
            let (name, _) = section.split_once("\n\n").unwrap();
            assert!(labels.insert(name), "duplicate help: {name}");
            let body = entry(name).unwrap();
            assert!(body.starts_with("Status:"), "{name}");
            assert!(body.contains("\n\nSource:"), "{name}");
            assert!(body.split_whitespace().count() >= 70, "short help: {name}");
            assert!(!body.contains("##"), "section leak: {name}");
        }
        assert!(labels.len() >= 45);
    }

    #[test]
    fn help_opens_on_enabled_and_disabled_labels() {
        for enabled in [true, false] {
            let ctx = egui::Context::default();
            ctx.style_mut_of(egui::Theme::Dark, |style| {
                style.interaction.tooltip_delay = 0.0;
                style.interaction.show_tooltips_only_when_still = false;
            });
            let mut rect = egui::Rect::NOTHING;
            let mut id = egui::Id::NULL;
            for frame in 0..5 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(640.0, 480.0),
                    )),
                    time: Some(frame as f64),
                    events: if frame == 0 {
                        vec![]
                    } else {
                        vec![egui::Event::PointerMoved(rect.center())]
                    },
                    ..Default::default()
                };
                let output = ctx.run_ui(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.add_enabled_ui(enabled, |ui| {
                            let response = label(ui, "Light exponent κ");
                            rect = response.rect;
                            id = response.id;
                        });
                    });
                });
                output.drop_without_applying_deltas();
            }
            assert!(
                egui::Tooltip::was_tooltip_open_last_frame(&ctx, id),
                "help did not open with enabled={enabled}"
            );
        }
    }
}

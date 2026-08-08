//! Application-wide egui fonts, spacing, colours, and interaction styling.

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, Stroke};
use std::sync::Arc;

/// A restrained native-style light palette. Explicit foreground colors avoid
/// the macOS system-theme mismatch that previously produced white-on-white UI.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::empty();
    fonts.font_data.insert(
        "Ubuntu".into(),
        Arc::new(FontData::from_static(epaint_default_fonts::UBUNTU_LIGHT)),
    );
    fonts.font_data.insert(
        "Hack".into(),
        Arc::new(FontData::from_static(epaint_default_fonts::HACK_REGULAR)),
    );
    fonts.font_data.insert(
        "Emoji".into(),
        Arc::new(FontData::from_static(
            epaint_default_fonts::NOTO_EMOJI_REGULAR,
        )),
    );
    fonts.families.insert(
        FontFamily::Proportional,
        vec!["Ubuntu".into(), "Emoji".into()],
    );
    fonts.families.insert(
        FontFamily::Monospace,
        vec!["Hack".into(), "Ubuntu".into(), "Emoji".into()],
    );
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Light);
    let mut style = (*ctx.style_of(egui::Theme::Light)).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = Color32::from_rgb(247, 248, 250);
    style.visuals.window_fill = Color32::WHITE;
    style.visuals.extreme_bg_color = Color32::WHITE;
    style.visuals.faint_bg_color = Color32::from_rgb(241, 243, 246);
    style.visuals.override_text_color = Some(Color32::from_rgb(29, 33, 41));
    style.visuals.widgets.noninteractive.fg_stroke =
        Stroke::new(1.0, Color32::from_rgb(29, 33, 41));
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(46, 53, 64));
    style.visuals.widgets.inactive.bg_fill = Color32::WHITE;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(205, 210, 219));
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(235, 240, 248);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(64, 112, 244));
    style.visuals.selection.bg_fill = Color32::from_rgb(221, 231, 255);
    style.visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(50, 94, 220));
    // Dark enough to remain legible on white and pale error backgrounds.
    style.visuals.error_fg_color = Color32::from_rgb(156, 28, 38);
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    ctx.set_style_of(egui::Theme::Light, style);
}

//! The look: a dark stone-and-gold palette in the spirit of the game's menus, and a small triforce
//! drawn with shapes (fonts do not carry the glyph, so it is painted).

use egui::{Color32, Stroke};

pub const BG: Color32 = Color32::from_rgb(0x0B, 0x12, 0x17);
pub const PANEL: Color32 = Color32::from_rgb(0x14, 0x1F, 0x27);
pub const STONE: Color32 = Color32::from_rgb(0x1C, 0x2A, 0x33);
pub const GOLD: Color32 = Color32::from_rgb(0xD8, 0xB0, 0x4A);
pub const GOLD_DIM: Color32 = Color32::from_rgb(0x8A, 0x71, 0x34);
pub const PARCHMENT: Color32 = Color32::from_rgb(0xEA, 0xE3, 0xCE);
pub const OK: Color32 = Color32::from_rgb(0x7C, 0xC9, 0x8A);
pub const WARN: Color32 = Color32::from_rgb(0xE3, 0xA5, 0x4F);
pub const WARN_BG: Color32 = Color32::from_rgb(0x35, 0x2A, 0x15);

pub fn apply(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.override_text_color = Some(PARCHMENT);
        v.panel_fill = BG;
        v.window_fill = PANEL;
        v.faint_bg_color = STONE;
        v.extreme_bg_color = Color32::from_rgb(0x08, 0x0D, 0x11);
        v.window_stroke = Stroke::new(1.0, GOLD_DIM);
        v.selection.bg_fill = GOLD;
        v.selection.stroke = Stroke::new(1.0, Color32::from_rgb(0x1A, 0x14, 0x06));
        v.hyperlink_color = GOLD;
        v.widgets.noninteractive.bg_fill = PANEL;
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, PARCHMENT);
        v.widgets.inactive.bg_fill = STONE;
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, PARCHMENT);
        v.widgets.hovered.bg_fill = Color32::from_rgb(0x24, 0x36, 0x41);
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, GOLD);
        v.widgets.active.bg_fill = GOLD_DIM;
        v.widgets.active.fg_stroke = Stroke::new(1.0, PARCHMENT);
        v.widgets.open.bg_fill = PANEL;
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = egui::CornerRadius::same(6);
        }
    });
}

/// Three gold triangles, the middle one upside down, drawn at the cursor.
pub fn triforce(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size * 2.0, size * 1.16), egui::Sense::hover());
    let p = ui.painter();
    let half = size;
    let h = size * 1.16;
    let gold = Stroke::new(1.0, GOLD);
    let tri = |x0: f32, y0: f32, up: bool| {
        let pts = if up {
            vec![
                egui::pos2(x0 + half / 2.0, y0),
                egui::pos2(x0 + half, y0 + h),
                egui::pos2(x0, y0 + h),
            ]
        } else {
            vec![
                egui::pos2(x0, y0),
                egui::pos2(x0 + half, y0),
                egui::pos2(x0 + half / 2.0, y0 + h),
            ]
        };
        egui::Shape::convex_polygon(pts, GOLD.gamma_multiply(0.25), gold)
    };
    p.add(tri(rect.left(), rect.top(), true));
    p.add(tri(rect.left() + half / 2.0, rect.top() + h, false));
    p.add(tri(rect.left() + half, rect.top(), true));
}

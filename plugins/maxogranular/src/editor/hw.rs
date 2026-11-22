//! Hardware-looking primitives: sprites, silkscreen print, lamps, lit push buttons, toggles,
//! LCD windows and steppers.

use nih_plug_egui::egui::{self, epaint::PathStroke, Align2, Color32, CornerRadius, FontFamily, FontId, Mesh, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, TextureHandle, Ui, Vec2};
use std::sync::Arc;

use super::tex::Tex;

/// silkscreen ink on the plates
pub const INK: Color32 = Color32::from_rgba_premultiplied(214, 205, 182, 228);
pub const INK_DIM: Color32 = Color32::from_rgba_premultiplied(140, 134, 118, 160);
pub const AMBER: Color32 = Color32::from_rgb(255, 172, 60);
pub const PHOS_A: Color32 = Color32::from_rgb(255, 178, 70);
pub const PHOS_B: Color32 = Color32::from_rgb(120, 255, 160);
pub const PHOS_M: Color32 = Color32::from_rgb(130, 215, 255);
pub const PHOS_F: Color32 = Color32::from_rgb(150, 255, 120);
pub const LCD_BG: Color32 = Color32::from_rgb(0x12, 0x0b, 0x05);
pub const SCREEN_BG: Color32 = Color32::from_rgb(0x06, 0x09, 0x08);

/// colour of each modulation source (cable, rings)
pub const SRC_COL: [Color32; 16] = [
    Color32::from_rgb(255, 92, 70),
    Color32::from_rgb(255, 148, 54),
    Color32::from_rgb(250, 208, 70),
    Color32::from_rgb(80, 196, 255),
    Color32::from_rgb(96, 240, 170),
    Color32::from_rgb(178, 132, 255),
    Color32::from_rgb(255, 112, 196),
    Color32::from_rgb(236, 236, 228),
    Color32::from_rgb(190, 228, 110),
    Color32::from_rgb(140, 170, 255),
    Color32::from_rgb(255, 170, 140),
    Color32::from_rgb(200, 200, 200),
    Color32::from_rgb(255, 226, 120),
    Color32::from_rgb(255, 226, 120),
    Color32::from_rgb(255, 226, 120),
    Color32::from_rgb(255, 226, 120),
];

pub fn install_fonts(ctx: &egui::Context) {
    use egui::FontData;
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("dejavu".into(), Arc::new(FontData::from_static(include_bytes!("../fonts/DejaVuSans.ttf"))));
    fonts.font_data.insert("cond".into(), Arc::new(FontData::from_static(include_bytes!("../fonts/DejaVuSansCondensed-Bold.ttf"))));
    fonts.font_data.insert("mono".into(), Arc::new(FontData::from_static(include_bytes!("../fonts/DejaVuSansMono-Bold.ttf"))));
    for fam in [FontFamily::Monospace, FontFamily::Proportional] {
        if let Some(list) = fonts.families.get_mut(&fam) {
            list.push("dejavu".into());
        }
    }
    fonts.families.insert(FontFamily::Name("cond".into()), vec!["cond".into(), "dejavu".into()]);
    fonts.families.insert(FontFamily::Name("mono".into()), vec!["mono".into(), "dejavu".into()]);
    ctx.set_fonts(fonts);
}

pub fn visuals(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    let bg = Color32::from_rgb(0x1c, 0x1a, 0x17);
    v.panel_fill = Color32::BLACK;
    v.window_fill = bg;
    v.extreme_bg_color = LCD_BG;
    v.override_text_color = Some(Color32::from_rgb(0xe8, 0xdf, 0xc8));
    v.window_stroke = Stroke::new(1.0f32, Color32::from_rgb(0x4a, 0x44, 0x3a));
    v.selection.bg_fill = Color32::from_rgb(0x6a, 0x48, 0x20);
    v.selection.stroke = Stroke::new(1.0f32, AMBER);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(3);
    }
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x26, 0x23, 0x1f);
    v.widgets.inactive.bg_fill = Color32::from_rgb(0x26, 0x23, 0x1f);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x3a, 0x33, 0x28);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x3a, 0x33, 0x28);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0x4a, 0x3a, 0x22);
    v.popup_shadow = egui::Shadow { offset: [2, 4], blur: 10, spread: 0, color: Color32::from_black_alpha(140) };
    v.window_shadow = egui::Shadow::NONE;
    ctx.set_visuals(v);
}

pub fn cond(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("cond".into()))
}
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("mono".into()))
}

/// Premultiplied colour with alpha 0: blends additively (light).
pub fn add(c: Color32, a: f32) -> Color32 {
    let a = a.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied((c.r() as f32 * a) as u8, (c.g() as f32 * a) as u8, (c.b() as f32 * a) as u8, 0)
}

pub fn with_a(c: Color32, a: f32) -> Color32 {
    let a = a.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (255.0 * a) as u8)
}

pub fn lerp_col(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgba_premultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

pub fn sprite(p: &Painter, t: &TextureHandle, rect: Rect, tint: Color32) {
    p.image(t.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), tint);
}

pub fn sprite_uv(p: &Painter, t: &TextureHandle, rect: Rect, uv: Rect, tint: Color32) {
    p.image(t.id(), rect, uv, tint);
}

/// A sprite rotated by `a` (radians, clockwise) around its centre.
pub fn sprite_rot(p: &Painter, t: &TextureHandle, c: Pos2, size: Vec2, a: f32, tint: Color32) {
    let (s, co) = a.sin_cos();
    let h = size * 0.5;
    let corner = |x: f32, y: f32| Pos2::new(c.x + x * co - y * s, c.y + x * s + y * co);
    let q = [corner(-h.x, -h.y), corner(h.x, -h.y), corner(h.x, h.y), corner(-h.x, h.y)];
    let uvs = [Pos2::new(0.0, 0.0), Pos2::new(1.0, 0.0), Pos2::new(1.0, 1.0), Pos2::new(0.0, 1.0)];
    let mut mesh = Mesh::with_texture(t.id());
    for i in 0..4 {
        mesh.vertices.push(egui::epaint::Vertex { pos: q[i], uv: uvs[i], color: tint });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));
}

/// A soft additive light.
pub fn glow(p: &Painter, tex: &Tex, c: Pos2, r: f32, col: Color32, a: f32) {
    if a <= 0.003 {
        return;
    }
    sprite(p, &tex.glow, Rect::from_center_size(c, Vec2::splat(2.0 * r)), add(col, a));
}

/// Font size at which `text` fits into `max_w` (never below 6.5).
pub fn fit(p: &Painter, text: &str, size: f32, max_w: f32, condensed: bool) -> f32 {
    let f = if condensed { cond(size) } else { mono(size) };
    let w = p.layout_no_wrap(text.to_string(), f, Color32::WHITE).size().x;
    if w > max_w && w > 0.0 {
        (size * max_w / w).max(6.5)
    } else {
        size
    }
}

/// Silkscreen print, shrunk to fit `max_w`.
pub fn ink_fit(p: &Painter, pos: Pos2, text: &str, size: f32, max_w: f32, align: Align2) {
    let s = fit(p, text, size, max_w, true);
    p.text(pos, align, text, cond(s), INK);
}

/// Silkscreen print.
pub fn ink(p: &Painter, pos: Pos2, text: &str, size: f32, align: Align2) {
    p.text(pos, align, text, cond(size), INK);
}
pub fn ink_c(p: &Painter, pos: Pos2, text: &str, size: f32, align: Align2, col: Color32) {
    p.text(pos, align, text, cond(size), col);
}

/// A section title: print with a line running to `end_x`.
pub fn title(p: &Painter, pos: Pos2, text: &str, end_x: f32) {
    let g = p.layout_no_wrap(text.to_string(), cond(11.0), INK);
    let w = g.size().x;
    p.galley(Pos2::new(pos.x, pos.y - g.size().y * 0.5), g, INK);
    if end_x > pos.x + w + 10.0 {
        p.line_segment([Pos2::new(pos.x + w + 6.0, pos.y + 0.5), Pos2::new(end_x, pos.y + 0.5)], Stroke::new(1.0f32, INK_DIM));
    }
}

/// A recessed dark window (LCD, screen opening).
pub fn recess(p: &Painter, r: Rect, fill: Color32) {
    p.rect_filled(r.expand(1.5), CornerRadius::same(3), Color32::from_rgba_premultiplied(0, 0, 0, 170));
    p.rect_filled(r, CornerRadius::same(2), fill);
    // inner shadow at the top, light lip at the bottom
    p.line_segment([Pos2::new(r.left() + 2.0, r.top() + 0.5), Pos2::new(r.right() - 2.0, r.top() + 0.5)], Stroke::new(1.0f32, Color32::from_black_alpha(200)));
    p.line_segment([Pos2::new(r.left() + 2.0, r.bottom() + 2.0), Pos2::new(r.right() - 2.0, r.bottom() + 2.0)], Stroke::new(1.0f32, Color32::from_white_alpha(22)));
}

/// Backlit amber LCD text (with a little bloom).
pub fn lcd_text(p: &Painter, pos: Pos2, text: &str, size: f32, col: Color32, align: Align2) {
    let f = mono(size);
    for (dx, dy) in [(-0.7, 0.0), (0.7, 0.0), (0.0, -0.7), (0.0, 0.7)] {
        p.text(pos + Vec2::new(dx, dy), align, text, f.clone(), add(col, 0.18));
    }
    p.text(pos, align, text, f, col);
}

/// An LCD window with text; the glass gloss on top.
pub fn lcd(p: &Painter, r: Rect, text: &str, size: f32, col: Color32, align: Align2) {
    recess(p, r, LCD_BG);
    let pos = match align.x() {
        egui::Align::Min => Pos2::new(r.left() + 6.0, r.center().y),
        egui::Align::Max => Pos2::new(r.right() - 6.0, r.center().y),
        _ => r.center(),
    };
    let clip = p.with_clip_rect(r.shrink(1.0));
    // shrink the text to fit the window
    let w = p.layout_no_wrap(text.to_string(), mono(size), col).size().x;
    let size = if w > r.width() - 8.0 { (size * (r.width() - 8.0) / w).max(6.5) } else { size };
    lcd_text(&clip, pos, text, size, col, align);
    gloss(p, r);
}

pub fn gloss(p: &Painter, r: Rect) {
    let g = Rect::from_min_max(Pos2::new(r.left() + 1.0, r.top() + 1.0), Pos2::new(r.right() - 1.0, r.top() + r.height() * 0.42));
    let mut m = Mesh::default();
    let top = Color32::from_white_alpha(16);
    let bot = Color32::from_white_alpha(0);
    m.colored_vertex(g.left_top(), top);
    m.colored_vertex(g.right_top(), top);
    m.colored_vertex(g.right_bottom(), bot);
    m.colored_vertex(g.left_bottom(), bot);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(Shape::mesh(m));
}

/// Draws a 3-slice sprite (`btn_on` / `btn_off`): the texture is a 64 x 22 button plus 3 px of
/// shadow around it; the ends (13 px of the texture) keep their shape.
fn slice3(p: &Painter, t: &TextureHandle, rect: Rect, tint: Color32) {
    let s = rect.height() / 22.0;
    let full = rect.expand(3.0 * s);
    let cap = (13.0 * s).min(full.width() * 0.5);
    let u = 13.0 / 70.0;
    let rows = [(full.left(), full.left() + cap, 0.0, u), (full.left() + cap, full.right() - cap, u, 1.0 - u), (full.right() - cap, full.right(), 1.0 - u, 1.0)];
    for (x0, x1, u0, u1) in rows {
        if x1 > x0 {
            sprite_uv(p, t, Rect::from_min_max(Pos2::new(x0, full.top()), Pos2::new(x1, full.bottom())), Rect::from_min_max(Pos2::new(u0, 0.0), Pos2::new(u1, 1.0)), tint);
        }
    }
}

/// An illuminated push button; `lit` 0..1 fades the light (animated by the caller).
pub fn lit_button(ui: &mut Ui, tex: &Tex, rect: Rect, id: egui::Id, text: &str, lit: f32, col: Color32, tip: &str) -> Response {
    let mut resp = ui.interact(rect, id, Sense::click());
    if !tip.is_empty() {
        resp = resp.on_hover_text(tip);
    }
    let p = ui.painter();
    let pressed = resp.is_pointer_button_down_on();
    let r = if pressed { rect.translate(Vec2::new(0.0, 0.8)) } else { rect };
    slice3(p, &tex.btn_off, r, Color32::WHITE);
    if lit > 0.01 {
        let tint = lerp_col(Color32::WHITE, col, 0.55);
        slice3(p, &tex.btn_on, r, with_a(tint, lit));
        // light spilling around the cap
        let c = r.center();
        let w = r.width();
        sprite(p, &tex.glow, Rect::from_center_size(c, Vec2::new(w * 1.5, r.height() * 2.6)), add(col, 0.22 * lit));
    }
    if resp.hovered() && !pressed {
        p.rect_filled(r.shrink(2.0), CornerRadius::same(2), Color32::from_white_alpha(10));
    }
    let tc = lerp_col(Color32::from_rgb(0xb8, 0xb0, 0x9c), Color32::from_rgb(0x2a, 0x1c, 0x0c), lit);
    let fs = fit(p, text, (r.height() * 0.48).clamp(8.0, 11.0), r.width() - 10.0, true);
    p.text(r.center() + Vec2::new(0.0, 0.5), Align2::CENTER_CENTER, text, cond(fs), tc);
    resp
}

/// A bat-handle toggle switch (drawn ~24 x 33); returns the response (click = flip).
pub fn toggle(ui: &mut Ui, tex: &Tex, c: Pos2, id: egui::Id, on: bool, tip: &str) -> Response {
    let size = Vec2::new(24.0, 33.0);
    let rect = Rect::from_center_size(c, size);
    let resp = ui.interact(rect, id, Sense::click());
    let resp = if tip.is_empty() { resp } else { resp.on_hover_text(tip) };
    sprite(ui.painter(), if on { &tex.toggle_up } else { &tex.toggle_down }, rect, Color32::WHITE);
    resp
}

/// A jewel lamp: `level` 0..1.
pub fn jewel(p: &Painter, tex: &Tex, c: Pos2, size: f32, color: usize, level: f32) {
    let (on, off) = &tex.lamp[color.min(2)];
    let r = Rect::from_center_size(c, Vec2::splat(size));
    sprite(p, off, r, Color32::WHITE);
    let lv = level.clamp(0.0, 1.0);
    if lv > 0.01 {
        sprite(p, on, r, with_a(Color32::WHITE, lv));
        let col = [AMBER, Color32::from_rgb(90, 255, 120), Color32::from_rgb(255, 60, 40)][color.min(2)];
        glow(p, tex, c, size * 1.6, col, 0.35 * lv);
    }
}

/// A small chrome arrow button for steppers.
pub fn arrow_btn(ui: &mut Ui, rect: Rect, id: egui::Id, left: bool, tip: &str) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let resp = if tip.is_empty() { resp } else { resp.on_hover_text(tip) };
    let p = ui.painter();
    let down = resp.is_pointer_button_down_on();
    p.rect_filled(rect.translate(Vec2::new(0.6, 1.2)), CornerRadius::same(3), Color32::from_black_alpha(140));
    let r = if down { rect.translate(Vec2::new(0.0, 0.7)) } else { rect };
    let mut m = Mesh::default();
    let top = if resp.hovered() { Color32::from_gray(96) } else { Color32::from_gray(78) };
    let bot = Color32::from_gray(34);
    m.colored_vertex(r.left_top(), top);
    m.colored_vertex(r.right_top(), top);
    m.colored_vertex(r.right_bottom(), bot);
    m.colored_vertex(r.left_bottom(), bot);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(Shape::mesh(m));
    p.rect_stroke(r, CornerRadius::same(2), Stroke::new(1.0f32, Color32::from_black_alpha(160)), egui::StrokeKind::Inside);
    let c = r.center();
    let s = r.height().min(r.width()) * 0.22;
    let pts = if left { vec![Pos2::new(c.x - s, c.y), Pos2::new(c.x + s * 0.8, c.y - s), Pos2::new(c.x + s * 0.8, c.y + s)] } else { vec![Pos2::new(c.x + s, c.y), Pos2::new(c.x - s * 0.8, c.y - s), Pos2::new(c.x - s * 0.8, c.y + s)] };
    p.add(Shape::convex_polygon(pts, Color32::from_rgb(0xdc, 0xd2, 0xb8), Stroke::NONE));
    resp
}

/// LCD with ◀ ▶; a click on the window opens the list. Returns the picked index.
#[allow(clippy::too_many_arguments)]
pub fn stepper(ui: &mut Ui, rect: Rect, id: &str, names: &[String], cur: usize, shown: &str, col: Color32, tip: &str) -> Option<usize> {
    let mut out = None;
    let bw = (rect.height() * 0.85).round();
    let lr = Rect::from_min_size(rect.min, Vec2::new(bw, rect.height()));
    let rr = Rect::from_min_size(Pos2::new(rect.right() - bw, rect.top()), Vec2::new(bw, rect.height()));
    let mid = Rect::from_min_max(Pos2::new(rect.left() + bw + 3.0, rect.top()), Pos2::new(rect.right() - bw - 3.0, rect.bottom()));
    let n = names.len().max(1);
    if arrow_btn(ui, lr, egui::Id::new((id, "l")), true, "").clicked() {
        out = Some((cur + n - 1) % n);
    }
    if arrow_btn(ui, rr, egui::Id::new((id, "r")), false, "").clicked() {
        out = Some((cur + 1) % n);
    }
    let fs = (rect.height() * 0.52).clamp(8.5, 12.0);
    let p = ui.painter().clone();
    lcd(&p, mid, shown, fs, col, Align2::CENTER_CENTER);
    let resp = ui.interact(mid, egui::Id::new((id, "m")), Sense::click());
    let resp = if tip.is_empty() { resp } else { resp.on_hover_text(tip) };
    if resp.hovered() {
        p.rect_filled(mid, CornerRadius::same(2), Color32::from_white_alpha(6));
    }
    let popup = egui::Id::new((id, "popup"));
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup));
    }
    egui::popup::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(mid.width().max(150.0));
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            for (i, nm) in names.iter().enumerate() {
                if let Some(h) = nm.strip_prefix("──") {
                    ui.add_space(3.0);
                    ui.label(egui::RichText::new(h).font(cond(10.0)).color(INK_DIM));
                    continue;
                }
                if ui.selectable_label(i == cur, egui::RichText::new(nm).font(egui::FontId::proportional(12.0))).clicked() {
                    out = Some(i);
                }
            }
        });
    });
    // never land on a heading
    if let Some(i) = out {
        if names.get(i).map(|s| s.starts_with("──")).unwrap_or(false) {
            let fwd = i != (cur + n - 1) % n;
            let mut k = i;
            for _ in 0..n {
                k = if fwd { (k + 1) % n } else { (k + n - 1) % n };
                if !names[k].starts_with("──") {
                    break;
                }
            }
            out = Some(k);
        }
    }
    out
}

/// A polyline with a phosphor glow (wide faint + thin bright).
pub fn trace(p: &Painter, pts: Vec<Pos2>, col: Color32, w: f32, bright: f32) {
    if pts.len() < 2 {
        return;
    }
    p.add(Shape::line(pts.clone(), PathStroke::new(w * 4.0, add(col, 0.07 * bright))));
    p.add(Shape::line(pts.clone(), PathStroke::new(w * 2.2, add(col, 0.16 * bright))));
    p.add(Shape::line(pts, PathStroke::new(w, add(lerp_col(col, Color32::WHITE, 0.25), 0.95 * bright))));
}

/// Vertical gradient quad.
pub fn vgrad(p: &Painter, r: Rect, top: Color32, bot: Color32) {
    let mut m = Mesh::default();
    m.colored_vertex(r.left_top(), top);
    m.colored_vertex(r.right_top(), top);
    m.colored_vertex(r.right_bottom(), bot);
    m.colored_vertex(r.left_bottom(), bot);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(Shape::mesh(m));
}

/// Filled area between a polyline and a base line (alpha fades to the base).
pub fn fill_to(path: &[Pos2], base: f32, col_top: Color32, col_base: Color32) -> Mesh {
    let mut m = Mesh::default();
    for (i, q) in path.iter().enumerate() {
        m.colored_vertex(*q, col_top);
        m.colored_vertex(Pos2::new(q.x, base), col_base);
        if i > 0 {
            let k = (i * 2) as u32;
            m.add_triangle(k - 2, k - 1, k);
            m.add_triangle(k - 1, k + 1, k);
        }
    }
    m
}

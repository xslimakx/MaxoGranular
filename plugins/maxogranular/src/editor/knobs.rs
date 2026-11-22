//! Knobs (with modulation rings), patch jacks and the patch cables.

use nih_plug::prelude::Param;
use nih_plug_egui::egui::{self, epaint::PathStroke, Align2, Color32, CornerRadius, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use std::f32::consts::PI;
use std::sync::atomic::Ordering;

use super::hw::*;
use super::tex::Tex;
use super::{edit, Cable, Cx, Gui};
use crate::kinds::{def, dest_name, modulatable, KnobDef, N_SRC, SRC_NAMES, SRC_TIPS};
use crate::{ModSlot, MAX_MODS};

const A0: f32 = -0.75 * PI;
const SWEEP: f32 = 1.5 * PI;

fn on_ring(c: Pos2, rad: f32, t: f32) -> Pos2 {
    let a = A0 + SWEEP * t;
    Pos2::new(c.x + rad * a.sin(), c.y - rad * a.cos())
}

fn arc(c: Pos2, rad: f32, t0: f32, t1: f32) -> Vec<Pos2> {
    let n = (((t1 - t0).abs()) * 64.0).ceil().max(2.0) as usize;
    (0..=n).map(|i| on_ring(c, rad, t0 + (t1 - t0) * i as f32 / n as f32)).collect()
}

/// What a source puts out right now (for the jack LEDs), 0..1.
pub fn src_level(cx: &Cx, s: usize) -> f32 {
    use crate::kinds::*;
    match s {
        _ if s < S_LFO => cx.shared.env_lvl[s - S_ENV].load(Ordering::Relaxed),
        _ if s < S_VEL => cx.shared.lfo_val[s - S_LFO].load(Ordering::Relaxed),
        S_WHEEL => cx.shared.wheel.load(Ordering::Relaxed),
        _ if s >= S_MACRO => cx.norm(M_MACRO + s - S_MACRO),
        _ => {
            if cx.voices > 0 {
                0.6
            } else {
                0.0
            }
        }
    }
}

/// The silkscreen scale, sprites, rings and value readout of a knob; `disp` is the shown value.
#[allow(clippy::too_many_arguments)]
fn paint_knob(ui: &Ui, tex: &Tex, c: Pos2, r: f32, disp: f32, tint: Color32, bipolar: bool, hot: bool) {
    let p = ui.painter();
    // dial print
    let rt = r + 4.5;
    for i in 0..=10 {
        let t = i as f32 / 10.0;
        let long = i == 0 || i == 10 || (bipolar && i == 5);
        let a = on_ring(c, rt, t);
        let b = on_ring(c, rt + if long { 5.0 } else { 3.0 }, t);
        p.line_segment([a, b], Stroke::new(if long { 1.4f32 } else { 1.0 }, if long { INK } else { INK_DIM }));
    }
    let s = Vec2::splat(r * 2.5);
    sprite(p, &tex.knob_base, Rect::from_center_size(c + Vec2::new(0.0, 0.0), s), Color32::WHITE);
    sprite(p, &tex.knob_cap, Rect::from_center_size(c, s), tint);
    sprite_rot(p, &tex.knob_rot, c, s, A0 + SWEEP * disp, Color32::WHITE);
    sprite(p, &tex.knob_light, Rect::from_center_size(c, s), Color32::from_white_alpha(if hot { 235 } else { 200 }));
}

/// A knob on control `d`. Drag (Shift = fine), wheel, double-click = default, right-click =
/// menu (type a value, the modulations on it), Alt+drag = amount of its modulation. A patch
/// cable dropped on it adds a modulation.
#[allow(clippy::too_many_arguments)]
pub fn knob(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex, d: usize, c: Pos2, r: f32, tint: Color32, label: &str) {
    let kd: &KnobDef = def(d);
    let prm = cx.p.knob(d);
    let norm = prm.unmodulated_normalized_value();
    let id = egui::Id::new(("knob", d));
    let hit = Rect::from_center_size(c, Vec2::splat(2.0 * r + 8.0));
    let can_mod = modulatable(d);
    if can_mod {
        g.targets_next.push((hit, d));
    }
    let resp = ui.interact(hit, id, Sense::click_and_drag());
    let mods: Vec<(usize, ModSlot)> = cx.st.mods.iter().copied().enumerate().filter(|(_, m)| m.dst as usize == d).collect();
    let alt = ui.input(|i| i.modifiers.alt);
    let focus = g.focus_mod.filter(|(_, dst)| *dst as usize == d).or_else(|| mods.first().map(|(_, m)| (m.src, m.dst)));
    let raw_id = id.with("raw");
    let mut adjusting_mod = false;

    if resp.drag_started() {
        if alt && focus.is_some() {
            g.focus_mod = focus;
        } else {
            cx.setter.begin_set_parameter(prm);
            ui.data_mut(|dd| dd.insert_temp(raw_id, norm));
        }
    }
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        let dv = -resp.drag_delta().y / if fine { 1100.0 } else { 210.0 };
        if alt && focus.is_some() {
            adjusting_mod = true;
            let (s, dst) = focus.unwrap();
            edit(cx, |st| {
                if let Some(m) = st.mods.iter_mut().find(|m| m.src == s && m.dst == dst) {
                    m.amount = (m.amount + dv * 1.6).clamp(-1.0, 1.0);
                }
            });
        } else {
            let raw = ui.data(|dd| dd.get_temp::<f32>(raw_id)).unwrap_or(norm) + dv;
            ui.data_mut(|dd| dd.insert_temp(raw_id, raw));
            cx.setter.set_parameter_normalized(prm, raw.clamp(0.0, 1.0));
        }
    }
    if resp.drag_stopped() && !(alt && focus.is_some()) {
        cx.setter.end_set_parameter(prm);
    }
    if resp.double_clicked() {
        cx.set_norm(d, prm.default_normalized_value());
    } else if resp.hovered() {
        let sc = ui.input(|i| i.smooth_scroll_delta.y);
        if sc != 0.0 {
            cx.set_norm(d, norm + sc / 1400.0);
        }
    }
    if resp.clicked() && ui.input(|i| i.modifiers.command) {
        g.typing = Some((d, prm.normalized_value_to_string(norm, false)));
    }
    resp.context_menu(|ui| {
        ui.set_min_width(230.0);
        ui.label(egui::RichText::new(dest_name(d)).font(cond(12.0)).color(AMBER));
        if ui.button("Type a value…").clicked() {
            g.typing = Some((d, prm.normalized_value_to_string(norm, false)));
            ui.close_menu();
        }
        if ui.button("Reset to default").clicked() {
            cx.set_norm(d, prm.default_normalized_value());
            ui.close_menu();
        }
        if can_mod {
            ui.separator();
            if mods.is_empty() {
                ui.label(egui::RichText::new("Drag a cable from a jack onto this knob to modulate it.").font(egui::FontId::proportional(11.0)).color(INK_DIM));
            }
            for (i, m) in &mods {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(SRC_NAMES[m.src as usize]).font(cond(11.0)).color(SRC_COL[m.src as usize]));
                    let mut a = m.amount * 100.0;
                    if ui.add(egui::Slider::new(&mut a, -100.0..=100.0).suffix(" %").fixed_decimals(0)).changed() {
                        let i = *i;
                        edit(cx, |st| {
                            if let Some(x) = st.mods.get_mut(i) {
                                x.amount = a / 100.0;
                            }
                        });
                    }
                    if ui.button("✕").on_hover_text("Remove").clicked() {
                        let (s, dst) = (m.src, m.dst);
                        edit(cx, |st| st.mods.retain(|x| !(x.src == s && x.dst == dst)));
                    }
                });
            }
            if !mods.is_empty() && ui.button("Remove all modulation").clicked() {
                edit(cx, |st| st.mods.retain(|x| x.dst as usize != d));
                ui.close_menu();
            }
        }
    });

    // shown angle: eased, so preset changes swing the knobs round
    let disp_id = id.with("disp");
    let disp = if resp.dragged() && !adjusting_mod { norm } else { g.ease(disp_id, norm, 16.0, cx.dt) };
    if resp.dragged() && !adjusting_mod {
        g.set_ease(disp_id, norm);
    }

    // cable hovering: the knob lights up as a drop target
    if let Some(cb) = &g.cable {
        if can_mod {
            let over = ui.input(|i| i.pointer.hover_pos()).map(|q| hit.contains(q)).unwrap_or(false);
            let pulse = 0.5 + 0.5 * ((cx.now * 6.0) as f32).sin();
            glow(ui.painter(), tex, c, r * 1.9, SRC_COL[cb.src], if over { 0.55 } else { 0.12 + 0.08 * pulse });
        }
    }

    paint_knob(ui, tex, c, r, disp, tint, kd.bipolar, resp.hovered() || resp.dragged());

    // modulation rings + the live value
    if !mods.is_empty() {
        let p = ui.painter();
        // a thin light ring in the gap between the knob and its dial print (several modulations
        // share it: the focused one on top)
        let mut order: Vec<&(usize, ModSlot)> = mods.iter().collect();
        order.sort_by_key(|(_, m)| focus == Some((m.src, m.dst)));
        for (_, m) in order.into_iter().rev().take(3).rev().map(|x| ((), *x)) {
            let m = &m.1;
            let rad = r + 2.2;
            let t1 = (disp + m.amount).clamp(0.0, 1.0);
            let col = SRC_COL[m.src as usize];
            let pts = arc(c, rad, disp, t1);
            let foc = focus == Some((m.src, m.dst));
            p.add(Shape::line(pts.clone(), PathStroke::new(3.2f32, add(col, if foc { 0.22 } else { 0.12 }))));
            p.add(Shape::line(pts, PathStroke::new(1.6f32, add(col, 0.95))));
        }
        let lv = cx.live[d];
        let q = on_ring(c, r + 2.2, lv);
        glow(p, tex, q, 5.0, Color32::WHITE, 0.45);
        p.circle_filled(q, 1.5, Color32::from_rgb(255, 250, 235));
    }

    // label, or the value while touching it
    let vr = Rect::from_center_size(Pos2::new(c.x, c.y + r + 12.0), Vec2::new((2.0 * r + 30.0).max(62.0), 14.0));
    let typing = g.typing.as_ref().filter(|(td, _)| *td == d).map(|(_, s)| s.clone());
    if let Some(mut buf) = typing {
        let te = egui::TextEdit::singleline(&mut buf).font(mono(10.0)).horizontal_align(egui::Align::Center).desired_width(vr.width()).margin(egui::Margin::symmetric(2, 0)).text_color(AMBER);
        let rt = ui.put(vr.expand(2.0), te);
        if !rt.has_focus() && !rt.lost_focus() {
            rt.request_focus();
        }
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if esc {
            g.typing = None;
        } else if enter || rt.lost_focus() {
            if let Some(v) = prm.string_to_normalized_value(buf.trim()).or_else(|| kd.parse(buf.trim())) {
                cx.set_norm(d, v);
            }
            g.typing = None;
        } else {
            g.typing = Some((d, buf));
        }
    } else if resp.hovered() || resp.dragged() || adjusting_mod {
        let txt = if adjusting_mod || (alt && focus.is_some() && resp.hovered()) {
            let (s, dst) = focus.unwrap();
            let a = cx.st.mods.iter().find(|m| m.src == s && m.dst == dst).map(|m| m.amount).unwrap_or(0.0);
            format!("{} {:+.0}%", SRC_NAMES[s as usize], a * 100.0)
        } else {
            kd.fmt(norm)
        };
        lcd(ui.painter(), vr, &txt, 9.5, AMBER, Align2::CENTER_CENTER);
    } else {
        ink_fit(ui.painter(), vr.center(), label, 9.5, vr.width() - 4.0, Align2::CENTER_CENTER);
    }
    if resp.hovered() && !resp.dragged() {
        let tip = if can_mod { format!("{}\n\nDrag · Shift fine · Double-click reset · Right-click menu · Ctrl+click type\nDrop a cable here to modulate it{}", kd.tip, if mods.is_empty() { "" } else { " · Alt+drag: modulation amount" }) } else { format!("{}\n\nDrag · Shift fine · Double-click reset · Ctrl+click type", kd.tip) };
        resp.on_hover_text(tip);
    }
}

/// A knob on a value of the saved state (envelope curves); returns the new plain value.
#[allow(clippy::too_many_arguments)]
pub fn sknob(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex, id: &str, kd: &KnobDef, v: f32, c: Pos2, r: f32, tint: Color32) -> Option<f32> {
    let eid = egui::Id::new(("sknob", id));
    let norm = kd.norm(v);
    let hit = Rect::from_center_size(c, Vec2::splat(2.0 * r + 8.0));
    let resp = ui.interact(hit, eid, Sense::click_and_drag()).on_hover_text(kd.tip);
    let mut out = None;
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        let dv = -resp.drag_delta().y / if fine { 1100.0 } else { 210.0 };
        out = Some((norm + dv).clamp(0.0, 1.0));
    }
    if resp.double_clicked() {
        out = Some(kd.default_norm());
    }
    let shown = out.unwrap_or(norm);
    let disp = if resp.dragged() {
        g.set_ease(eid, shown);
        shown
    } else {
        g.ease(eid, shown, 16.0, cx.dt)
    };
    paint_knob(ui, tex, c, r, disp, tint, kd.bipolar, resp.hovered() || resp.dragged());
    let vr = Rect::from_center_size(Pos2::new(c.x, c.y + r + 12.0), Vec2::new((2.0 * r + 30.0).max(62.0), 14.0));
    if resp.hovered() || resp.dragged() {
        lcd(ui.painter(), vr, &kd.fmt(shown), 9.5, AMBER, Align2::CENTER_CENTER);
    } else {
        ink_fit(ui.painter(), vr.center(), kd.name, 9.5, vr.width() - 4.0, Align2::CENTER_CENTER);
    }
    out.map(|n| kd.plain(n))
}

/// A patch jack for modulation source `src`: drag a cable from it onto a knob.
pub fn jack(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex, src: usize, c: Pos2, label: &str) {
    g.jacks[src] = c;
    let rect = Rect::from_center_size(c, Vec2::splat(20.0));
    let id = egui::Id::new(("jack", src));
    let resp = ui.interact(rect.expand(2.0), id, Sense::click_and_drag());
    let p = ui.painter();
    sprite(p, &tex.jack, rect, Color32::WHITE);
    // LED: how much this source moves right now; ring if it is patched somewhere
    let used = cx.st.mods.iter().filter(|m| m.src as usize == src).count();
    let led = Pos2::new(c.x + 11.0, c.y - 10.0);
    let lv = src_level(cx, src).abs().clamp(0.0, 1.0);
    p.circle_filled(led, 2.2, Color32::from_rgb(30, 14, 10));
    if used > 0 || lv > 0.0 {
        let a = if used > 0 { 0.35 + 0.65 * lv } else { 0.25 * lv };
        p.circle_filled(led, 1.8, with_a(SRC_COL[src], a));
        glow(p, tex, led, 7.0, SRC_COL[src], 0.5 * a);
    }
    if resp.hovered() || g.cable.as_ref().map(|cb| cb.src == src).unwrap_or(false) {
        p.circle_stroke(c, 10.5, Stroke::new(1.5f32, with_a(SRC_COL[src], 0.8)));
    }
    if !label.is_empty() {
        ink(p, Pos2::new(c.x, c.y + 17.0), label, 8.5, Align2::CENTER_CENTER);
    }
    if resp.drag_started() {
        g.cable = Some(Cable { src, from: c, end: c, vel: Vec2::ZERO });
        g.cable_back = None;
    }
    if resp.drag_stopped() {
        if let Some(cb) = g.cable.take() {
            let pos = ui.input(|i| i.pointer.interact_pos()).unwrap_or(cb.end);
            let hit = g.targets.iter().rev().find(|(r, _)| r.contains(pos)).map(|(_, d)| *d);
            match hit {
                Some(d) if cx.st.mods.len() < MAX_MODS || cx.st.mods.iter().any(|m| m.src as usize == src && m.dst as usize == d) => {
                    let (s, dd) = (src as u8, d as u16);
                    edit(cx, |st| {
                        if !st.mods.iter().any(|m| m.src == s && m.dst == dd) {
                            let amount = if def(d).bipolar { 0.25 } else { 0.4 };
                            st.mods.push(ModSlot { src: s, dst: dd, amount });
                        }
                    });
                    g.focus_mod = Some((s, dd));
                    g.say(format!("{} → {}   (Alt+drag the knob: amount)", SRC_NAMES[src], dest_name(d)), cx.now);
                    g.cable_back = Some((src, cb.end, cb.from, cx.now));
                }
                Some(_) => g.say("All 32 modulation slots are in use", cx.now),
                None => g.cable_back = Some((src, cb.from, cb.end, cx.now)),
            }
        }
    }
    if resp.clicked() && src < 7 {
        g.tab = src;
    }
    let n = used;
    resp.on_hover_text(format!("{}\nDrag a cable from here onto any knob to modulate it.{}", SRC_TIPS[src], if n > 0 { format!("\nPatched to {n} control{}", if n == 1 { "" } else { "s" }) } else { String::new() }));
}

fn draw_cable(p: &egui::Painter, tex: &Tex, from: Pos2, end: Pos2, col: Color32, plug: bool) {
    let dist = from.distance(end);
    let sag = 18.0 + dist * 0.22;
    let mid = Pos2::new((from.x + end.x) * 0.5, (from.y + end.y) * 0.5 + sag);
    let n = 28;
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let a = from.lerp(mid, t);
            let b = mid.lerp(end, t);
            a.lerp(b, t)
        })
        .collect();
    let shadow: Vec<Pos2> = pts.iter().map(|q| *q + Vec2::new(3.0, 6.0)).collect();
    p.add(Shape::line(shadow, PathStroke::new(6.0f32, Color32::from_black_alpha(80))));
    p.add(Shape::line(pts.clone(), PathStroke::new(6.0f32, lerp_col(col, Color32::BLACK, 0.55))));
    p.add(Shape::line(pts.clone(), PathStroke::new(4.2f32, lerp_col(col, Color32::BLACK, 0.15))));
    let hl: Vec<Pos2> = pts.iter().map(|q| *q + Vec2::new(-0.8, -1.2)).collect();
    p.add(Shape::line(hl, PathStroke::new(1.2f32, with_a(lerp_col(col, Color32::WHITE, 0.6), 0.6))));
    // the jack end: a plug sitting in the socket
    p.circle_filled(from, 6.5, Color32::from_gray(30));
    p.circle_filled(from, 4.5, lerp_col(col, Color32::BLACK, 0.3));
    if plug {
        let t = (end - mid).normalized();
        let ang = t.x.atan2(-t.y);
        sprite_rot(p, &tex.plug, end - t * 22.0, Vec2::new(20.0, 48.0), ang, Color32::WHITE);
        let band = end - t * 30.0;
        p.circle_filled(band, 3.0, col);
    }
}

/// The cable being patched, and one springing back.
pub fn cables(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex) {
    let p = ui.painter().clone();
    if let Some(cb) = g.cable.as_mut() {
        if let Some(ptr) = ui.input(|i| i.pointer.hover_pos()) {
            let dt = cx.dt.max(1.0 / 240.0);
            let k = 420.0;
            let damp = 26.0;
            let acc = (ptr - cb.end) * k - cb.vel * damp;
            cb.vel += acc * dt;
            cb.end += cb.vel * dt;
            if cb.end.distance(ptr) > 400.0 {
                cb.end = ptr;
            }
        }
        let (from, end, src) = (cb.from, cb.end, cb.src);
        draw_cable(&p, tex, from, end, SRC_COL[src], true);
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    if let Some((src, from, end, t0)) = g.cable_back {
        let t = ((cx.now - t0) / 0.28) as f32;
        if t >= 1.0 {
            g.cable_back = None;
        } else {
            let e = 1.0 - (1.0 - t).powi(3);
            let cur = end.lerp(from, e);
            draw_cable(&p, tex, from, cur, SRC_COL[src], false);
        }
    }
    let _ = CornerRadius::ZERO;
}


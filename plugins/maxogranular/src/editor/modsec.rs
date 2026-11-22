//! The modulation module: three envelopes and four drawn LFOs on one screen, their jacks.

use nih_plug_egui::egui::{self, epaint::PathStroke, Align2, Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use std::sync::atomic::Ordering;

use super::hw::*;
use super::knobs::{jack, knob, sknob};
use super::tex::Tex;
use super::{edit, edit_waves, Cx, Gui, Overlay, P_MOD};
use crate::dsp::curve;
use crate::kinds::*;
use crate::wave::{bend, Preset, Wave};

const TABS: [&str; 7] = ["ENV 1", "ENV 2", "ENV 3", "LFO 1", "LFO 2", "LFO 3", "LFO 4"];
const CRT: [f32; 4] = [20.0, 58.0, 318.0, 176.0];

static CURVE_A: KnobDef = KnobDef::new("A CURVE", "Attack curve: fast start (left) .. slow start (right).", -1.0, 1.0, 0.0, Scale::Lin, Unit::Pct, true);
static CURVE_D: KnobDef = KnobDef::new("D CURVE", "Decay / release curve: fast (left) .. slow (right).", -1.0, 1.0, -0.6, Scale::Lin, Unit::Pct, true);

pub fn draw(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex) {
    let (x0, y0) = (P_MOD[0], P_MOD[1]);
    let at = |x: f32, y: f32| cx.at(x0 + x, y0 + y);
    let rr = |x: f32, y: f32, w: f32, h: f32| cx.r(x0 + x, y0 + y, w, h);
    let p = ui.painter().clone();

    // tabs with their jacks
    for (i, name) in TABS.iter().enumerate() {
        let x = 14.0 + i as f32 * 76.0;
        let lit = g.ease(egui::Id::new(("tab", i)), if g.tab == i { 1.0 } else { 0.0 }, 14.0, cx.dt);
        if lit_button(ui, tex, rr(x, 12.0, 50.0, 22.0), egui::Id::new(("tabb", i)), name, lit, SRC_COL[i], if i == 0 { "Envelope 1 - also the volume of every note" } else { "" }).clicked() {
            g.tab = i;
        }
        jack(ui, cx, g, tex, i, at(x + 64.0, 23.0), "");
    }
    let mx = g.overlay == Overlay::Matrix;
    let ml = g.ease(egui::Id::new("mtxlit"), if mx { 1.0 } else { 0.0 }, 14.0, cx.dt);
    if lit_button(ui, tex, rr(548.0, 12.0, 64.0, 22.0), egui::Id::new("mtx"), "MATRIX", ml, AMBER, "Every modulation in one list: amounts, remove.").clicked() {
        g.overlay = if mx { Overlay::None } else { Overlay::Matrix };
    }

    let tab = g.tab.min(6);
    let col = SRC_COL[tab];
    let scr = rr(CRT[0], CRT[1], CRT[2], CRT[3]);
    let p0 = p.clone();
    p0.rect_filled(scr, 8.0, SCREEN_BG);
    let since = (cx.now - g.t_open) as f32 - 0.3;
    let bright = (since / 0.7).clamp(0.0, 1.0);
    let ps = p0.with_clip_rect(scr);
    glow(&ps, tex, scr.center(), scr.width() * 0.7, PHOS_M, 0.05 * bright);
    for i in 1..8 {
        let x = scr.left() + scr.width() * i as f32 / 8.0;
        ps.line_segment([Pos2::new(x, scr.top()), Pos2::new(x, scr.bottom())], Stroke::new(1.0f32, add(PHOS_M, 0.04 * bright)));
    }
    for i in 1..4 {
        let y = scr.top() + scr.height() * i as f32 / 4.0;
        ps.line_segment([Pos2::new(scr.left(), y), Pos2::new(scr.right(), y)], Stroke::new(1.0f32, add(PHOS_M, 0.04 * bright)));
    }
    if tab < 3 {
        env_screen(ui, cx, g, tex, &ps, scr, tab, col, bright);
    } else {
        lfo_screen(ui, cx, g, tex, &ps, scr, tab - 3, col, bright);
    }
    let (bz, gl) = &tex.crt[1];
    sprite(&p0, gl, scr, Color32::WHITE);
    sprite(&p0, bz, Rect::from_min_size(scr.min - Vec2::splat(17.0), scr.size() + Vec2::splat(34.0)), Color32::WHITE);

    // controls on the right
    let tint = Color32::from_rgb(230, 222, 210);
    let r = 19.0;
    if tab < 3 {
        let e = tab;
        let names = ["ATTACK", "HOLD", "DECAY", "SUSTAIN", "RELEASE"];
        for k in 0..4 {
            knob(ui, cx, g, tex, env_d(e, k), at(390.0 + k as f32 * 66.0, 92.0), r, tint, names[k]);
        }
        knob(ui, cx, g, tex, env_d(e, 4), at(390.0, 172.0), r, tint, names[4]);
        let es = cx.st.envs[e];
        if let Some(v) = sknob(ui, cx, g, tex, &format!("ca{e}"), &CURVE_A, es.curve_a, at(456.0, 172.0), 16.0, tint) {
            edit(cx, |st| st.envs[e].curve_a = v);
        }
        if let Some(v) = sknob(ui, cx, g, tex, &format!("cd{e}"), &CURVE_D, es.curve_d, at(522.0, 172.0), 16.0, tint) {
            edit(cx, |st| st.envs[e].curve_d = v);
        }
        // level needle-ish LED bar
        let lv = cx.shared.env_lvl[e].load(Ordering::Relaxed);
        let bar = rr(578.0, 142.0, 12.0, 60.0);
        recess(&p, bar, LCD_BG);
        let n = 10;
        for i in 0..n {
            let t = (i as f32 + 0.5) / n as f32;
            let seg = Rect::from_min_size(Pos2::new(bar.left() + 2.0, bar.bottom() - 2.0 - (i + 1) as f32 * 5.6 + 1.0), Vec2::new(8.0, 4.0));
            let on = lv >= t;
            p.rect_filled(seg, 1.0, if on { with_a(col, 0.95) } else { with_a(col, 0.10) });
            if on {
                glow(&p, tex, seg.center(), 6.0, col, 0.25);
            }
        }
        ink(&p, at(584.0, 214.0), "LEVEL", 8.0, Align2::CENTER_CENTER);
        let hint = if e == 0 { "ENV 1 IS THE VOLUME OF EVERY NOTE" } else { "PATCH THE JACK ONTO ANY KNOB" };
        ink_c(&p, at(CRT[0] + CRT[2] * 0.5, 262.0), hint, 9.0, Align2::CENTER_CENTER, INK_DIM);
        // drag tip
        ink_c(&p, at(CRT[0] + CRT[2] * 0.5, 276.0), "DRAG THE HANDLES ON THE SCREEN", 8.0, Align2::CENTER_CENTER, INK_DIM);
    } else {
        let k = tab - 3;
        let ls = cx.st.lfos[k].clone();
        knob(ui, cx, g, tex, lfo_d(k, 0), at(380.0, 92.0), r, tint, if ls.sync { "RATE (FREE)" } else { "RATE" });
        knob(ui, cx, g, tex, lfo_d(k, 1), at(442.0, 92.0), r, tint, "SMOOTH");
        knob(ui, cx, g, tex, lfo_d(k, 2), at(504.0, 92.0), r, tint, "PHASE");
        let sl = g.ease(egui::Id::new(("lfosync", k)), if ls.sync { 1.0 } else { 0.0 }, 14.0, cx.dt);
        if lit_button(ui, tex, rr(540.0, 66.0, 72.0, 20.0), egui::Id::new(("lfosyncb", k)), "SYNC", sl, col, "Rate as a note length (follows the tempo).").clicked() {
            edit(cx, |st| st.lfos[k].sync = !st.lfos[k].sync);
        }
        let names: Vec<String> = SYNC_NAMES.iter().map(|s| s.to_string()).collect();
        if let Some(i) = stepper(ui, rr(540.0, 94.0, 72.0, 18.0), &format!("lfolen{k}"), &names, ls.sync_len as usize, SYNC_NAMES[ls.sync_len as usize], if ls.sync { col } else { with_a(col, 0.35) }, "Note length of one cycle (with SYNC on).") {
            edit(cx, |st| {
                st.lfos[k].sync_len = i as u8;
                st.lfos[k].sync = true;
            });
        }
        let tips = ["Free: one clock for all notes", "Retrig: restarts with every note", "1-shot: runs once per note, then holds"];
        for (i, nm) in ["FREE", "RETRIG", "1-SHOT"].iter().enumerate() {
            let lit = g.ease(egui::Id::new(("lfomode", k, i)), if ls.mode as usize == i { 1.0 } else { 0.0 }, 14.0, cx.dt);
            if lit_button(ui, tex, rr(362.0 + i as f32 * 84.0, 150.0, 78.0, 22.0), egui::Id::new(("lfomodeb", k, i)), nm, lit, col, tips[i]).clicked() {
                edit(cx, |st| st.lfos[k].mode = i as u8);
            }
        }
        // speed readout
        let rate = cx.live_plain(lfo_d(k, 0));
        let txt = if ls.sync {
            let beats = SYNC_BEATS[ls.sync_len as usize];
            let tempo = cx.shared.tempo.load(Ordering::Relaxed).max(20.0) as f64;
            format!("{}  ·  {:.2} s", SYNC_NAMES[ls.sync_len as usize], beats * 60.0 / tempo)
        } else {
            format!("{}  ·  {:.2} s", if rate < 10.0 { format!("{rate:.2} Hz") } else { fmt_unit(rate, Unit::Hz) }, 1.0 / rate.max(1e-3))
        };
        lcd(&p, rr(362.0, 186.0, 248.0, 22.0), &txt, 10.5, col, Align2::CENTER_CENTER);
        // output meter (bipolar)
        let v = cx.shared.lfo_val[k].load(Ordering::Relaxed);
        let mr = rr(362.0, 222.0, 248.0, 10.0);
        recess(&p, mr, LCD_BG);
        let c = mr.center().x;
        let x = c + v.clamp(-1.0, 1.0) * mr.width() * 0.48;
        p.rect_filled(Rect::from_min_max(Pos2::new(c.min(x), mr.top() + 2.0), Pos2::new(c.max(x), mr.bottom() - 2.0)), 1.0, with_a(col, 0.85));
        glow(&p, tex, Pos2::new(x, mr.center().y), 9.0, col, 0.4);
        p.line_segment([Pos2::new(c, mr.top()), Pos2::new(c, mr.bottom())], Stroke::new(1.0f32, INK_DIM));
        ink(&p, at(486.0, 244.0), "OUTPUT", 8.0, Align2::CENTER_CENTER);

        // shape tools under the screen
        let y = 252.0;
        let names: Vec<String> = Preset::ALL.iter().map(|q| q.name().to_string()).collect();
        let cur = Preset::ALL.iter().position(|q| Wave::preset(*q) == ls.wave).unwrap_or(usize::MAX);
        let shown = if cur == usize::MAX { "Drawn".to_string() } else { names[cur].clone() };
        if let Some(i) = stepper(ui, rr(20.0, y, 128.0, 22.0), &format!("shape{k}"), &names, if cur == usize::MAX { 0 } else { cur }, &shown, col, "Ready-made shapes (then edit them on the screen).") {
            edit_waves(cx, |st| st.lfos[k].wave = Wave::preset(Preset::ALL[i]));
        }
        let tools: [(&str, &str); 4] = [("INV", "Upside down"), ("REV", "Backwards"), ("×2", "Twice as fast"), ("RND", "A random shape")];
        for (i, (t, tip)) in tools.iter().enumerate() {
            let r = rr(154.0 + i as f32 * 46.5, y, 42.0, 22.0);
            if lit_button(ui, tex, r, egui::Id::new(("lfotool", k, i)), t, 0.0, col, tip).clicked() {
                let seed = (cx.now * 1000.0) as u32 ^ 0x9e37;
                edit_waves(cx, |st| {
                    let w = &mut st.lfos[k].wave;
                    match i {
                        0 => w.invert(),
                        1 => w.reverse(),
                        2 => w.double(),
                        _ => w.randomize(8, seed, false),
                    }
                });
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn env_screen(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex, p: &egui::Painter, scr: Rect, e: usize, col: Color32, bright: f32) {
    let es = cx.st.envs[e];
    let a = cx.live_plain(env_d(e, 0));
    let h = cx.live_plain(env_d(e, 1));
    let d = cx.live_plain(env_d(e, 2));
    let s = cx.live_plain(env_d(e, 3));
    let rl = cx.live_plain(env_d(e, 4));
    let inner = scr.shrink2(Vec2::new(14.0, 22.0));
    let wa = a.max(0.1).sqrt();
    let wh = h.sqrt();
    let wd = d.sqrt();
    let wr = rl.sqrt();
    let sum = wa + wh + wd + wr;
    let ws = sum * 0.28 + 4.0;
    let total = sum + ws;
    let k = inner.width() / total;
    let xa = inner.left();
    let xh = xa + wa * k;
    let xd = xh + wh * k;
    let xs = xd + wd * k;
    let xr = xs + ws * k;
    let xe = xr + wr * k;
    let y_of = |v: f32| inner.bottom() - v * inner.height();
    let mut pts = Vec::new();
    let n = 40;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        pts.push(Pos2::new(xa + (xh - xa) * t, y_of(curve(t, es.curve_a))));
    }
    pts.push(Pos2::new(xd, y_of(1.0)));
    for i in 0..=n {
        let t = i as f32 / n as f32;
        pts.push(Pos2::new(xd + (xs - xd) * t, y_of(1.0 - (1.0 - s) * (1.0 - curve(1.0 - t, -es.curve_d)))));
    }
    pts.push(Pos2::new(xr, y_of(s)));
    for i in 0..=n {
        let t = i as f32 / n as f32;
        pts.push(Pos2::new(xr + (xe - xr) * t, y_of(s * curve(1.0 - t, -es.curve_d))));
    }
    p.add(Shape::mesh(fill_to(&pts, inner.bottom(), add(col, 0.14 * bright), add(col, 0.02 * bright))));
    trace(p, pts.clone(), col, 1.6, bright);
    // stage separators
    for x in [xh, xd, xs, xr] {
        p.line_segment([Pos2::new(x, inner.top()), Pos2::new(x, inner.bottom())], Stroke::new(1.0f32, add(col, 0.10 * bright)));
    }
    // the running note
    let code = cx.shared.env_stage[e].load(Ordering::Relaxed);
    let stage = code >> 24;
    let t = (code & 0xff_ffff) as f32;
    let lv = cx.shared.env_lvl[e].load(Ordering::Relaxed);
    if stage > 0 {
        let x = match stage {
            1 => xa + (xh - xa) * (t / a.max(0.05)).min(1.0),
            2 => xh + (xd - xh) * (t / h.max(0.05)).min(1.0),
            3 => xd + (xs - xd) * (t / d.max(0.5)).min(1.0),
            4 => xs + (xr - xs) * (0.5 + 0.5 * ((t / 900.0) % 1.0)),
            _ => xr + (xe - xr) * (t / rl.max(0.5)).min(1.0),
        };
        let q = Pos2::new(x, y_of(lv));
        p.line_segment([Pos2::new(x, inner.top()), Pos2::new(x, inner.bottom())], Stroke::new(1.0f32, add(col, 0.25 * bright)));
        glow(p, tex, q, 16.0, col, 0.6 * bright);
        p.circle_filled(q, 3.0, add(Color32::WHITE, 0.9 * bright));
    }
    // handles: attack peak, decay/sustain, release end
    let hs = [Pos2::new(xh, y_of(1.0)), Pos2::new(xs, y_of(s)), Pos2::new(xe, y_of(0.0))];
    let ptr = ui.input(|i| i.pointer.hover_pos());
    let resp = ui.interact(scr, egui::Id::new(("envscr", e)), Sense::click_and_drag());
    if resp.drag_started() {
        g.env_drag = ptr.and_then(|q| hs.iter().enumerate().map(|(i, hp)| (i, hp.distance(q))).filter(|(_, dd)| *dd < 16.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i));
    }
    if let Some(hi) = g.env_drag {
        if resp.dragged() {
            let dlt = resp.drag_delta();
            let dk = match hi {
                0 => env_d(e, 0),
                1 => env_d(e, 2),
                _ => env_d(e, 4),
            };
            cx.set_norm(dk, cx.norm(dk) + dlt.x / 260.0);
            if hi == 1 {
                let ds = env_d(e, 3);
                cx.set_norm(ds, cx.norm(ds) - dlt.y / inner.height());
            }
        }
        if resp.drag_stopped() {
            g.env_drag = None;
        }
    }
    for (i, hp) in hs.iter().enumerate() {
        let hot = g.env_drag == Some(i) || ptr.map(|q| q.distance(*hp) < 12.0).unwrap_or(false);
        p.rect_filled(Rect::from_center_size(*hp, Vec2::splat(if hot { 8.0 } else { 6.0 })), 1.0, add(Color32::WHITE, if hot { 0.9 } else { 0.55 } * bright));
        if hot {
            glow(p, tex, *hp, 12.0, col, 0.5);
        }
    }
    if resp.hovered() && g.env_drag.is_none() {
        resp.on_hover_text("Drag the handles: attack, decay + sustain, release");
    }
    let txt = format!("A {}   H {}   D {}   S {:.0}%   R {}", short_ms(a), short_ms(h), short_ms(d), s * 100.0, short_ms(rl));
    lcd_text(p, Pos2::new(scr.left() + 10.0, scr.top() + 11.0), &format!("ENV {}", e + 1), 9.5, with_a(col, 0.9 * bright), Align2::LEFT_CENTER);
    lcd_text(p, Pos2::new(scr.center().x, scr.bottom() - 10.0), &txt, 8.5, with_a(col, 0.75 * bright), Align2::CENTER_CENTER);
}

fn short_ms(v: f32) -> String {
    if v >= 1000.0 {
        format!("{:.2}s", v / 1000.0)
    } else if v >= 10.0 {
        format!("{v:.0}ms")
    } else {
        format!("{v:.1}ms")
    }
}

#[allow(clippy::too_many_arguments)]
fn lfo_screen(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex, p: &egui::Painter, scr: Rect, k: usize, col: Color32, bright: f32) {
    let wave = cx.st.lfos[k].wave.clone();
    let inner = scr.shrink2(Vec2::new(12.0, 22.0));
    let to_s = |x: f32, y: f32| Pos2::new(inner.left() + x * inner.width(), inner.bottom() - y * inner.height());
    let from_s = |q: Pos2| ((q.x - inner.left()) / inner.width(), (inner.bottom() - q.y) / inner.height());
    // centre line
    p.line_segment([to_s(0.0, 0.5), to_s(1.0, 0.5)], Stroke::new(1.0f32, add(col, 0.12 * bright)));
    // the shape, segment by segment (keeps jumps sharp)
    let mut pts = Vec::new();
    for i in 0..wave.pts.len() - 1 {
        let (a, b) = (wave.pts[i], wave.pts[i + 1]);
        let n = (((b.x - a.x) * 160.0).ceil() as usize).clamp(1, 160);
        for j in 0..=n {
            let t = j as f32 / n as f32;
            pts.push(to_s(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * bend(t, a.c)));
        }
    }
    p.add(Shape::mesh(fill_to(&pts, to_s(0.0, 0.5).y, add(col, 0.12 * bright), add(col, 0.01 * bright))));
    trace(p, pts, col, 1.6, bright);

    // playhead
    let ph = cx.shared.lfo_ph[k].load(Ordering::Relaxed);
    let phase = cx.live_plain(lfo_d(k, 2)) / 360.0;
    let x = (ph + phase).rem_euclid(1.0);
    let q = to_s(x, wave.eval(x));
    p.line_segment([Pos2::new(q.x, inner.top() - 10.0), Pos2::new(q.x, inner.bottom() + 10.0)], Stroke::new(1.0f32, add(col, 0.25 * bright)));
    glow(p, tex, q, 16.0, col, 0.6 * bright);
    p.circle_filled(q, 3.0, add(Color32::WHITE, 0.9 * bright));

    // editing: drag points, double-click adds / removes, drag a line bends it
    let resp = ui.interact(scr, egui::Id::new(("lfoscr", k)), Sense::click_and_drag());
    let ptr = resp.hover_pos().or_else(|| resp.interact_pointer_pos());
    let near = |q: Pos2| wave.pts.iter().enumerate().map(|(i, pt)| (i, to_s(pt.x, pt.y).distance(q))).filter(|(_, d)| *d < 9.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i);
    if resp.drag_started() {
        g.lfo_drag = ptr.and_then(near).map(|i| i + 1).or(Some(0));
        if g.lfo_drag == Some(0) {
            // bend: remember the segment
            if let Some(q) = ptr {
                let (xx, _) = from_s(q);
                let seg = wave.pts.iter().rposition(|pt| pt.x <= xx).unwrap_or(0).min(wave.pts.len() - 2);
                ui.data_mut(|dd| dd.insert_temp(egui::Id::new(("lfoseg", k)), seg));
            }
        }
    }
    if resp.dragged() {
        if let (Some(dr), Some(q)) = (g.lfo_drag, ptr) {
            if dr > 0 {
                let (xx, yy) = from_s(q);
                let i = dr - 1;
                edit_waves(cx, |st| st.lfos[k].wave.move_pt(i, xx, yy));
            } else {
                let seg: usize = ui.data(|dd| dd.get_temp(egui::Id::new(("lfoseg", k)))).unwrap_or(0);
                let dy = resp.drag_delta().y;
                edit_waves(cx, |st| {
                    let w = &mut st.lfos[k].wave;
                    if seg + 1 < w.pts.len() {
                        let up = w.pts[seg + 1].y >= w.pts[seg].y;
                        let c = &mut w.pts[seg].c;
                        *c = (*c + dy / 90.0 * if up { 1.0 } else { -1.0 }).clamp(-1.0, 1.0);
                    }
                });
            }
        }
    }
    if resp.drag_stopped() {
        g.lfo_drag = None;
    }
    if resp.double_clicked() {
        if let Some(q) = ptr {
            if let Some(i) = near(q) {
                edit_waves(cx, |st| st.lfos[k].wave.remove(i));
            } else {
                let (xx, yy) = from_s(q);
                edit_waves(cx, |st| {
                    st.lfos[k].wave.insert(xx, yy);
                });
            }
        }
    }
    if resp.secondary_clicked() {
        if let Some(i) = ptr.and_then(near) {
            edit_waves(cx, |st| st.lfos[k].wave.remove(i));
        }
    }
    let hot = ptr.and_then(near);
    for (i, pt) in wave.pts.iter().enumerate() {
        let c = to_s(pt.x, pt.y);
        let h = hot == Some(i) || g.lfo_drag == Some(i + 1);
        p.rect_filled(Rect::from_center_size(c, Vec2::splat(if h { 8.0 } else { 5.0 })), 1.0, add(Color32::WHITE, if h { 0.95 } else { 0.5 } * bright));
        if h {
            glow(p, tex, c, 12.0, col, 0.5);
        }
    }
    if resp.hovered() && g.lfo_drag.is_none() {
        resp.on_hover_text("Drag a point · drag a line up/down to bend it\nDouble-click: add / remove a point · right-click a point: remove");
    }
    lcd_text(p, Pos2::new(scr.left() + 10.0, scr.top() + 11.0), &format!("LFO {}", k + 1), 9.5, with_a(col, 0.9 * bright), Align2::LEFT_CENTER);
    let mode = LFO_MODES[cx.st.lfos[k].mode as usize].to_uppercase();
    lcd_text(p, Pos2::new(scr.right() - 10.0, scr.top() + 11.0), &mode, 8.5, with_a(col, 0.7 * bright), Align2::RIGHT_CENTER);
    let _ = PathStroke::NONE;
}

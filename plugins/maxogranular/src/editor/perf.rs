//! Performance controls (wheels, voice settings, the other modulation jacks) and the keyboard.

use nih_plug_egui::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::sync::atomic::Ordering;

use super::hw::*;
use super::knobs::{jack, knob};
use super::tex::Tex;
use super::{edit, Cx, Gui, P_KEYS, P_PERF};
use crate::kinds::*;

fn wheel(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex, i: usize, slot: Rect, label: &str) {
    let p = ui.painter().clone();
    sprite(&p, &tex.wheel_slot, slot, Color32::WHITE);
    let rub = slot.shrink2(Vec2::new(3.0, 4.0));
    let id = egui::Id::new(("wheel", i));
    let resp = ui.interact(rub, id, Sense::drag()).on_hover_text(if i == 0 { "Pitch bend (springs back)" } else { "Mod wheel - patch its jack onto any knob" });
    let mut v = g.wheel[i];
    if resp.dragged() {
        let dv = -resp.drag_delta().y / 70.0;
        v = if i == 0 { (v + dv).clamp(-1.0, 1.0) } else { (v + dv * 0.5).clamp(0.0, 1.0) };
    } else if i == 0 {
        // spring back to the middle, or show what a MIDI pitch bend does
        if v != 0.0 {
            v *= (-14.0 * cx.dt).exp();
            if v.abs() < 1e-3 {
                v = 0.0;
            }
        } else {
            let eng = cx.shared.bend.load(Ordering::Relaxed);
            g.wheel_roll[0] += (eng - g.wheel_roll[0]) * (1.0 - (-30.0 * cx.dt).exp());
        }
    } else {
        // MIDI CC1 moves it too
        let eng = cx.shared.wheel.load(Ordering::Relaxed);
        if (eng - v).abs() > 1e-3 && !resp.dragged() {
            v = eng;
        }
    }
    let midi_bend = i == 0 && v == 0.0 && g.wheel[0] == 0.0;
    if v != g.wheel[i] {
        g.wheel[i] = v;
        if i == 0 {
            cx.shared.ui_bend.store(v, Ordering::Relaxed);
        } else {
            cx.shared.ui_wheel.store(v, Ordering::Relaxed);
        }
        cx.shared.ui_ctl.fetch_add(1, Ordering::AcqRel);
    }
    // rubber surface scrolls with the value, marker line on the cylinder
    let v = if midi_bend { g.wheel_roll[0] } else { v };
    if i == 0 && !midi_bend {
        g.wheel_roll[0] = v;
    }
    let phi = if i == 0 { v * 0.9 } else { (v - 0.5) * 1.8 };
    let off = -phi / (2.0 * std::f32::consts::PI) * 1.9;
    sprite_uv(&p, &tex.wheel_rubber, rub, Rect::from_min_max(Pos2::new(0.0, off), Pos2::new(1.0, off + 0.66)), Color32::WHITE);
    let rad = rub.height() * 0.5;
    if phi.abs() < 1.35 {
        let y = rub.center().y - rad * phi.sin();
        let a = phi.cos();
        p.line_segment([Pos2::new(rub.left() + 1.0, y), Pos2::new(rub.right() - 1.0, y)], Stroke::new(2.0f32 * a.max(0.3), with_a(Color32::from_rgb(235, 228, 205), 0.85 * a)));
    }
    sprite(&p, &tex.wheel_light, rub, Color32::WHITE);
    ink(&p, Pos2::new(slot.center().x, slot.bottom() + 7.0), label, 8.0, Align2::CENTER_CENTER);
}

pub fn draw(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex) {
    let (x0, y0) = (P_PERF[0], P_PERF[1]);
    let at = |x: f32, y: f32| cx.at(x0 + x, y0 + y);
    let rr = |x: f32, y: f32, w: f32, h: f32| cx.r(x0 + x, y0 + y, w, h);
    let p = ui.painter().clone();

    wheel(ui, cx, g, tex, 0, rr(18.0, 10.0, 26.0, 92.0), "PITCH");
    wheel(ui, cx, g, tex, 1, rr(54.0, 10.0, 26.0, 92.0), "MOD");

    jack(ui, cx, g, tex, S_WHEEL, at(110.0, 30.0), "WHEEL");
    jack(ui, cx, g, tex, S_VEL, at(152.0, 30.0), "VELO");
    jack(ui, cx, g, tex, S_NOTE, at(194.0, 30.0), "NOTE");
    jack(ui, cx, g, tex, S_PRESS, at(110.0, 76.0), "PRESS");
    jack(ui, cx, g, tex, S_RAND, at(152.0, 76.0), "RANDOM");
    p.line_segment([at(214.0, 12.0), at(214.0, 106.0)], Stroke::new(1.0f32, INK_DIM));

    let vs = cx.st.voice;
    let col = AMBER;
    for (i, nm) in ["POLY", "MONO", "LEGATO"].iter().enumerate() {
        let lit = g.ease(egui::Id::new(("vmode", i)), if vs.mode as usize == i { 1.0 } else { 0.0 }, 14.0, cx.dt);
        let tips = ["Every key its own voice", "One voice; a new key restarts the envelopes", "One voice; overlapping keys glide without restarting"];
        if lit_button(ui, tex, rr(222.0 + i as f32 * 57.0, 10.0, 53.0, 20.0), egui::Id::new(("vmodeb", i)), nm, lit, col, tips[i]).clicked() {
            edit(cx, |st| st.voice.mode = i as u8);
        }
    }
    let names: Vec<String> = (1..=crate::MAX_VOICES).map(|i| format!("{i} voices")).collect();
    if let Some(i) = stepper(ui, rr(222.0, 38.0, 80.0, 18.0), "voices", &names, vs.voices as usize - 1, &format!("{} VOICES", vs.voices), col, "Polyphony") {
        edit(cx, |st| st.voice.voices = i as u8 + 1);
    }
    let names: Vec<String> = (0..=24).map(|i| format!("±{i} st")).collect();
    if let Some(i) = stepper(ui, rr(308.0, 38.0, 82.0, 18.0), "bend", &names, vs.bend as usize, &format!("BEND ±{}", vs.bend), col, "Pitch bend range (semitones)") {
        edit(cx, |st| st.voice.bend = i as u8);
    }
    let tint = Color32::from_rgb(230, 222, 210);
    knob(ui, cx, g, tex, M_GLIDE, at(240.0, 84.0), 13.0, tint, "GLIDE");
    knob(ui, cx, g, tex, M_VEL, at(298.0, 84.0), 13.0, tint, "VELOCITY");
    let names: Vec<String> = (0..=6).map(|o| format!("C{}", o - 1)).collect();
    if let Some(i) = stepper(ui, rr(330.0, 72.0, 60.0, 18.0), "oct", &names, g.oct as usize, &format!("C{}", g.oct - 1), col, "Lowest octave of the keyboard below") {
        g.oct = i as i32;
    }
    ink(&p, at(360.0, 100.0), "KEYBOARD", 7.5, Align2::CENTER_CENTER);
}

const WHITE: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];

pub fn keyboard(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex) {
    let bed = cx.r(P_KEYS[0], P_KEYS[1], P_KEYS[2], P_KEYS[3]);
    let p = ui.painter().clone();
    let kw = 28.0;
    let left = bed.left() + 3.0;
    let top = bed.top() + 5.0;
    let wh = 108.0;
    let bw = 17.0;
    let bh = 66.0;
    let base = 12 * (g.oct + 1);
    let n_white = 28;
    // geometry of every key on the keyboard
    let mut whites: Vec<(u8, Rect)> = Vec::new();
    let mut blacks: Vec<(u8, Rect)> = Vec::new();
    for w in 0..n_white {
        let oct = w / 7;
        let k = WHITE[w % 7];
        let note = base + oct as i32 * 12 + k as i32;
        if !(0..128).contains(&note) {
            continue;
        }
        let x = left + w as f32 * kw;
        whites.push((note as u8, Rect::from_min_size(Pos2::new(x, top), Vec2::new(kw, wh))));
        if matches!(k, 0 | 2 | 5 | 7 | 9) {
            let nb = note + 1;
            let shift = match k {
                0 => -2.5,
                2 => 2.5,
                5 => -3.0,
                7 => 0.0,
                _ => 3.0,
            };
            if w + 1 < n_white && nb < 128 {
                blacks.push((nb as u8, Rect::from_min_size(Pos2::new(x + kw + shift - bw * 0.5, top), Vec2::new(bw, bh))));
            }
        }
    }
    // input
    let resp = ui.interact(bed, egui::Id::new("keys"), Sense::click_and_drag());
    let ptr = resp.interact_pointer_pos();
    let hit = |q: Pos2| -> Option<(u8, f32)> {
        for (n, r) in &blacks {
            if r.contains(q) {
                return Some((*n, ((q.y - r.top()) / r.height()).clamp(0.0, 1.0)));
            }
        }
        for (n, r) in &whites {
            if r.contains(q) {
                return Some((*n, ((q.y - r.top()) / r.height()).clamp(0.0, 1.0)));
            }
        }
        None
    };
    let down = resp.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_down());
    let want = if down { ptr.and_then(hit) } else { None };
    let want_note = want.map(|w| w.0);
    if want_note != g.kb_note {
        let mut q = cx.shared.kb.lock();
        if let Some(old) = g.kb_note {
            q.push((old, false, 0.0));
        }
        if let Some((n, y)) = want {
            q.push((n, true, (0.35 + 0.65 * y).clamp(0.05, 1.0)));
        }
        g.kb_note = want_note;
    }
    // draw
    let kbn = g.kb_note;
    let held = |n: u8| cx.shared.notes_on[n as usize].load(Ordering::Relaxed) || kbn == Some(n);
    let felt_p = p.clone();
    for (n, r) in &whites {
        let d = g.ease(egui::Id::new(("key", *n)), if held(*n) { 1.0 } else { 0.0 }, 40.0, cx.dt);
        let rr = r.translate(Vec2::new(0.0, d * 2.0));
        p.rect_filled(r.translate(Vec2::new(1.5, 2.0)), 3.0, Color32::from_black_alpha(90));
        let tint = Color32::from_gray((255.0 - 38.0 * d) as u8);
        sprite(&p, if d > 0.5 { &tex.key_w_dn } else { &tex.key_w }, rr, tint);
        if d > 0.01 {
            // the neighbours' edges shade the sunken key
            vgrad(&p, Rect::from_min_size(rr.min, Vec2::new(rr.width(), 18.0)), Color32::from_black_alpha((110.0 * d) as u8), Color32::from_black_alpha(0));
        }
        if *n % 12 == 0 {
            p.text(Pos2::new(rr.center().x, rr.bottom() - 10.0), Align2::CENTER_CENTER, format!("C{}", *n as i32 / 12 - 2), cond(8.0), Color32::from_rgb(120, 110, 92));
        }
    }
    for (n, r) in &blacks {
        let d = g.ease(egui::Id::new(("key", *n)), if held(*n) { 1.0 } else { 0.0 }, 40.0, cx.dt);
        let rr = r.translate(Vec2::new(0.0, d * 1.6));
        p.rect_filled(r.translate(Vec2::new(2.0, 3.0 - d * 1.5)).expand(0.5), 2.0, Color32::from_black_alpha(120));
        sprite(&p, if d > 0.5 { &tex.key_b_dn } else { &tex.key_b }, rr, Color32::WHITE);

    }
    // the felt strip over the back of the keys
    vgrad(&felt_p, Rect::from_min_size(bed.min, Vec2::new(bed.width(), 7.0)), Color32::from_rgb(60, 14, 12), Color32::from_rgb(30, 6, 6));
    p.line_segment([Pos2::new(bed.left(), bed.top() + 7.0), Pos2::new(bed.right(), bed.top() + 7.0)], Stroke::new(1.0f32, Color32::from_black_alpha(160)));
    // LEDs on top of the felt
    for n in 0..128u8 {
        let d = g.ease.get(&egui::Id::new(("key", n))).copied().unwrap_or(0.0);
        if d > 0.01 {
            if let Some(r) = whites.iter().chain(blacks.iter()).find(|(k, _)| *k == n).map(|(_, r)| *r) {
                let led = Pos2::new(r.center().x, bed.top() + 3.5);
                p.circle_filled(led, 1.6, with_a(Color32::from_rgb(255, 200, 120), d));
                glow(&p, tex, led, 8.0, AMBER, 0.5 * d);
            }
        }
    }
}

/// The computer keyboard plays notes (a piano row, like in most hosts): A W S E D F T G Y H U J
/// K O L P ; and Z / X for the octave. Works by key position, whatever the keyboard layout.
pub fn qwerty(ui: &mut Ui, cx: &Cx, g: &mut Gui) {
    use egui::Key;
    const MAP: [(Key, i32); 18] = [
        (Key::A, 0), (Key::W, 1), (Key::S, 2), (Key::E, 3), (Key::D, 4), (Key::F, 5), (Key::T, 6), (Key::G, 7), (Key::Y, 8),
        (Key::H, 9), (Key::U, 10), (Key::J, 11), (Key::K, 12), (Key::O, 13), (Key::L, 14), (Key::P, 15), (Key::Semicolon, 16), (Key::Quote, 17),
    ];
    // let go of everything when the window loses the keyboard (no hanging notes)
    let focused = ui.ctx().input(|i| i.focused);
    if !focused && !g.qwerty_held.is_empty() {
        let mut q = cx.shared.kb.lock();
        for (_, n) in g.qwerty_held.drain(..) {
            q.push((n, false, 0.0));
        }
    }
    // not while typing into a field
    if ui.ctx().wants_keyboard_input() {
        return;
    }
    let events = ui.ctx().input(|i| i.events.clone());
    let base = 12 * (g.oct + 2);
    for ev in events {
        if let egui::Event::Key { key, physical_key, pressed, repeat, modifiers } = ev {
            if repeat || modifiers.command || modifiers.alt {
                continue;
            }
            let k = physical_key.unwrap_or(key);
            if pressed && k == Key::Z {
                g.oct = (g.oct - 1).max(0);
                continue;
            }
            if pressed && k == Key::X {
                g.oct = (g.oct + 1).min(6);
                continue;
            }
            if let Some((_, off)) = MAP.iter().find(|(mk, _)| *mk == k) {
                if pressed {
                    let n = (base + off).clamp(0, 127) as u8;
                    if !g.qwerty_held.iter().any(|(kk, _)| *kk == k) {
                        g.qwerty_held.push((k, n));
                        cx.shared.kb.lock().push((n, true, 0.8));
                    }
                } else if let Some(i) = g.qwerty_held.iter().position(|(kk, _)| *kk == k) {
                    let (_, n) = g.qwerty_held.remove(i);
                    cx.shared.kb.lock().push((n, false, 0.0));
                }
            }
        }
    }
}

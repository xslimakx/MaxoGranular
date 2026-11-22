//! Filter and effects modules.

use nih_plug_egui::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::sync::atomic::Ordering;

use super::hw::*;
use super::knobs::knob;
use super::tex::Tex;
use super::{edit, Cx, Gui, P_FILTER, P_FX};
use crate::kinds::*;

/// |H| of the filter at f (analog prototype of the engine's SVF), dB.
fn response(ftype: u8, f: f32, fc: f32, res: f32) -> f32 {
    let r1 = (1.0 - res).max(0.0);
    let k = 2.0 * r1 * r1.sqrt() + 0.02;
    let w = f / fc;
    // s = j w ; D = 1 - w^2 + j k w
    let (dr, di) = (1.0 - w * w, k * w);
    let d2 = dr * dr + di * di;
    let lp = 1.0 / d2.sqrt();
    let hp = w * w / d2.sqrt();
    let bp = k * w / d2.sqrt();
    let notch = (1.0 - w * w).abs() / d2.sqrt();
    let mag = match ftype {
        0 => lp,
        1 => lp * lp,
        2 => hp,
        3 => hp * hp,
        4 => bp,
        5 => notch,
        _ => {
            // x + 2 k s / D
            let (nr, ni) = (dr + 2.0 * k * k * w * w / d2 * dr * 0.0, 0.0);
            let _ = (nr, ni);
            let re = 1.0 + (2.0 * k * w * di) / d2;
            let im = (2.0 * k * w * dr) / d2;
            (re * re + im * im).sqrt()
        }
    };
    20.0 * mag.max(1e-5).log10()
}

pub fn filter(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex) {
    let (x0, y0) = (P_FILTER[0], P_FILTER[1]);
    let at = |x: f32, y: f32| cx.at(x0 + x, y0 + y);
    let rr = |x: f32, y: f32, w: f32, h: f32| cx.r(x0 + x, y0 + y, w, h);
    let p = ui.painter().clone();
    let fs = cx.st.filter;
    title(&p, at(16.0, 22.0), "FILTER", at(196.0, 0.0).x);
    if toggle(ui, tex, at(224.0, 22.0), egui::Id::new("filt_on"), fs.on, "Filter on / off").clicked() {
        edit(cx, |st| st.filter.on = !st.filter.on);
    }
    let names: Vec<String> = FILTER_TYPES.iter().map(|s| s.to_string()).collect();
    if let Some(i) = stepper(ui, rr(16.0, 40.0, 220.0, 20.0), "ftype", &names, fs.ftype as usize, &FILTER_TYPES[fs.ftype as usize].to_uppercase(), PHOS_F, "Filter type") {
        edit(cx, |st| st.filter.ftype = i as u8);
    }

    // response screen
    let scr = rr(14.0, 74.0, 224.0, 88.0);
    p.rect_filled(scr, 8.0, SCREEN_BG);
    let ps = p.with_clip_rect(scr);
    let since = (cx.now - g.t_open) as f32 - 0.4;
    let bright = (since / 0.7).clamp(0.0, 1.0) * if fs.on { 1.0 } else { 0.35 };
    let col = PHOS_F;
    glow(&ps, tex, scr.center(), scr.width() * 0.7, col, 0.05 * bright);
    let x_of = |f: f32| scr.left() + (f / 20.0).log10() / 3.0 * scr.width();
    for f in [50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
        let x = x_of(f);
        let major = f == 100.0 || f == 1000.0 || f == 10000.0;
        ps.line_segment([Pos2::new(x, scr.top()), Pos2::new(x, scr.bottom())], Stroke::new(1.0f32, add(col, if major { 0.08 } else { 0.035 } * bright)));
    }
    let (top_db, bot_db) = (24.0, -36.0);
    let y_of = |db: f32| scr.top() + 10.0 + (top_db - db.clamp(bot_db, top_db)) / (top_db - bot_db) * (scr.height() - 16.0);
    ps.line_segment([Pos2::new(scr.left(), y_of(0.0)), Pos2::new(scr.right(), y_of(0.0))], Stroke::new(1.0f32, add(col, 0.10 * bright)));
    let fc = cx.live_plain(F_CUTOFF);
    let res = cx.live_plain(F_RES);
    let mix = cx.live_plain(F_MIX);
    let w = scr.width() as usize;
    let pts: Vec<Pos2> = (0..=w / 2)
        .map(|i| {
            let x = scr.left() + i as f32 * 2.0;
            let f = 20.0 * 10f32.powf((x - scr.left()) / scr.width() * 3.0);
            let h = 10f32.powf(response(fs.ftype, f, fc, res) / 20.0);
            let m = (1.0 - mix) + mix * h;
            Pos2::new(x, y_of(20.0 * m.max(1e-5).log10()))
        })
        .collect();
    ps.add(egui::Shape::mesh(fill_to(&pts, scr.bottom(), add(col, 0.13 * bright), add(col, 0.01 * bright))));
    trace(&ps, pts, col, 1.6, bright);
    let cxp = Pos2::new(x_of(fc.clamp(20.0, 20000.0)), y_of(response(fs.ftype, fc, fc, res).min(top_db)));
    glow(&ps, tex, cxp, 14.0, col, 0.5 * bright);
    ps.circle_filled(cxp, 2.5, add(Color32::WHITE, 0.9 * bright));
    lcd_text(&ps, Pos2::new(scr.left() + 9.0, scr.top() + 10.0), &fmt_unit(fc, Unit::Hz), 8.5, with_a(col, 0.8 * bright), Align2::LEFT_CENTER);
    lcd_text(&ps, Pos2::new(scr.right() - 9.0, scr.top() + 10.0), &format!("Q {:.0}%", res * 100.0), 8.5, with_a(col, 0.8 * bright), Align2::RIGHT_CENTER);
    // drag: x cutoff, y resonance
    let resp = ui.interact(scr, egui::Id::new("fscr"), Sense::drag()).on_hover_text("Drag: left/right cutoff, up/down resonance");
    if resp.dragged() {
        let d = resp.drag_delta();
        cx.set_norm(F_CUTOFF, cx.norm(F_CUTOFF) + d.x / scr.width());
        cx.set_norm(F_RES, cx.norm(F_RES) - d.y / 140.0);
    }
    let (bz, gl) = &tex.crt[2];
    sprite(&p, gl, scr, Color32::WHITE);
    sprite(&p, bz, Rect::from_min_size(scr.min - Vec2::splat(17.0), scr.size() + Vec2::splat(34.0)), Color32::WHITE);

    let tint = Color32::from_rgb(196, 236, 170);
    knob(ui, cx, g, tex, F_CUTOFF, at(52.0, 198.0), 22.0, tint, "CUTOFF");
    knob(ui, cx, g, tex, F_RES, at(128.0, 196.0), 18.0, tint, "RESONANCE");
    knob(ui, cx, g, tex, F_DRIVE, at(202.0, 196.0), 18.0, tint, "DRIVE");
    knob(ui, cx, g, tex, F_KEY, at(102.0, 250.0), 15.0, tint, "KEY TRACK");
    knob(ui, cx, g, tex, F_MIX, at(178.0, 250.0), 15.0, tint, "MIX");
}

pub fn fx(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex) {
    let (x0, y0) = (P_FX[0], P_FX[1]);
    let at = |x: f32, y: f32| cx.at(x0 + x, y0 + y);
    let rr = |x: f32, y: f32, w: f32, h: f32| cx.r(x0 + x, y0 + y, w, h);
    let p = ui.painter().clone();
    let f = cx.st.fx;
    let tint = Color32::from_rgb(232, 214, 236);

    // delay
    title(&p, at(16.0, 22.0), "TAPE DELAY", at(250.0, 0.0).x);
    if toggle(ui, tex, at(280.0, 22.0), egui::Id::new("dly_on"), f.delay_on, "Delay on / off").clicked() {
        edit(cx, |st| st.fx.delay_on = !st.fx.delay_on);
    }
    let col = Color32::from_rgb(255, 150, 200);
    let ls = g.ease(egui::Id::new("dsync"), if f.delay_sync { 1.0 } else { 0.0 }, 14.0, cx.dt);
    if lit_button(ui, tex, rr(16.0, 40.0, 46.0, 20.0), egui::Id::new("dsyncb"), "SYNC", ls, col, "Delay time as a note length").clicked() {
        edit(cx, |st| st.fx.delay_sync = !st.fx.delay_sync);
    }
    let lp = g.ease(egui::Id::new("dping"), if f.pingpong { 1.0 } else { 0.0 }, 14.0, cx.dt);
    if lit_button(ui, tex, rr(66.0, 40.0, 50.0, 20.0), egui::Id::new("dpingb"), "PING", lp, col, "Ping-pong: the repeats bounce left / right").clicked() {
        edit(cx, |st| st.fx.pingpong = !st.fx.pingpong);
    }
    let names: Vec<String> = SYNC_NAMES.iter().map(|s| s.to_string()).collect();
    if let Some(i) = stepper(ui, rr(122.0, 40.0, 84.0, 20.0), "ddiv", &names, f.delay_div as usize, SYNC_NAMES[f.delay_div as usize], if f.delay_sync { col } else { with_a(col, 0.35) }, "Note length (with SYNC on)") {
        edit(cx, |st| {
            st.fx.delay_div = i as u8;
            st.fx.delay_sync = true;
        });
    }
    let ms = cx.shared.delay_ms.load(Ordering::Relaxed);
    lcd(&p, rr(212.0, 40.0, 80.0, 20.0), &format!("{:.0} ms", ms), 10.0, col, Align2::CENTER_CENTER);
    let names_d = [(X_TIME, if f.delay_sync { "TIME (FREE)" } else { "TIME" }), (X_FB, "FEEDBACK"), (X_TONE, "TONE"), (X_DMIX, "MIX")];
    for (i, (d, n)) in names_d.iter().enumerate() {
        knob(ui, cx, g, tex, *d, at(44.0 + i as f32 * 72.0, 100.0), 18.0, tint, n);
    }
    p.line_segment([at(14.0, 148.0), at(292.0, 148.0)], Stroke::new(1.0f32, INK_DIM));

    // reverb
    title(&p, at(16.0, 164.0), "REVERB", at(250.0, 0.0).x);
    if toggle(ui, tex, at(280.0, 164.0), egui::Id::new("rev_on"), f.reverb_on, "Reverb on / off").clicked() {
        edit(cx, |st| st.fx.reverb_on = !st.fx.reverb_on);
    }
    let mut names: Vec<String> = Vec::new();
    let mut idx: Vec<usize> = Vec::new();
    let mut last = "";
    for (i, (cat, n)) in REVERBS.iter().enumerate() {
        if *cat != last {
            names.push(format!("──{cat}"));
            idx.push(usize::MAX);
            last = cat;
        }
        names.push(n.to_string());
        idx.push(i);
    }
    let cur = idx.iter().position(|&i| i == f.reverb_type as usize).unwrap_or(1);
    if let Some(i) = stepper(ui, rr(16.0, 182.0, 276.0, 20.0), "rtype", &names, cur, &REVERBS[f.reverb_type as usize].1.to_uppercase(), Color32::from_rgb(190, 170, 255), "Reverb type") {
        if let Some(&t) = idx.get(i) {
            if t != usize::MAX {
                edit(cx, |st| st.fx.reverb_type = t as u8);
            }
        }
    }
    let rv = [(X_PRE, "PRE-DLY"), (X_SIZE, "SIZE"), (X_DECAY, "DECAY"), (X_RTONE, "TONE"), (X_WIDTH, "WIDTH"), (X_RMIX, "MIX")];
    for (i, (d, n)) in rv.iter().enumerate() {
        knob(ui, cx, g, tex, *d, at(32.0 + i as f32 * 48.5, 244.0), 15.0, tint, n);
    }
}

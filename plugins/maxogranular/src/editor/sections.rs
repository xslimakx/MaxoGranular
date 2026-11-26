//! Filter and effects modules.

use nih_plug_egui::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use nih_plug::prelude::Param;
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

/// The filter screen's analyzer: an FFT of the last samples before and after the filter.
pub struct AnaView {
    fft: std::sync::Arc<dyn realfft::RealToComplex<f32>>,
    win: Vec<f32>,
    buf: Vec<f32>,
    out: Vec<realfft::num_complex::Complex<f32>>,
    /// smoothed level per bin, dB
    pub pre: Vec<f32>,
    pub post: Vec<f32>,
    count: u64,
}

impl AnaView {
    fn new() -> Self {
        let n = crate::ANA_N;
        let fft = realfft::RealFftPlanner::<f32>::new().plan_fft_forward(n);
        let out = fft.make_output_vec();
        let win: Vec<f32> = (0..n).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos()).collect();
        Self { fft, win, buf: vec![0.0; n], out, pre: vec![-120.0; n / 2 + 1], post: vec![-120.0; n / 2 + 1], count: 0 }
    }

    /// new samples in: spectra with a fast rise and a ~1 s fall
    fn update(&mut self, shared: &crate::Shared, dt: f32) {
        let n = crate::ANA_N;
        let mut fresh = false;
        {
            let a = shared.ana.lock();
            if a.count != self.count {
                self.count = a.count;
                fresh = true;
                for (k, which) in [&a.pre, &a.post].into_iter().enumerate() {
                    for i in 0..n {
                        self.buf[i] = which[(a.pos + i) % n] * self.win[i];
                    }
                    let _ = self.fft.process(&mut self.buf, &mut self.out);
                    let dst = if k == 0 { &mut self.pre } else { &mut self.post };
                    // Hann: coherent gain 0.5, so a full-scale sine reads 0 dB
                    let norm = 2.0 / (n as f32 * 0.5);
                    for (j, c) in self.out.iter().enumerate() {
                        let db = 20.0 * (c.norm() * norm + 1e-9).log10();
                        let fall = dst[j] - 26.0 * dt;
                        dst[j] = if db > dst[j] { dst[j] + (db - dst[j]) * 0.6 } else { db.max(fall) };
                    }
                }
            }
        }
        if !fresh {
            // no audio (nothing playing / bypassed host): let it fall
            for v in self.pre.iter_mut().chain(self.post.iter_mut()) {
                *v -= 26.0 * dt;
            }
        }
    }

    /// level at f (Hz): the loudest bin of the pixel's frequency span
    fn at(v: &[f32], f0: f32, f1: f32, sr: f32) -> f32 {
        let n = crate::ANA_N as f32;
        let b0 = (f0 * n / sr).floor().max(1.0) as usize;
        let b1 = ((f1 * n / sr).ceil() as usize).max(b0 + 1).min(v.len() - 1);
        v[b0.min(v.len() - 1)..=b1].iter().fold(-200.0f32, |m, &x| m.max(x))
    }
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
    // the frequency axis has a margin, so the handle at 20 Hz / 20 kHz stays inside the glass
    let inner = Rect::from_min_max(Pos2::new(scr.left() + 10.0, scr.top()), Pos2::new(scr.right() - 10.0, scr.bottom()));
    let x_of = |f: f32| inner.left() + (f.max(1.0) / 20.0).log10() / 3.0 * inner.width();
    let f_of = |x: f32| 20.0 * 10f32.powf(((x - inner.left()) / inner.width()).clamp(0.0, 1.0) * 3.0);
    let (lab_h, top_pad) = (11.0, 18.0);
    for f in [30.0, 40.0, 50.0, 60.0, 80.0, 100.0, 200.0, 300.0, 400.0, 500.0, 600.0, 800.0, 1000.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0, 8000.0, 10000.0] {
        let x = x_of(f);
        let major = f == 100.0 || f == 1000.0 || f == 10000.0;
        let labelled = major || f == 50.0 || f == 500.0 || f == 5000.0;
        let a = if major { 0.09 } else if labelled { 0.05 } else { 0.025 };
        ps.line_segment([Pos2::new(x, scr.top()), Pos2::new(x, scr.bottom() - lab_h)], Stroke::new(1.0f32, add(col, a * bright)));
    }
    // filter curve scale: +18 .. -30 dB
    let (top_db, bot_db) = (18.0, -30.0);
    let y_of = |db: f32| scr.top() + top_pad + (top_db - db.clamp(bot_db, top_db)) / (top_db - bot_db) * (scr.height() - top_pad - lab_h - 2.0);
    for db in [12.0f32, -12.0, -24.0] {
        ps.line_segment([Pos2::new(scr.left(), y_of(db)), Pos2::new(scr.right(), y_of(db))], Stroke::new(1.0f32, add(col, 0.025 * bright)));
    }
    ps.line_segment([Pos2::new(scr.left(), y_of(0.0)), Pos2::new(scr.right(), y_of(0.0))], Stroke::new(1.0f32, add(col, 0.10 * bright)));

    // ---- analyzer: what goes into the filter (dim) and what comes out (bright)
    let ana = g.ana.get_or_insert_with(AnaView::new);
    ana.update(cx.shared, cx.dt);
    let sr = cx.sr.max(8000.0);
    let (sp_top, sp_bot) = (scr.top() + top_pad, scr.bottom() - lab_h);
    let sy = |db: f32| sp_top + ((-6.0 - db) / 78.0).clamp(0.0, 1.0) * (sp_bot - sp_top);
    let cols: Vec<(f32, f32, f32)> = (0..=(inner.width() / 2.0) as usize)
        .map(|i| {
            let x = inner.left() + i as f32 * 2.0;
            let (f0, f1) = (f_of(x - 1.0), f_of(x + 1.0));
            (x, AnaView::at(&ana.pre, f0, f1, sr), AnaView::at(&ana.post, f0, f1, sr))
        })
        .collect();
    let sp_pre: Vec<Pos2> = cols.iter().map(|c| Pos2::new(c.0, sy(c.1))).collect();
    let sp_post: Vec<Pos2> = cols.iter().map(|c| Pos2::new(c.0, sy(c.2))).collect();
    let sp_col = Color32::from_rgb(150, 190, 170);
    if cols.iter().any(|c| c.1 > -84.0) {
        ps.add(egui::Shape::mesh(fill_to(&sp_pre, sp_bot, add(sp_col, 0.05 * bright), add(sp_col, 0.01 * bright))));
        ps.add(egui::Shape::line(sp_pre, Stroke::new(1.0f32, add(sp_col, 0.10 * bright))));
        ps.add(egui::Shape::mesh(fill_to(&sp_post, sp_bot, add(sp_col, 0.16 * bright), add(sp_col, 0.02 * bright))));
        ps.add(egui::Shape::line(sp_post, Stroke::new(1.2f32, add(sp_col, 0.40 * bright))));
    }

    // ---- the filter's response
    let fc = cx.live_plain(F_CUTOFF);
    let res = cx.live_plain(F_RES);
    let mix = cx.live_plain(F_MIX);
    let pts: Vec<Pos2> = (0..=(scr.width() / 2.0) as usize)
        .map(|i| {
            let x = scr.left() + i as f32 * 2.0;
            let f = 20.0 * 10f32.powf((x - inner.left()) / inner.width() * 3.0);
            let h = 10f32.powf(response(fs.ftype, f, fc, res) / 20.0);
            let m = (1.0 - mix) + mix * h;
            Pos2::new(x, y_of(20.0 * m.max(1e-5).log10()))
        })
        .collect();
    trace(&ps, pts, col, 1.6, bright);

    // labels: frequencies under the graph, dB of the curve on the right
    for (f, t) in [(50.0, "50"), (100.0, "100"), (200.0, "200"), (500.0, "500"), (1000.0, "1k"), (2000.0, "2k"), (5000.0, "5k"), (10000.0, "10k")] {
        lcd_text(&ps, Pos2::new(x_of(f), scr.bottom() - 5.5), t, 7.0, with_a(col, 0.55 * bright), Align2::CENTER_CENTER);
    }
    for (db, t) in [(12.0f32, "+12"), (0.0, "0"), (-12.0, "-12"), (-24.0, "-24")] {
        lcd_text(&ps, Pos2::new(scr.left() + 3.0, y_of(db) - 4.5), t, 6.5, with_a(col, 0.40 * bright), Align2::LEFT_CENTER);
    }

    // ---- the handle: cutoff (x) and resonance; always inside the glass
    let base_n = (cx.p.knob(F_CUTOFF).unmodulated_normalized_value(), cx.p.knob(F_RES).unmodulated_normalized_value());
    let hx = x_of(fc.clamp(20.0, 20000.0));
    let hy = y_of(response(fs.ftype, fc, fc, res).min(top_db)).clamp(scr.top() + top_pad - 4.0, scr.bottom() - lab_h - 4.0);
    let hp = Pos2::new(hx.clamp(scr.left() + 6.0, scr.right() - 6.0), hy);
    let resp = ui.interact(scr, egui::Id::new("fscr"), Sense::click_and_drag()).on_hover_text("Drag anywhere: the cutoff follows the mouse left / right, up / down sets the resonance. Double-click: reset both.");
    let hot = resp.hovered() || resp.dragged();
    glow(&ps, tex, hp, if hot { 18.0 } else { 14.0 }, col, (if hot { 0.7 } else { 0.5 }) * bright);
    ps.circle_filled(hp, if hot { 3.5 } else { 2.5 }, add(Color32::WHITE, 0.9 * bright));
    if hot {
        ps.circle_stroke(hp, 6.5, Stroke::new(1.0f32, with_a(col, 0.8 * bright)));
    }
    lcd_text(&ps, Pos2::new(scr.left() + 9.0, scr.top() + 9.0), &fmt_unit(fc, Unit::Hz), 8.5, with_a(col, 0.8 * bright), Align2::LEFT_CENTER);
    lcd_text(&ps, Pos2::new(scr.right() - 9.0, scr.top() + 9.0), &format!("Q {:.0}%", res * 100.0), 8.5, with_a(col, 0.8 * bright), Align2::RIGHT_CENTER);
    // absolute: the press position and the values then; the host may answer the parameter
    // changes late, so nothing is read back from the parameters while dragging
    if resp.drag_started() {
        let o = ui.input(|i| i.pointer.press_origin()).unwrap_or(hp);
        cx.setter.begin_set_parameter(cx.p.knob(F_CUTOFF));
        cx.setter.begin_set_parameter(cx.p.knob(F_RES));
        g.fdrag = Some((base_n.0, base_n.1, o));
    }
    if let (Some((c0, r0, o)), Some(pp)) = (g.fdrag, resp.interact_pointer_pos()) {
        if resp.dragged() {
            let fine = ui.input(|i| i.modifiers.shift);
            let k = if fine { 0.2 } else { 1.0 };
            let cn = (c0 + (pp.x - o.x) / inner.width() * k).clamp(0.0, 1.0);
            let rn = (r0 - (pp.y - o.y) / 90.0 * k).clamp(0.0, 1.0);
            cx.setter.set_parameter_normalized(cx.p.knob(F_CUTOFF), cn);
            cx.setter.set_parameter_normalized(cx.p.knob(F_RES), rn);
        }
    }
    if resp.drag_stopped() {
        cx.setter.end_set_parameter(cx.p.knob(F_CUTOFF));
        cx.setter.end_set_parameter(cx.p.knob(F_RES));
        g.fdrag = None;
    }
    if resp.double_clicked() {
        cx.set_norm(F_CUTOFF, cx.p.knob(F_CUTOFF).default_normalized_value());
        cx.set_norm(F_RES, cx.p.knob(F_RES).default_normalized_value());
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

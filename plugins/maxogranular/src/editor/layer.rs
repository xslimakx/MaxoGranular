//! A grain generator module: the CRT with the source and its living grains, the tape reels,
//! the switches and the knobs.

use nih_plug::prelude::Param;
use nih_plug_egui::egui::{self, epaint::PathStroke, Align2, Color32, Mesh, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use std::f32::consts::PI;
use std::sync::atomic::Ordering;

use super::hw::*;
use super::knobs::knob;
use super::tex::Tex;
use super::{edit, open_folder, set_source, Cx, Gui, P_A, P_B};
use crate::kinds::*;
use crate::sources::{self, SourceRef, BUILTIN};
use crate::GRAIN_RING;

const CRT_X: f32 = 24.0;
const CRT_Y: f32 = 44.0;
const CRT_W: f32 = 352.0;
const CRT_H: f32 = 150.0;

pub fn layer_col(l: usize) -> Color32 {
    if l == 0 {
        PHOS_A
    } else {
        PHOS_B
    }
}
pub fn cap_tint(l: usize) -> Color32 {
    if l == 0 {
        Color32::from_rgb(255, 205, 150)
    } else {
        Color32::from_rgb(178, 236, 214)
    }
}

fn note_name(n: u8) -> String {
    const N: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", N[n as usize % 12], n as i32 / 12 - 2)
}

/// Where the grains are taken from right now (0..1): position + scan, as the newest voice plays.
fn playhead(cx: &Cx, l: usize) -> f32 {
    let pos = cx.live[layer_d(l, L_POS)];
    let scan = cx.shared.scan[l].load(Ordering::Relaxed);
    (pos + if cx.voices > 0 { scan } else { 0.0 }).rem_euclid(1.0)
}

pub fn draw(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex, l: usize) {
    let pl = if l == 0 { P_A } else { P_B };
    let (x0, y0) = (pl[0], pl[1]);
    let at = |x: f32, y: f32| cx.at(x0 + x, y0 + y);
    let rr = |x: f32, y: f32, w: f32, h: f32| cx.r(x0 + x, y0 + y, w, h);
    let set = cx.st.layers[l].clone();
    let col = layer_col(l);
    let p = ui.painter().clone();

    // ---- top row: title, source, mode, lamp, power
    title(&p, at(22.0, 22.0), if l == 0 { "GRAIN A" } else { "GRAIN B" }, x0 + 0.0);
    let on = g.ease(egui::Id::new(("onlit", l)), if set.on { 1.0 } else { 0.0 }, 14.0, cx.dt);
    if lit_button(ui, tex, rr(84.0, 11.0, 40.0, 22.0), egui::Id::new(("onb", l)), if set.on { "ON" } else { "OFF" }, on, Color32::from_rgb(120, 255, 140), "Layer on / off").clicked() {
        edit(cx, |st| st.layers[l].on = !st.layers[l].on);
    }
    if lit_button(ui, tex, rr(130.0, 11.0, 58.0, 22.0), egui::Id::new(("loadb", l)), "LOAD", 0.0, col, "Load your own sound: WAV, AIFF, FLAC, MP3, OGG, M4A\n(or drag a file onto the screen)").clicked() {
        super::open_dialog(g, l);
    }
    source_selector(ui, cx, g, l, rr(194.0, 11.0, 172.0, 22.0));
    let lit_c = g.ease(egui::Id::new(("modelit", l, 0)), if set.mode == 0 { 1.0 } else { 0.0 }, 12.0, cx.dt);
    let lit_s = g.ease(egui::Id::new(("modelit", l, 1)), if set.mode == 1 { 1.0 } else { 0.0 }, 12.0, cx.dt);
    if lit_button(ui, tex, rr(372.0, 11.0, 58.0, 22.0), egui::Id::new(("cloud", l)), "CLOUD", lit_c, col, "Cloud: grains at random times (Density per second) - textures, pads, clouds.").clicked() {
        edit(cx, |st| st.layers[l].mode = 0);
    }
    if lit_button(ui, tex, rr(434.0, 11.0, 58.0, 22.0), egui::Id::new(("sync", l)), "SYNC", lit_s, col, "Sync: grains locked to the note's pitch - playable tones; Pitch moves the formant.").clicked() {
        edit(cx, |st| st.layers[l].mode = 1);
    }
    // activity lamp
    let mut active = 0usize;
    for gv in cx.shared.grains.iter() {
        if gv.flags.load(Ordering::Relaxed) & 1 == l as u32 {
            let life = gv.life.load(Ordering::Relaxed) as i64;
            let age = cx.clock as i64 - gv.birth.load(Ordering::Relaxed) as i64;
            if life > 0 && age >= 0 && age < life {
                active += 1;
            }
        }
    }
    let lamp = g.ease(egui::Id::new(("lamp", l)), if set.on { (active as f32 / 12.0).min(1.0) } else { 0.0 }, 20.0, cx.dt);
    jewel(&p, tex, at(548.0, 22.0), 18.0, if l == 0 { 0 } else { 1 }, lamp);

    // ---- the screen
    screen(ui, cx, g, tex, l, rr(CRT_X, CRT_Y, CRT_W, CRT_H), active);

    // ---- reels
    reels(ui, cx, g, tex, l, at(0.0, 0.0), active);

    // ---- switches under the reels
    {
        let names: Vec<String> = WINDOWS.iter().map(|s| s.to_string()).collect();
        if let Some(i) = stepper(ui, rr(392.0, 158.0, 112.0, 18.0), &format!("win{l}"), &names, set.window as usize, WINDOWS[set.window as usize], col, "Grain window: the shape of every grain.\nHann / Gauss: smooth · Triangle · Tukey: fuller · Decay: percussive · Swell: reversed attack") {
            edit(cx, |st| st.layers[l].window = i as u8);
        }
        let names: Vec<String> = (1..=8).map(|i| format!("{i} stream{}", if i == 1 { "" } else { "s" })).collect();
        if let Some(i) = stepper(ui, rr(508.0, 158.0, 66.0, 18.0), &format!("str{l}"), &names, set.streams as usize - 1, &format!("×{}", set.streams), col, "Streams: parallel grain clouds, spread by Detune (unison).") {
            edit(cx, |st| st.layers[l].streams = i as u8 + 1);
        }
        let kt = g.ease(egui::Id::new(("kt", l)), if set.keytrack { 1.0 } else { 0.0 }, 12.0, cx.dt);
        if lit_button(ui, tex, rr(392.0, 181.0, 52.0, 18.0), egui::Id::new(("ktb", l)), "KEYS", kt, col, "The grains follow the keys (off: every key plays the same pitch).").clicked() {
            edit(cx, |st| st.layers[l].keytrack = !st.layers[l].keytrack);
        }
        let ft = g.ease(egui::Id::new(("ft", l)), if set.filter { 1.0 } else { 0.0 }, 12.0, cx.dt);
        if lit_button(ui, tex, rr(448.0, 181.0, 56.0, 18.0), egui::Id::new(("ftb", l)), "FILTER", ft, col, "Send this layer through the filter.").clicked() {
            edit(cx, |st| st.layers[l].filter = !st.layers[l].filter);
        }
        let names: Vec<String> = (24..=96u8).map(note_name).collect();
        if let Some(i) = stepper(ui, rr(508.0, 181.0, 66.0, 18.0), &format!("root{l}"), &names, (set.root.clamp(24, 96) - 24) as usize, &note_name(set.root), col, "Root: the key that plays the source at its own pitch.") {
            edit(cx, |st| st.layers[l].root = i as u8 + 24);
        }
    }

    // ---- knobs
    let tint = cap_tint(l);
    let r = 18.0;
    let row1 = [(L_POS, "POSITION"), (L_SCAN, "SCAN"), (L_SPRAY, "SPRAY"), (L_SIZE, "SIZE"), (L_DENSITY, if set.mode == 0 { "DENSITY" } else { "OVERLAP" }), (L_JITTER, "JITTER"), (L_PITCH, if set.mode == 0 { "PITCH" } else { "FORMANT" }), (L_FINE, "FINE")];
    for (i, (k, name)) in row1.iter().enumerate() {
        knob(ui, cx, g, tex, layer_d(l, *k), at(52.0 + i as f32 * 69.5, 232.0), r, tint, name);
    }
    let row2: [(usize, &str, usize); 7] = [(L_PRAND, "PITCH RND", 0), (L_SKEW, "SKEW", 1), (L_REVERSE, "REVERSE", 2), (L_STEREO, "STEREO", 3), (L_DETUNE, "DETUNE", 4), (L_PAN, "PAN", 6), (L_LEVEL, "LEVEL", 7)];
    for (k, name, slot) in row2 {
        let rk = if k == L_LEVEL { 20.0 } else { r };
        knob(ui, cx, g, tex, layer_d(l, k), at(52.0 + slot as f32 * 69.5, 289.0), rk, tint, name);
    }
    // a little window-shape print in the gap
    {
        let c = at(52.0 + 5.0 * 69.5, 289.0);
        let w = 34.0;
        let h = 18.0;
        let tab = crate::dsp::make_windows();
        let wi = set.window as usize;
        let skew = cx.live[layer_d(l, L_SKEW)] * 2.0 - 1.0;
        let pk = 0.5 + 0.45 * skew;
        let pts: Vec<Pos2> = (0..=40)
            .map(|i| {
                let t = i as f32 / 40.0;
                let u = if t < pk { t / pk * 0.5 } else { 0.5 + (t - pk) / (1.0 - pk) * 0.5 };
                let v = tab[wi][((u * (crate::dsp::WIN_N - 1) as f32) as usize).min(crate::dsp::WIN_N - 1)];
                Pos2::new(c.x - w * 0.5 + t * w, c.y + h * 0.5 - v * h)
            })
            .collect();
        p.add(Shape::line(pts, PathStroke::new(1.3f32, INK)));
        p.line_segment([Pos2::new(c.x - w * 0.5 - 2.0, c.y + h * 0.5), Pos2::new(c.x + w * 0.5 + 2.0, c.y + h * 0.5)], Stroke::new(1.0f32, INK_DIM));
        ink(&p, Pos2::new(c.x, c.y + 30.0), "WINDOW", 8.0, Align2::CENTER_CENTER);
    }
}

#[derive(Clone, PartialEq)]
enum Act {
    Head,
    Load,
    Path,
    Folder,
    Rescan,
    Src(SourceRef),
}

fn source_selector(ui: &mut Ui, cx: &Cx, g: &mut Gui, l: usize, r: Rect) {
    // list: loading your own sound first, then your samples folder, then the built-in sounds
    let mut names: Vec<String> = Vec::new();
    let mut acts: Vec<Act> = Vec::new();
    let mut add = |n: String, a: Act| {
        names.push(n);
        acts.push(a);
    };
    add("──Your sound".into(), Act::Head);
    add("📂  Load a sound file…".into(), Act::Load);
    add("✎  Load by path…".into(), Act::Path);
    add("📁  Open the samples folder".into(), Act::Folder);
    add("⟳  Rescan the samples folder".into(), Act::Rescan);
    add(format!("──Samples folder ({})", g.files.len()), Act::Head);
    for f in &g.files {
        add(f.clone(), Act::Src(SourceRef::File(f.clone())));
    }
    let mut last = "";
    for (i, (cat, name)) in BUILTIN.iter().enumerate() {
        if *cat != last {
            add(format!("──Built-in: {cat}"), Act::Head);
            last = cat;
        }
        add(name.to_string(), Act::Src(SourceRef::Builtin(i as u16)));
    }
    let cur_ref = cx.st.layers[l].source.clone();
    // a file loaded from elsewhere is not in the list: show its name anyway
    let cur = acts.iter().position(|a| *a == Act::Src(cur_ref.clone()));
    let shown = sources::display_name(&cur_ref).to_uppercase();
    let col = layer_col(l);
    let cur_i = cur.unwrap_or(1);
    if let Some(mut i) = stepper(ui, r, &format!("src{l}"), &names, cur_i, &shown, col, "The sound the grains are cut from.\nLOAD (or drag a WAV / AIFF / FLAC / MP3 / OGG file onto the screen) for your own sound.") {
        // the arrows step through the sounds only
        let n = acts.len();
        let step = if i == (cur_i + 1) % n { Some(1) } else if i == (cur_i + n - 1) % n { Some(n - 1) } else { None };
        if let Some(st) = step {
            let mut k = i;
            for _ in 0..n {
                if matches!(acts[k], Act::Src(_)) {
                    break;
                }
                k = (k + st) % n;
            }
            i = k;
        }
        match acts[i].clone() {
            Act::Src(s) => set_source(cx, l, s),
            Act::Load => super::open_dialog(g, l),
            Act::Path => g.path_entry = Some((l, String::new())),
            Act::Folder => {
                if let Some(d) = sources::samples_dir() {
                    open_folder(&d);
                }
            }
            Act::Rescan => {
                g.files = sources::list_files();
                g.say(format!("{} samples found", g.files.len()), cx.now);
            }
            Act::Head => {}
        }
    }
    if let Some((pl, buf)) = g.path_entry.clone() {
        if pl == l {
            let mut b = buf;
            let er = Rect::from_min_size(r.left_bottom() + Vec2::new(0.0, 4.0), Vec2::new(r.width() + 140.0, 22.0));
            egui::Area::new(egui::Id::new(("patharea", l))).fixed_pos(er.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let te = ui.add(egui::TextEdit::singleline(&mut b).desired_width(er.width() - 90.0).hint_text("/path/to/sound.wav (.aif .flac .mp3 .ogg)"));
                        te.request_focus();
                        let go = ui.button("Load").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let esc = ui.button("✕").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
                        if go {
                            let path = b.trim().trim_matches('"').to_string();
                            if sources::load_audio(std::path::Path::new(&path)).is_some() {
                                set_source(cx, l, SourceRef::File(path));
                                edit(cx, |st| st.layers[l].on = true);
                                g.path_entry = None;
                            } else {
                                g.say("Could not read that file", cx.now);
                                g.path_entry = Some((l, b.clone()));
                            }
                        } else if esc {
                            g.path_entry = None;
                        } else {
                            g.path_entry = Some((l, b.clone()));
                        }
                    });
                });
            });
        }
    }
}

fn screen(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex, l: usize, r: Rect, active: usize) {
    let set = &cx.st.layers[l];
    let col = layer_col(l);
    let bez = Rect::from_min_size(r.min - Vec2::splat(17.0), r.size() + Vec2::splat(34.0));
    let p0 = ui.painter().clone();
    p0.rect_filled(r, 8.0, SCREEN_BG);
    let p = p0.with_clip_rect(r);
    // power-on: brightness ramps up with a flicker
    let since = (cx.now - g.t_open) as f32 - 0.12 * l as f32;
    let on_k = (since / 0.7).clamp(0.0, 1.0);
    let flick = if on_k < 1.0 { 0.75 + 0.25 * ((since * 47.0).sin() * (since * 13.0).cos()) } else { 1.0 };
    let bright = on_k * flick * if set.on { 1.0 } else { 0.35 };
    // phosphor haze
    glow(&p, tex, r.center(), r.width() * 0.7, col, 0.05 * bright);
    // graticule
    for i in 1..10 {
        let x = r.left() + r.width() * i as f32 / 10.0;
        p.line_segment([Pos2::new(x, r.top()), Pos2::new(x, r.bottom())], Stroke::new(1.0f32, add(col, 0.045 * bright)));
    }
    for i in 1..4 {
        let y = r.top() + r.height() * i as f32 / 4.0;
        p.line_segment([Pos2::new(r.left(), y), Pos2::new(r.right(), y)], Stroke::new(1.0f32, add(col, if i == 2 { 0.07 } else { 0.045 } * bright)));
    }

    let mid = r.center().y + 6.0;
    let amp_h = r.height() * 0.36;
    // the source waveform
    if let Some(src) = cx.shared.source(l) {
        let ov = &src.overview;
        let n = ov.len().max(1);
        let w = r.width() as usize;
        let mut top = Vec::with_capacity(w + 1);
        let mut bot = Vec::with_capacity(w + 1);
        for x in 0..=w {
            let a = (x * n / (w + 1)).min(n - 1);
            let b = (((x + 1) * n / (w + 1)).max(a + 1)).min(n);
            let (mut lo, mut hi) = (0.0f32, 0.0f32);
            for c in &ov[a..b] {
                lo = lo.min(c.0);
                hi = hi.max(c.1);
            }
            let xx = r.left() + x as f32;
            top.push(Pos2::new(xx, mid - hi * amp_h));
            bot.push(Pos2::new(xx, mid - lo * amp_h));
        }
        let mut m = Mesh::default();
        let ca = add(col, 0.16 * bright);
        let cm = add(col, 0.05 * bright);
        for i in 0..top.len() {
            m.colored_vertex(top[i], ca);
            m.colored_vertex(Pos2::new(top[i].x, mid), cm);
            m.colored_vertex(bot[i], ca);
            if i > 0 {
                let k = (i * 3) as u32;
                m.add_triangle(k - 3, k - 2, k);
                m.add_triangle(k - 2, k + 1, k);
                m.add_triangle(k - 2, k - 1, k + 1);
                m.add_triangle(k - 1, k + 2, k + 1);
            }
        }
        p.add(Shape::mesh(m));
        p.add(Shape::line(top, PathStroke::new(1.0f32, add(col, 0.35 * bright))));
        p.add(Shape::line(bot, PathStroke::new(1.0f32, add(col, 0.35 * bright))));
        // name, length
        lcd_text(&p, Pos2::new(r.left() + 10.0, r.top() + 10.0), &format!("{}  {:.1}s", src.name.to_uppercase(), src.seconds()), 9.0, with_a(col, 0.85 * bright), Align2::LEFT_CENTER);
    }
    let x_of = |f: f32| r.left() + f.rem_euclid(1.0) * r.width();

    // spray band around the position
    let pos = cx.live[layer_d(l, L_POS)];
    let spray = cx.live_plain(layer_d(l, L_SPRAY));
    let ph = playhead(cx, l);
    if spray > 0.002 {
        let half = (spray * 0.5).min(0.5);
        for (a, b) in [(ph - half, ph + half)] {
            let segs = if a < 0.0 { vec![(a + 1.0, 1.0), (0.0, b)] } else if b > 1.0 { vec![(a, 1.0), (0.0, b - 1.0)] } else { vec![(a, b)] };
            for (s0, s1) in segs {
                let rect = Rect::from_min_max(Pos2::new(r.left() + s0 * r.width(), r.top()), Pos2::new(r.left() + s1 * r.width(), r.bottom()));
                p.rect_filled(rect, 0.0, add(col, 0.06 * bright));
            }
        }
    }
    // playhead
    let phx = x_of(ph);
    p.line_segment([Pos2::new(phx, r.top()), Pos2::new(phx, r.bottom())], Stroke::new(5.0f32, add(col, 0.10 * bright)));
    p.line_segment([Pos2::new(phx, r.top()), Pos2::new(phx, r.bottom())], Stroke::new(1.2f32, add(col, 0.75 * bright)));
    // the knob's position (where a note starts)
    let bx = x_of(cx.norm(layer_d(l, L_POS)));
    p.add(Shape::convex_polygon(vec![Pos2::new(bx - 5.0, r.bottom()), Pos2::new(bx + 5.0, r.bottom()), Pos2::new(bx, r.bottom() - 7.0)], add(col, 0.8 * bright), Stroke::NONE));
    let _ = pos;

    // grains
    if set.on {
        let clock = cx.clock as i64;
        let sr = cx.sr;
        let w_head = cx.shared.grain_w.load(Ordering::Relaxed) as usize;
        let thin = (1.0 / ((active as f32 / 45.0).max(1.0)).sqrt()).max(0.3);
        for k in 0..GRAIN_RING {
            let gv = &cx.shared.grains[(w_head + GRAIN_RING - 1 - k) % GRAIN_RING];
            let fl = gv.flags.load(Ordering::Relaxed);
            if fl & 1 != l as u32 {
                continue;
            }
            let life = gv.life.load(Ordering::Acquire) as f32;
            let age = (clock - gv.birth.load(Ordering::Relaxed) as i64) as f32;
            if life <= 0.0 || age < 0.0 {
                continue;
            }
            let t = age / life;
            let tail = (age - life) / sr / 0.45;
            if t >= 1.0 && tail >= 1.0 {
                continue;
            }
            let p0 = gv.pos.load(Ordering::Relaxed);
            let len = gv.len.load(Ordering::Relaxed);
            let dir = if fl & 2 != 0 { -1.0 } else { 1.0 };
            let semis = gv.pitch.load(Ordering::Relaxed);
            let amp = gv.amp.load(Ordering::Relaxed).clamp(0.05, 1.0);
            let y = (mid - (semis / 30.0).clamp(-1.0, 1.0) * r.height() * 0.40).clamp(r.top() + 6.0, r.bottom() - 6.0);
            let head = p0 + dir * len * t.min(1.0);
            let hx = x_of(head);
            let sx = x_of(p0);
            let straight = (head - p0).abs() < 0.5 && (head.floor() == p0.floor());
            if t < 1.0 {
                let env = (PI * t).sin();
                let a = (0.25 + 0.75 * env) * thin * bright;
                if straight && (hx - sx).abs() > 1.0 {
                    p.line_segment([Pos2::new(sx, y), Pos2::new(hx, y)], Stroke::new(1.4f32, add(col, 0.35 * a)));
                }
                glow(&p, tex, Pos2::new(hx, y), 6.0 + 10.0 * amp * env, col, 0.55 * a);
                p.circle_filled(Pos2::new(hx, y), 1.0 + 1.4 * env, add(lerp_col(col, Color32::WHITE, 0.5), a));
            } else {
                let a = (1.0 - tail).powi(2) * 0.4 * thin * bright;
                if straight && (hx - sx).abs() > 1.0 {
                    p.line_segment([Pos2::new(sx, y), Pos2::new(hx, y)], Stroke::new(1.2f32, add(col, a)));
                } else {
                    p.circle_filled(Pos2::new(hx, y), 1.5, add(col, a));
                }
            }
        }
    } else {
        lcd_text(&p, r.center() - Vec2::new(0.0, 8.0), "LAYER OFF", 14.0, with_a(col, 0.55 * on_k), Align2::CENTER_CENTER);
        lcd_text(&p, r.center() + Vec2::new(0.0, 12.0), "PRESS  ON  TO HEAR IT", 9.0, with_a(col, 0.4 * on_k), Align2::CENTER_CENTER);
    }
    // mode / window print in the corner
    let info = format!("{} · {}{}", MODES[set.mode as usize].to_uppercase(), WINDOWS[set.window as usize].to_uppercase(), if set.streams > 1 { format!(" · ×{}", set.streams) } else { String::new() });
    lcd_text(&p, Pos2::new(r.right() - 10.0, r.top() + 10.0), &info, 9.0, with_a(col, 0.7 * bright), Align2::RIGHT_CENTER);
    if active > 0 {
        lcd_text(&p, Pos2::new(r.right() - 10.0, r.bottom() - 10.0), &format!("{active} GRAINS"), 8.5, with_a(col, 0.55 * bright), Align2::RIGHT_CENTER);
    }
    // power-on line
    if on_k < 0.35 && on_k > 0.0 {
        let k = on_k / 0.35;
        let hh = r.height() * 0.5 * k;
        p.rect_filled(Rect::from_center_size(r.center(), Vec2::new(r.width(), hh.max(1.5))), 0.0, add(Color32::WHITE, 0.25 * (1.0 - k)));
    }

    // interaction: drag = position, wheel = spray
    let id = egui::Id::new(("crt", l));
    let resp = ui.interact(r, id, Sense::click_and_drag()).on_hover_text("Drag: position in the sound · Wheel: spray");
    let dp = layer_d(l, L_POS);
    if resp.drag_started() {
        cx.setter.begin_set_parameter(cx.p.knob(dp));
    }
    if resp.dragged() || resp.clicked() {
        if let Some(q) = resp.interact_pointer_pos() {
            let v = ((q.x - r.left()) / r.width()).clamp(0.0, 1.0);
            if resp.clicked() {
                cx.set_norm(dp, v);
            } else {
                cx.setter.set_parameter_normalized(cx.p.knob(dp), v);
            }
        }
    }
    if resp.drag_stopped() {
        cx.setter.end_set_parameter(cx.p.knob(dp));
    }
    if resp.hovered() {
        let sc = ui.input(|i| i.smooth_scroll_delta.y);
        if sc != 0.0 {
            let ds = layer_d(l, L_SPRAY);
            cx.set_norm(ds, cx.norm(ds) + sc / 1200.0);
        }
        if let Some(q) = resp.hover_pos() {
            p.line_segment([Pos2::new(q.x, r.top()), Pos2::new(q.x, r.bottom())], Stroke::new(1.0f32, add(Color32::WHITE, 0.12)));
        }
    }

    // glass and bezel on top
    let (bz, gl) = &tex.crt[0];
    sprite(&p0, gl, r, Color32::WHITE);
    sprite(&p0, bz, bez, Color32::WHITE);
    let _ = cx.p.knob(dp).name();
}

fn reels(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex, l: usize, o: Pos2, active: usize) {
    let p = ui.painter().clone();
    let set = &cx.st.layers[l];
    let target = playhead(cx, l);
    // the tape follows the playhead; a jump rewinds / fast-forwards the reels
    let prev = g.reel_pos[l];
    let mut d = target - prev;
    if d.abs() > 0.5 {
        d -= d.signum();
    }
    let k = 1.0 - (-9.0 * cx.dt).exp();
    let step = d * k;
    g.reel_pos[l] = (prev + step).rem_euclid(1.0);
    let cur = g.reel_pos[l];
    let (rmin, rmax) = (15.0f32, 39.0f32);
    let rad = |amount: f32| (rmin * rmin + (rmax * rmax - rmin * rmin) * amount.clamp(0.0, 1.0)).sqrt();
    let rl = rad(1.0 - cur);
    let rr = rad(cur);
    let tape_len = 900.0;
    g.reel_ang[l][0] += step * tape_len / rl;
    g.reel_ang[l][1] += step * tape_len / rr;
    // searching: the reels rock a little while grains spray around a frozen position
    let rock = if cx.voices > 0 && set.on { 0.04 * cx.live[layer_d(l, L_SPRAY)].sqrt() * ((cx.now * 9.0 + l as f64) as f32).sin() } else { 0.0 };
    let cl = o + Vec2::new(436.0, 90.0);
    let cr = o + Vec2::new(532.0, 90.0);
    let head = o + Vec2::new(484.0, 141.0);
    // tape path: pack -> guide -> head -> guide -> pack
    let gl = o + Vec2::new(452.0, 141.0);
    let gr = o + Vec2::new(516.0, 141.0);
    let tape_c = Color32::from_rgb(58, 36, 22);
    p.line_segment([cl + Vec2::new(-rl * 0.15, rl * 0.99), gl + Vec2::new(-3.0, 3.0)], Stroke::new(2.4f32, tape_c));
    p.line_segment([gl + Vec2::new(0.0, 3.5), gr + Vec2::new(0.0, 3.5)], Stroke::new(2.4f32, tape_c));
    p.line_segment([gr + Vec2::new(3.0, 3.0), cr + Vec2::new(rr * 0.15, rr * 0.99)], Stroke::new(2.4f32, tape_c));
    for (c, rp, ang) in [(cl, rl, g.reel_ang[l][0] + rock), (cr, rr, g.reel_ang[l][1] + rock)] {
        // shadow of the reel on the plate
        p.circle_filled(c + Vec2::new(3.0, 5.0), 43.0, Color32::from_black_alpha(70));
        sprite_rot(&p, &tex.tape, c, Vec2::splat(rp * 2.0 / 0.98), ang * 0.3, Color32::WHITE);
        sprite_rot(&p, &tex.reel, c, Vec2::splat(43.0 * 2.0 / 0.96), ang, Color32::WHITE);
        // spindle nut
        p.circle_filled(c, 4.5, Color32::from_gray(40));
        p.circle_filled(c + Vec2::new(-1.0, -1.0), 2.4, Color32::from_gray(150));
    }
    // guide rollers and the head
    for c in [gl, gr] {
        p.circle_filled(c + Vec2::new(1.0, 2.0), 5.5, Color32::from_black_alpha(90));
        p.circle_filled(c, 5.0, Color32::from_gray(120));
        p.circle_filled(c + Vec2::new(-1.2, -1.2), 2.5, Color32::from_gray(200));
        p.circle_filled(c, 1.5, Color32::from_gray(50));
    }
    let hr = Rect::from_center_size(head + Vec2::new(0.0, 1.0), Vec2::new(22.0, 12.0));
    p.rect_filled(hr.translate(Vec2::new(1.5, 2.5)), 2.0, Color32::from_black_alpha(100));
    vgrad(&p, hr, Color32::from_gray(190), Color32::from_gray(90));
    p.rect_stroke(hr, 2.0, Stroke::new(1.0f32, Color32::from_black_alpha(160)), egui::StrokeKind::Inside);
    p.line_segment([Pos2::new(head.x, hr.top() + 1.0), Pos2::new(head.x, hr.bottom() - 1.0)], Stroke::new(1.0f32, Color32::from_gray(30)));
    let lit = g.ease(egui::Id::new(("headlit", l)), if active > 0 && set.on { 1.0 } else { 0.0 }, 10.0, cx.dt);
    glow(&p, tex, head, 18.0, layer_col(l), 0.35 * lit);
}

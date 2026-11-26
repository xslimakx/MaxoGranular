//! The editor: a rack of worn hammertone module plates with real-looking knobs, reels, CRT
//! screens, lamps and patch cables.

mod header;
mod hw;
mod knobs;
mod layer;
mod modsec;
mod perf;
mod sections;
mod tex;

use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};
use nih_plug_egui::{create_egui_editor, EguiState};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use crate::kinds::{N_DEST, N_SRC};
use crate::presets::{self, PresetData};
use crate::sources::{self, SourceRef};
use crate::{GrainParams, Shared, State};
use hw::*;
use tex::Tex;

pub const BASE_W: f32 = 1240.0;
pub const BASE_H: f32 = 820.0;

/// module plates (x, y, w, h) - keep in sync with tools/render_textures.py
pub const P_HEADER: [f32; 4] = [24.0, 6.0, 1192.0, 54.0];
pub const P_A: [f32; 4] = [24.0, 64.0, 590.0, 332.0];
pub const P_B: [f32; 4] = [626.0, 64.0, 590.0, 332.0];
pub const P_MOD: [f32; 4] = [24.0, 400.0, 626.0, 292.0];
pub const P_FILTER: [f32; 4] = [654.0, 400.0, 252.0, 292.0];
pub const P_FX: [f32; 4] = [910.0, 400.0, 306.0, 292.0];
pub const P_PERF: [f32; 4] = [24.0, 696.0, 398.0, 118.0];
pub const P_KEYS: [f32; 4] = [426.0, 696.0, 790.0, 118.0];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Presets,
    Matrix,
}

pub struct Cable {
    pub src: usize,
    pub from: Pos2,
    /// the plug end (follows the pointer with a little lag)
    pub end: Pos2,
    pub vel: Vec2,
}

pub struct Gui {
    pub tex: Option<Tex>,
    pub ease: HashMap<egui::Id, f32>,
    pub t_open: f64,
    pub last: f64,
    /// 0..2 envelopes, 3..6 LFOs
    pub tab: usize,
    pub cable: Option<Cable>,
    /// a dropped cable springing back: (src, from, end, start time)
    pub cable_back: Option<(usize, Pos2, Pos2, f64)>,
    /// knob rectangles (drop targets) of the last frame
    pub targets: Vec<(Rect, usize)>,
    pub targets_next: Vec<(Rect, usize)>,
    pub jacks: [Pos2; N_SRC],
    /// the modulation the mod-amount drag (Alt) works on
    pub focus_mod: Option<(u8, u16)>,
    /// typing a value into a knob
    pub typing: Option<(usize, String)>,
    pub reel_ang: [[f32; 2]; 2],
    pub reel_pos: [f32; 2],
    pub vu: [(f32, f32); 2],
    pub kb_note: Option<u8>,
    /// computer-keyboard notes being held: (key, note)
    pub qwerty_held: Vec<(egui::Key, u8)>,
    pub oct: i32,
    pub key_anim: Vec<f32>,
    pub wheel: [f32; 2],
    pub wheel_roll: [f32; 2],
    pub overlay: Overlay,
    pub factory: Vec<PresetData>,
    pub user: Vec<PresetData>,
    pub favs: Vec<String>,
    pub cat: Option<usize>,
    pub search: String,
    pub fav_only: bool,
    pub save_name: Option<String>,
    pub files: Vec<String>,
    pub path_entry: Option<(usize, String)>,
    pub status: Option<(String, f64)>,
    pub lfo_drag: Option<usize>,
    pub env_drag: Option<usize>,
    pub demo_done: bool,
    /// a file dialog is open for layer n; its answer lands here
    pub dialog: Option<(usize, Arc<parking_lot::Mutex<Option<Result<Option<std::path::PathBuf>, ()>>>>)>,
    pub last_dir: Option<std::path::PathBuf>,
    /// filter screen analyzer: smoothed spectra (dB, per FFT bin) before / after the filter
    pub ana: Option<sections::AnaView>,
    /// filter screen drag: (cutoff norm, resonance norm, press position)
    pub fdrag: Option<(f32, f32, Pos2)>,
}

impl Gui {
    fn new() -> Self {
        Self {
            tex: None,
            ease: HashMap::new(),
            t_open: -1.0,
            last: 0.0,
            tab: 0,
            ana: None,
            fdrag: None,
            cable: None,
            cable_back: None,
            targets: Vec::new(),
            targets_next: Vec::new(),
            jacks: [Pos2::ZERO; N_SRC],
            focus_mod: None,
            typing: None,
            reel_ang: [[0.0; 2]; 2],
            reel_pos: [0.3; 2],
            vu: [(0.0, 0.0); 2],
            kb_note: None,
            qwerty_held: Vec::new(),
            oct: 3,
            key_anim: vec![0.0; 128],
            wheel: [0.0; 2],
            wheel_roll: [0.0; 2],
            overlay: Overlay::None,
            factory: presets::factory(),
            user: presets::list_user(),
            favs: presets::load_favorites(),
            cat: None,
            search: String::new(),
            fav_only: false,
            save_name: None,
            files: sources::list_files(),
            path_entry: None,
            status: None,
            lfo_drag: None,
            env_drag: None,
            demo_done: false,
            dialog: None,
            last_dir: None,
        }
    }

    /// Eases a displayed value towards its target; `rate` per second.
    pub fn ease(&mut self, id: egui::Id, target: f32, rate: f32, dt: f32) -> f32 {
        let v = self.ease.entry(id).or_insert(target);
        let k = 1.0 - (-rate * dt).exp();
        *v += (target - *v) * k;
        if (*v - target).abs() < 1e-4 {
            *v = target;
        }
        *v
    }
    pub fn set_ease(&mut self, id: egui::Id, v: f32) {
        self.ease.insert(id, v);
    }
    pub fn say(&mut self, s: impl Into<String>, now: f64) {
        self.status = Some((s.into(), now));
    }
}

pub struct Cx<'a> {
    pub p: &'a GrainParams,
    pub shared: &'a Shared,
    pub setter: &'a ParamSetter<'a>,
    pub o: Pos2,
    pub st: State,
    pub now: f64,
    pub dt: f32,
    /// modulated (normalized) value of every control
    pub live: Vec<f32>,
    pub clock: u64,
    pub sr: f32,
    pub voices: u32,
}

impl Cx<'_> {
    pub fn at(&self, x: f32, y: f32) -> Pos2 {
        Pos2::new(self.o.x + x, self.o.y + y)
    }
    pub fn r(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(self.at(x, y), Vec2::new(w, h))
    }
    pub fn plate(&self, p: [f32; 4]) -> Rect {
        self.r(p[0], p[1], p[2], p[3])
    }
    pub fn norm(&self, d: usize) -> f32 {
        self.p.knob(d).unmodulated_normalized_value()
    }
    pub fn plain(&self, d: usize) -> f32 {
        crate::kinds::def(d).plain(self.norm(d))
    }
    pub fn live_plain(&self, d: usize) -> f32 {
        crate::kinds::def(d).plain(self.live[d])
    }
    pub fn set_norm(&self, d: usize, v: f32) {
        let prm = self.p.knob(d);
        self.setter.begin_set_parameter(prm);
        self.setter.set_parameter_normalized(prm, v.clamp(0.0, 1.0));
        self.setter.end_set_parameter(prm);
    }
    pub fn set_plain(&self, d: usize, v: f32) {
        self.set_norm(d, crate::kinds::def(d).norm(v));
    }
}

/// Changes the saved settings and tells the audio thread.
pub fn edit(cx: &Cx, f: impl FnOnce(&mut State)) {
    {
        let mut st = cx.p.state.write();
        f(&mut st);
        st.sanitize();
    }
    cx.shared.bump_settings();
}

/// Same for the LFO shapes (they are baked into tables).
pub fn edit_waves(cx: &Cx, f: impl FnOnce(&mut State)) {
    edit(cx, f);
    cx.shared.bump_waves();
}

/// Points a layer at another sound (loaded here, on the GUI thread).
pub fn set_source(cx: &Cx, l: usize, src: SourceRef) {
    edit(cx, |st| st.layers[l].source = src.clone());
    let new = sources::load(&src);
    let mut both = [cx.shared.source(0), cx.shared.source(1)];
    both[l] = Some(new);
    cx.shared.set_sources(both);
}

pub fn load_preset(cx: &Cx, d: &PresetData) {
    presets::apply(cx.setter, cx.p, d);
    let st = cx.p.state.read().clone();
    crate::load_sources(&st, cx.shared);
    cx.shared.bump_settings();
    cx.shared.bump_waves();
}

pub fn open_folder(d: &std::path::Path) {
    let _ = std::fs::create_dir_all(d);
    let cmd = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(cmd).arg(d).spawn();
}

pub fn create(params: Arc<GrainParams>, shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    let editor_state = params.editor_state.clone();
    let size_state: Arc<EguiState> = editor_state.clone();
    create_egui_editor(
        editor_state,
        Gui::new(),
        |ctx, g| {
            // a new window gets a new renderer: textures from the last one are gone
            g.tex = None;
            g.t_open = -1.0;
            visuals(ctx);
            install_fonts(ctx);
        },
        move |ctx, setter, g| {
            ctx.request_repaint_after(Duration::from_millis(16));
            if g.tex.is_none() {
                g.tex = Some(Tex::load(ctx));
            }
            let now = ctx.input(|i| i.time);
            if g.t_open < 0.0 {
                g.t_open = now;
                g.last = now;
            }
            let dt = ((now - g.last) as f32).clamp(0.0, 0.1);
            g.last = now;
            shared.purge();

            let p = &*params;
            let live: Vec<f32> = (0..N_DEST).map(|d| shared.live[d].load(Ordering::Relaxed)).collect();
            let st = p.state.read().clone();
            demo_setup(p, &shared, setter, g);

            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::BLACK)).show(ctx, |ui| {
                let full = ui.max_rect();
                let cx = Cx {
                    p,
                    shared: &shared,
                    setter,
                    o: full.min,
                    st,
                    now,
                    dt,
                    live,
                    clock: shared.clock.load(Ordering::Relaxed),
                    sr: shared.sr.load(Ordering::Relaxed).max(8000.0),
                    voices: shared.voices.load(Ordering::Relaxed),
                };
                g.targets = std::mem::take(&mut g.targets_next);

                let tex = g.tex.take().unwrap();
                ui.painter().image(tex.face.id(), Rect::from_min_size(full.min, Vec2::new(BASE_W, BASE_H)), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);

                header::draw(ui, &cx, g, &tex);
                layer::draw(ui, &cx, g, &tex, 0);
                layer::draw(ui, &cx, g, &tex, 1);
                modsec::draw(ui, &cx, g, &tex);
                sections::filter(ui, &cx, g, &tex);
                sections::fx(ui, &cx, g, &tex);
                perf::draw(ui, &cx, g, &tex);
                perf::keyboard(ui, &cx, g, &tex);
                perf::qwerty(ui, &cx, g);

                match g.overlay {
                    Overlay::Presets => header::browser(ui, &cx, g, &tex),
                    Overlay::Matrix => header::matrix(ui, &cx, g, &tex),
                    Overlay::None => {}
                }
                knobs::cables(ui, &cx, g, &tex);
                file_drop(ui, &cx, g, &tex);
                poll_dialog(&cx, g);

                if let Some((_, t)) = g.status {
                    if now - t > 3.0 {
                        g.status = None;
                    }
                }

                // resize grip on the right cheek
                let grip = Rect::from_min_size(full.min + Vec2::new(BASE_W - 18.0, BASE_H - 18.0), Vec2::splat(18.0));
                let r = ui.interact(grip, egui::Id::new("mg_grip"), Sense::drag()).on_hover_text("Drag to resize");
                if r.dragged() {
                    if let Some(pos) = r.interact_pointer_pos() {
                        let ppp = if cfg!(target_os = "macos") { size_state.size().0 as f32 / BASE_W } else { ctx.pixels_per_point() };
                        let w = ((pos.x - full.min.x + 9.0) * ppp).clamp(900.0, 2800.0);
                        size_state.set_requested_size((w.round() as u32, (w * BASE_H / BASE_W).round() as u32));
                    }
                }
                let gc = if r.hovered() || r.dragged() { INK } else { Color32::from_rgba_premultiplied(120, 100, 80, 120) };
                for k in [5.0f32, 9.0, 13.0] {
                    ui.painter().line_segment([Pos2::new(grip.right() - 3.0, grip.bottom() - k), Pos2::new(grip.right() - k, grip.bottom() - 3.0)], Stroke::new(1.0f32, gc));
                }
                let _ = CornerRadius::ZERO;
                g.tex = Some(tex);
            });
        },
    )
}

/// `MAXO_DEMO=1`: something to look at in screenshots (a preset, a cable being patched).
fn demo_setup(p: &GrainParams, shared: &Shared, setter: &ParamSetter, g: &mut Gui) {
    if g.demo_done {
        return;
    }
    g.demo_done = true;
    let Ok(v) = std::env::var("MAXO_DEMO") else { return };
    if let Ok(name) = std::env::var("MAXO_PRESET") {
        let name = name.replace('_', " ");
        if let Some(d) = g.factory.iter().find(|d| d.name == name).cloned() {
            presets::apply(setter, p, &d);
            let st = p.state.read().clone();
            crate::load_sources(&st, shared);
            shared.bump_settings();
            shared.bump_waves();
        }
    }
    if let Ok(t) = std::env::var("MAXO_TAB") {
        g.tab = t.parse().unwrap_or(0);
    }
    match v.as_str() {
        "presets" => g.overlay = Overlay::Presets,
        "matrix" => g.overlay = Overlay::Matrix,
        _ => {}
    }
}

/// Audio files dragged from the desktop onto the window: the half under the pointer (A left,
/// B right) gets the sound.
fn file_drop(ui: &mut egui::Ui, cx: &Cx, g: &mut Gui, tex: &Tex) {
    let (hovered, dropped, ptr) = ui.ctx().input(|i| (i.raw.hovered_files.clone(), i.raw.dropped_files.clone(), i.pointer.hover_pos()));
    let layer = |q: Option<Pos2>| if q.map(|q| q.x - cx.o.x < BASE_W * 0.5).unwrap_or(true) { 0 } else { 1 };
    if !hovered.is_empty() {
        let l = layer(ptr);
        let pl = if l == 0 { P_A } else { P_B };
        let r = cx.r(pl[0] + 24.0, pl[1] + 44.0, 352.0, 150.0);
        let ok = hovered.iter().any(|h| h.path.as_ref().map(|p| sources::is_audio(p)).unwrap_or(true));
        let col = if l == 0 { PHOS_A } else { PHOS_B };
        let p = ui.painter();
        let pulse = 0.6 + 0.4 * ((cx.now * 5.0) as f32).sin();
        p.rect_filled(r, 8.0, Color32::from_black_alpha(215));
        glow(p, tex, r.center(), r.width() * 0.6, col, 0.25 * pulse);
        p.rect_stroke(r.shrink(3.0), 6.0, Stroke::new(2.0f32, with_a(col, 0.8 * pulse)), egui::StrokeKind::Inside);
        let txt = if ok { format!("DROP TO LOAD INTO GRAIN {}", if l == 0 { "A" } else { "B" }) } else { "WAV · AIFF · FLAC · MP3 · OGG · M4A".to_string() };
        lcd_text(p, r.center(), &txt, 13.0, col, egui::Align2::CENTER_CENTER);
    }
    if !dropped.is_empty() {
        let l = layer(ptr);
        let file = dropped.iter().filter_map(|d| d.path.clone()).find(|p| sources::is_audio(p));
        match file {
            Some(path) => {
                if sources::load_audio(&path).is_some() {
                    set_source(cx, l, SourceRef::File(path.to_string_lossy().to_string()));
                    edit(cx, |st| st.layers[l].on = true);
                    g.say(format!("{} → GRAIN {}", path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), if l == 0 { "A" } else { "B" }), cx.now);
                } else {
                    g.say("Could not read that file", cx.now);
                }
            }
            None => g.say("Drop an audio file: WAV, AIFF, FLAC, MP3, OGG, M4A", cx.now),
        }
    }
}

/// Opens the system's file dialog for a layer (on its own thread; macOS wants it on this one).
pub fn open_dialog(g: &mut Gui, l: usize) {
    if g.dialog.is_some() {
        return;
    }
    let slot = Arc::new(parking_lot::Mutex::new(None));
    let dir = g.last_dir.clone().or_else(sources::samples_dir).or_else(dirs::home_dir);
    let run = {
        let slot = slot.clone();
        move || {
            *slot.lock() = Some(pick_file(dir));
        }
    };
    if cfg!(target_os = "macos") {
        run();
    } else {
        std::thread::spawn(run);
    }
    g.dialog = Some((l, slot));
}

fn poll_dialog(cx: &Cx, g: &mut Gui) {
    let Some((l, slot)) = g.dialog.clone() else { return };
    let ans = slot.lock().take();
    if let Some(ans) = ans {
        g.dialog = None;
        let Ok(ans) = ans else {
            // no file dialog on this system: type the path instead
            g.path_entry = Some((l, String::new()));
            return;
        };
        if let Some(path) = ans {
            g.last_dir = path.parent().map(|p| p.to_path_buf());
            if sources::load_audio(&path).is_some() {
                set_source(cx, l, SourceRef::File(path.to_string_lossy().to_string()));
                edit(cx, |st| st.layers[l].on = true);
                g.say(format!("{} → GRAIN {}", path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), if l == 0 { "A" } else { "B" }), cx.now);
            } else {
                g.say("Could not read that file", cx.now);
            }
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn pick_file(dir: Option<std::path::PathBuf>) -> Result<Option<std::path::PathBuf>, ()> {
    let mut d = rfd::FileDialog::new().set_title("Load a sound").add_filter("Audio", &sources::AUDIO_EXT);
    if let Some(dir) = dir {
        d = d.set_directory(dir);
    }
    Ok(d.pick_file())
}

/// Linux: the desktop's own dialog through zenity (GNOME / most desktops) or kdialog (KDE).
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn pick_file(dir: Option<std::path::PathBuf>) -> Result<Option<std::path::PathBuf>, ()> {
    use std::process::Command;
    let start = dir.map(|d| format!("{}/", d.to_string_lossy())).unwrap_or_default();
    let pattern: Vec<String> = sources::AUDIO_EXT.iter().flat_map(|e| [format!("*.{e}"), format!("*.{}", e.to_uppercase())]).collect();
    let tries: Vec<Vec<String>> = vec![
        vec!["zenity".into(), "--file-selection".into(), "--title=Load a sound".into(), format!("--filename={start}"), format!("--file-filter=Audio | {}", pattern.join(" "))],
        vec!["kdialog".into(), "--getopenfilename".into(), if start.is_empty() { ".".into() } else { start.clone() }, format!("Audio ({})", pattern.join(" "))],
        vec!["qarma".into(), "--file-selection".into(), "--title=Load a sound".into()],
    ];
    for t in tries {
        match Command::new(&t[0]).args(&t[1..]).output() {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                return Ok(if o.status.success() && !s.is_empty() { Some(std::path::PathBuf::from(s)) } else { None });
            }
            Err(_) => continue, // not installed: try the next one
        }
    }
    Err(())
}

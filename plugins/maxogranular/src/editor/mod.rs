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
}

impl Gui {
    fn new() -> Self {
        Self {
            tex: None,
            ease: HashMap::new(),
            t_open: -1.0,
            last: 0.0,
            tab: 0,
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

                match g.overlay {
                    Overlay::Presets => header::browser(ui, &cx, g, &tex),
                    Overlay::Matrix => header::matrix(ui, &cx, g, &tex),
                    Overlay::None => {}
                }
                knobs::cables(ui, &cx, g, &tex);

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

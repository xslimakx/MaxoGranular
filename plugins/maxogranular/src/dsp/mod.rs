//! The synth engine: voices with two grain layers, a filter, three envelopes and four LFOs,
//! the modulation matrix, and delay + reverb on the sum.
//!
//! Control values are worked out every `SUB` samples per voice (base value + every modulation
//! routed to it); grains read them when they start, levels glide between updates.

pub mod blocks;
pub mod reverb;
#[cfg(test)]
mod tests;

use crate::kinds::*;
use crate::sources::Source;
use crate::wave::TABLE;
use crate::{GrainParams, ModSlot, Shared, State, MAX_MODS, MAX_VOICES, N_ENVS, N_LAYERS, N_LFOS};
use blocks::*;
use nih_plug::prelude::Param;
use std::f32::consts::{PI, TAU};
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// control rate: values are updated every SUB samples
pub const SUB: usize = 32;
/// grains per layer and voice
pub const MAX_GRAINS: usize = 40;
/// grains in the whole engine (new ones wait when it is full)
pub const GRAIN_BUDGET: usize = 600;
pub const WIN_N: usize = 1024;
const DELAY_MAX_S: f32 = 2.6;

pub struct Transport {
    pub tempo: f64,
    pub pos_beats: Option<f64>,
    pub playing: bool,
}

/// Normalized control values (read from the parameters once per block; tests fill it).
#[derive(Clone)]
pub struct Controls {
    pub n: Vec<f32>,
}

impl Controls {
    pub fn from_params(p: &GrainParams) -> Self {
        Self { n: (0..N_DEST).map(|d| p.knob(d).unmodulated_normalized_value()).collect() }
    }
    pub fn defaults() -> Self {
        Self { n: (0..N_DEST).map(|d| def(d).default_norm()).collect() }
    }
    pub fn set(&mut self, d: usize, plain: f32) {
        self.n[d] = def(d).norm(plain);
    }
}

// ------------------------------------------------------------------------------------------
// Grains
// ------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct Grain {
    /// position in the source (frames) and step per output sample
    pos: f64,
    inc: f64,
    /// window phase 0..1 and its step
    ph: f32,
    dph: f32,
    gl: f32,
    gr: f32,
    win: u8,
    /// window peak position (skew)
    c: f32,
}

#[derive(Clone, Copy)]
struct LayerV {
    grains: [Grain; MAX_GRAINS],
    n: usize,
    /// samples to the next grain, per stream
    next: [f32; 8],
    /// scan offset in source frames
    scan: f64,
    /// level glide
    lvl: f32,
    lvl_to: f32,
}

impl Default for LayerV {
    fn default() -> Self {
        Self { grains: [Grain::default(); MAX_GRAINS], n: 0, next: [0.0; 8], scan: 0.0, lvl: 0.0, lvl_to: 0.0 }
    }
}

// ------------------------------------------------------------------------------------------
// Envelopes, LFOs
// ------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Default, PartialEq)]
enum Stage {
    #[default]
    Idle,
    Attack,
    Hold,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Copy, Default)]
struct EnvS {
    stage: Stage,
    t: f32,
    lvl: f32,
    from: f32,
}

/// curve: 0 linear, >0 slow start, <0 fast start
#[inline]
pub fn curve(t: f32, c: f32) -> f32 {
    if c.abs() < 0.02 {
        t
    } else {
        let k = c * 5.0;
        (fast_exp2(k * t * 1.4427) - 1.0) / (fast_exp2(k * 1.4427) - 1.0)
    }
}

impl EnvS {
    fn gate(&mut self) {
        self.from = self.lvl;
        self.stage = Stage::Attack;
        self.t = 0.0;
    }
    fn release(&mut self) {
        if self.stage != Stage::Idle && self.stage != Stage::Release {
            self.from = self.lvl;
            self.stage = Stage::Release;
            self.t = 0.0;
        }
    }
    /// one step of `dt` ms; a, h, d, r in ms, s 0..1
    fn step(&mut self, dt: f32, a: f32, h: f32, d: f32, s: f32, r: f32, ca: f32, cd: f32) -> f32 {
        self.t += dt;
        match self.stage {
            Stage::Idle => self.lvl = 0.0,
            Stage::Attack => {
                let k = (self.t / a.max(0.05)).min(1.0);
                self.lvl = self.from + (1.0 - self.from) * curve(k, ca);
                if k >= 1.0 {
                    self.stage = if h > 0.0 { Stage::Hold } else { Stage::Decay };
                    self.t = 0.0;
                }
            }
            Stage::Hold => {
                self.lvl = 1.0;
                if self.t >= h {
                    self.stage = Stage::Decay;
                    self.t = 0.0;
                }
            }
            Stage::Decay => {
                let k = (self.t / d.max(0.5)).min(1.0);
                self.lvl = 1.0 - (1.0 - s) * (1.0 - curve(1.0 - k, -cd));
                if k >= 1.0 {
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => self.lvl = s,
            Stage::Release => {
                let k = (self.t / r.max(0.5)).min(1.0);
                self.lvl = self.from * (1.0 - (1.0 - curve(1.0 - k, -cd)));
                if k >= 1.0 {
                    self.stage = Stage::Idle;
                    self.lvl = 0.0;
                }
            }
        }
        self.lvl
    }
    fn stage_code(&self) -> u32 {
        match self.stage {
            Stage::Idle => 0,
            Stage::Attack => 1,
            Stage::Hold => 2,
            Stage::Decay => 3,
            Stage::Sustain => 4,
            Stage::Release => 5,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct LfoS {
    ph: f64,
    out: f32,
    done: bool,
}

// ------------------------------------------------------------------------------------------
// Voice
// ------------------------------------------------------------------------------------------

#[derive(Clone)]
struct Voice {
    active: bool,
    note: u8,
    vel: f32,
    held: bool,
    /// held by the sustain pedal only
    pedal: bool,
    /// order of start (oldest is stolen first)
    serial: u64,
    rand: f32,
    /// current pitch (semitones, MIDI) gliding to `target`
    pitch: f32,
    target: f32,
    env: [EnvS; N_ENVS],
    lfo: [LfoS; N_LFOS],
    /// plain control values for this voice (base + modulation)
    vals: Vec<f32>,
    layers: [LayerV; N_LAYERS],
    filt: [[Svf; 2]; 2],
    fcoef: SvfCoef,
    /// fade-out before a stolen voice restarts
    steal: f32,
    pending: Option<(u8, f32)>,
}

impl Voice {
    fn new(sr: f32) -> Self {
        Self {
            active: false,
            note: 60,
            vel: 1.0,
            held: false,
            pedal: false,
            serial: 0,
            rand: 0.0,
            pitch: 60.0,
            target: 60.0,
            env: [EnvS::default(); N_ENVS],
            lfo: [LfoS::default(); N_LFOS],
            vals: (0..N_DEST).map(|d| def(d).def).collect(),
            layers: [LayerV::default(); N_LAYERS],
            filt: Default::default(),
            fcoef: SvfCoef::new(20000.0f32.min(sr * 0.45), sr, 1.4),
            steal: 1.0,
            pending: None,
        }
    }
}

// ------------------------------------------------------------------------------------------
// Delay
// ------------------------------------------------------------------------------------------

struct Delay {
    buf: Vec<[f32; 2]>,
    w: usize,
    d: f32,
    lp: [f32; 2],
}

impl Delay {
    fn new(sr: f32) -> Self {
        Self { buf: vec![[0.0; 2]; (DELAY_MAX_S * sr) as usize + 8], w: 0, d: 0.3 * sr, lp: [0.0; 2] }
    }
    fn clear(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = [0.0; 2]);
        self.lp = [0.0; 2];
    }
    #[allow(clippy::too_many_arguments)]
    fn process(&mut self, x: [f32; 2], target: f32, k_glide: f32, fb: f32, tone_a: f32, pingpong: bool) -> [f32; 2] {
        let n = self.buf.len();
        self.d += (target.clamp(1.0, (n - 4) as f32) - self.d) * k_glide;
        let read = |ch: usize, buf: &[[f32; 2]], w: usize, d: f32| {
            let pos = w as f32 - d;
            let i = pos.floor();
            let f = pos - i;
            let i0 = (i as isize).rem_euclid(n as isize) as usize;
            buf[i0][ch] * (1.0 - f) + buf[(i0 + 1) % n][ch] * f
        };
        let y = [read(0, &self.buf, self.w, self.d), read(1, &self.buf, self.w, self.d)];
        for c in 0..2 {
            self.lp[c] += (y[c] - self.lp[c]) * tone_a;
        }
        let back = if pingpong { [self.lp[1], self.lp[0]] } else { self.lp };
        let inp = if pingpong { [0.5 * (x[0] + x[1]), 0.0] } else { x };
        self.buf[self.w] = [(inp[0] + back[0] * fb).clamp(-4.0, 4.0), (inp[1] + back[1] * fb).clamp(-4.0, 4.0)];
        self.w = (self.w + 1) % n;
        y
    }
}

// ------------------------------------------------------------------------------------------
// Demo player (screenshots: MAXO_DEMO=1 plays a little progression)
// ------------------------------------------------------------------------------------------

pub struct Demo {
    on: bool,
    t: u64,
    step: usize,
}

impl Demo {
    pub fn new() -> Self {
        Self { on: std::env::var("MAXO_DEMO").is_ok(), t: 0, step: usize::MAX }
    }
    pub fn tick(&mut self, e: &mut Engine, _block_start: bool, _n: usize) {
        if !self.on {
            return;
        }
        let chords: [[u8; 4]; 4] = [[57, 60, 64, 69], [53, 57, 60, 65], [48, 55, 60, 64], [55, 59, 62, 67]];
        let len = (e.sr * 2.0) as u64;
        let step = (self.t / len) as usize;
        if step != self.step {
            if self.step != usize::MAX {
                for &n in &chords[self.step % 4] {
                    e.note_off(n);
                }
            }
            for &n in &chords[step % 4] {
                e.note_on(n, 0.8);
            }
            self.step = step;
        }
        self.t += 1;
    }
}

impl Default for Demo {
    fn default() -> Self {
        Self::new()
    }
}

// ------------------------------------------------------------------------------------------
// Engine
// ------------------------------------------------------------------------------------------

/// The engine's copy of the saved settings.
#[derive(Clone, Copy)]
struct Snap {
    on: [bool; N_LAYERS],
    mode: [u8; N_LAYERS],
    window: [u8; N_LAYERS],
    keytrack: [bool; N_LAYERS],
    streams: [u8; N_LAYERS],
    to_filter: [bool; N_LAYERS],
    root: [u8; N_LAYERS],
    lfo_sync: [bool; N_LFOS],
    lfo_len: [u8; N_LFOS],
    lfo_mode: [u8; N_LFOS],
    curve_a: [f32; N_ENVS],
    curve_d: [f32; N_ENVS],
    filter_on: bool,
    ftype: u8,
    vmode: u8,
    voices: u8,
    bend: u8,
    fx: crate::FxSet,
    mods: [ModSlot; MAX_MODS],
    n_mods: usize,
}

impl Snap {
    fn of(st: &State) -> Self {
        let mut mods = [ModSlot { src: 0, dst: 0, amount: 0.0 }; MAX_MODS];
        for (i, m) in st.mods.iter().take(MAX_MODS).enumerate() {
            mods[i] = *m;
        }
        Snap {
            on: std::array::from_fn(|i| st.layers[i].on),
            mode: std::array::from_fn(|i| st.layers[i].mode),
            window: std::array::from_fn(|i| st.layers[i].window),
            keytrack: std::array::from_fn(|i| st.layers[i].keytrack),
            streams: std::array::from_fn(|i| st.layers[i].streams.clamp(1, 8)),
            to_filter: std::array::from_fn(|i| st.layers[i].filter),
            root: std::array::from_fn(|i| st.layers[i].root),
            lfo_sync: std::array::from_fn(|i| st.lfos[i].sync),
            lfo_len: std::array::from_fn(|i| st.lfos[i].sync_len),
            lfo_mode: std::array::from_fn(|i| st.lfos[i].mode),
            curve_a: std::array::from_fn(|i| st.envs[i].curve_a),
            curve_d: std::array::from_fn(|i| st.envs[i].curve_d),
            filter_on: st.filter.on,
            ftype: st.filter.ftype,
            vmode: st.voice.mode,
            voices: st.voice.voices.clamp(1, MAX_VOICES as u8),
            bend: st.voice.bend,
            fx: st.fx,
            mods,
            n_mods: st.mods.len().min(MAX_MODS),
        }
    }
}

pub struct Engine {
    pub sr: f32,
    snap: Snap,
    set_version: u32,
    wave_version: u32,
    src_version: u32,
    sources: [Option<Arc<Source>>; N_LAYERS],
    lfo_tab: Box<[[f32; TABLE]; N_LFOS]>,
    win_tab: Box<[[f32; WIN_N]; 6]>,
    voices: Vec<Voice>,
    serial: u64,
    /// normalized controls (smoothed) and their plain values
    base_n: Vec<f32>,
    base_p: Vec<f32>,
    /// which controls are a modulation target
    modded: Vec<bool>,
    ctl_n: Vec<f32>,
    pub bend: f32,
    pub wheel: f32,
    pub pressure: f32,
    pedal: bool,
    tempo: f64,
    /// free-running LFO phases (shared by all voices)
    free_ph: [f64; N_LFOS],
    pos_beats: Option<f64>,
    playing: bool,
    rng: u32,
    sub: usize,
    clock: u64,
    delay: Delay,
    reverb: reverb::Reverb,
    vol: f32,
    pk: [f32; 2],
    grains_alive: usize,
    newest: usize,
    k_glide_delay: f32,
}

pub fn make_windows() -> Box<[[f32; WIN_N]; 6]> {
    let mut w = Box::new([[0.0f32; WIN_N]; 6]);
    for i in 0..WIN_N {
        let p = i as f32 / (WIN_N - 1) as f32;
        w[0][i] = 0.5 - 0.5 * (TAU * p).cos();
        let g = (p - 0.5) / 0.18;
        w[1][i] = ((-0.5 * g * g).exp() - (-0.5 * (0.5f32 / 0.18).powi(2)).exp()).max(0.0) / (1.0 - (-0.5 * (0.5f32 / 0.18).powi(2)).exp());
        w[2][i] = 1.0 - (2.0 * p - 1.0).abs();
        w[3][i] = if p < 0.25 {
            0.5 - 0.5 * (PI * p / 0.25).cos()
        } else if p > 0.75 {
            0.5 - 0.5 * (PI * (1.0 - p) / 0.25).cos()
        } else {
            1.0
        };
        let att = 0.04;
        w[4][i] = if p < att { 0.5 - 0.5 * (PI * p / att).cos() } else { ((-(p - att) * 5.0).exp() - (-(1.0 - att) * 5.0f32).exp()).max(0.0) / (1.0 - (-(1.0 - att) * 5.0f32).exp()) };
        w[5][WIN_N - 1 - i] = w[4][i];
    }
    w
}

#[inline]
fn hermite(buf: &[[f32; 2]], pos: f64) -> [f32; 2] {
    let n = buf.len();
    let i = pos.floor();
    let t = (pos - i) as f32;
    let iu = i as usize;
    let (a, b, c, d) = if iu >= 1 && iu + 2 < n {
        (buf[iu - 1], buf[iu], buf[iu + 1], buf[iu + 2])
    } else {
        let i = i as isize;
        let at = |k: isize| buf[(i + k).rem_euclid(n as isize) as usize];
        (at(-1), at(0), at(1), at(2))
    };
    let mut out = [0.0; 2];
    for ch in 0..2 {
        let (xm1, x0, x1, x2) = (a[ch], b[ch], c[ch], d[ch]);
        let c1 = 0.5 * (x1 - xm1);
        let c2 = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
        let c3 = 0.5 * (x2 - xm1) + 1.5 * (x0 - x1);
        out[ch] = ((c3 * t + c2) * t + c1) * t + x0;
    }
    out
}

impl Engine {
    pub fn new(sr: f32) -> Self {
        let st = State::default();
        let ctl = Controls::defaults();
        let base_p: Vec<f32> = (0..N_DEST).map(|d| def(d).plain(ctl.n[d])).collect();
        Self {
            sr,
            snap: Snap::of(&st),
            set_version: 0,
            wave_version: 0,
            src_version: 0,
            sources: [None, None],
            lfo_tab: Box::new([[0.5; TABLE]; N_LFOS]),
            win_tab: make_windows(),
            voices: (0..MAX_VOICES).map(|_| Voice::new(sr)).collect(),
            serial: 0,
            base_n: ctl.n.clone(),
            base_p,
            modded: vec![false; N_DEST],
            ctl_n: ctl.n,
            bend: 0.0,
            wheel: 0.0,
            pressure: 0.0,
            pedal: false,
            tempo: 120.0,
            free_ph: [0.0; N_LFOS],
            pos_beats: None,
            playing: false,
            rng: 0x1234_5678,
            sub: 0,
            clock: 0,
            delay: Delay::new(sr),
            reverb: reverb::Reverb::new(sr, None),
            vol: 0.5,
            pk: [0.0; 2],
            grains_alive: 0,
            newest: 0,
            k_glide_delay: coef_ms(60.0, sr),
        }
    }

    /// Non-real-time: settings, shapes and sources.
    pub fn prepare(&mut self, st: &State, srcs: [Arc<Source>; N_LAYERS]) {
        self.apply(st);
        self.bake(st);
        self.sources = [Some(srcs[0].clone()), Some(srcs[1].clone())];
    }

    pub fn reset(&mut self) {
        for v in self.voices.iter_mut() {
            v.active = false;
            v.layers = [LayerV::default(); N_LAYERS];
        }
        self.delay.clear();
        self.reverb.clear();
    }

    fn apply(&mut self, st: &State) {
        self.snap = Snap::of(st);
        self.modded = vec![false; N_DEST];
        for m in &self.snap.mods[..self.snap.n_mods] {
            self.modded[m.dst as usize] = true;
        }
    }

    fn bake(&mut self, st: &State) {
        for (i, l) in st.lfos.iter().enumerate().take(N_LFOS) {
            l.wave.bake(&mut self.lfo_tab[i]);
        }
    }

    /// Takes the GUI's changes (never waits).
    pub fn sync(&mut self, p: &GrainParams, shared: &Shared) {
        let sv = shared.set_version.load(Ordering::Acquire);
        let wv = shared.wave_version.load(Ordering::Acquire);
        if sv != self.set_version || wv != self.wave_version {
            if let Some(st) = p.state.try_read() {
                if sv != self.set_version {
                    // `apply` allocates a small Vec for `modded`: reuse instead
                    self.snap = Snap::of(&st);
                    self.modded.iter_mut().for_each(|m| *m = false);
                    for m in &self.snap.mods[..self.snap.n_mods] {
                        self.modded[m.dst as usize] = true;
                    }
                    self.set_version = sv;
                }
                if wv != self.wave_version {
                    self.bake(&st);
                    self.wave_version = wv;
                }
            }
        }
        let srv = shared.src_version.load(Ordering::Acquire);
        if srv != self.src_version {
            if let Some(s) = shared.sources.try_lock() {
                // the GUI keeps a copy of the old ones until it sees `src_taken`, so dropping
                // ours never frees memory here
                for (i, x) in s.iter().enumerate() {
                    if let Some(x) = x {
                        if self.sources[i].as_ref().map(|o| !Arc::ptr_eq(o, x)).unwrap_or(true) {
                            self.sources[i] = Some(x.clone());
                            for v in self.voices.iter_mut() {
                                let n = v.layers[i].n;
                                self.grains_alive = self.grains_alive.saturating_sub(n);
                                v.layers[i].n = 0;
                                v.layers[i].scan = 0.0;
                            }
                        }
                    }
                }
                self.src_version = srv;
                shared.src_taken.store(srv, Ordering::Release);
            }
        }
    }

    pub fn begin_block(&mut self, c: &Controls, t: &Transport, shared: &Shared) {
        self.ctl_n.copy_from_slice(&c.n);
        self.tempo = t.tempo.clamp(20.0, 999.0);
        self.pos_beats = t.pos_beats;
        self.playing = t.playing;
        shared.tempo.store(self.tempo as f32, Ordering::Relaxed);
        shared.wheel.store(self.wheel, Ordering::Relaxed);
    }

    fn rnd(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    // --------------------------------------------------------------------------------------
    // Notes
    // --------------------------------------------------------------------------------------

    pub fn note_on(&mut self, note: u8, vel: f32) {
        let mono = self.snap.vmode > 0;
        if mono {
            let legato = self.snap.vmode == 2;
            let v = &mut self.voices[0];
            let was = v.active && v.held && v.env[0].stage != Stage::Release && v.env[0].stage != Stage::Idle;
            v.note = note;
            v.held = true;
            v.pedal = false;
            v.target = note as f32;
            if !v.active {
                v.pitch = note as f32;
            }
            if !(legato && was) {
                self.start_voice(0, note, vel, !v_active(&self.voices[0]));
            }
            self.newest = 0;
            return;
        }
        let n = self.snap.voices as usize;
        // same note held: restart it
        let idx = (0..n)
            .find(|&i| self.voices[i].active && self.voices[i].note == note)
            .or_else(|| (0..n).find(|&i| !self.voices[i].active))
            .unwrap_or_else(|| {
                // steal: oldest released, else oldest
                let rel = (0..n).filter(|&i| !self.voices[i].held).min_by_key(|&i| self.voices[i].serial);
                rel.unwrap_or_else(|| (0..n).min_by_key(|&i| self.voices[i].serial).unwrap_or(0))
            });
        if self.voices[idx].active && self.voices[idx].note != note {
            // fade the old note out quickly, then start
            self.voices[idx].pending = Some((note, vel));
            self.voices[idx].steal = self.voices[idx].steal.min(1.0) - 1e-6;
            self.voices[idx].held = true;
            self.serial += 1;
            self.voices[idx].serial = self.serial;
            self.newest = idx;
            return;
        }
        let fresh = !self.voices[idx].active;
        {
            let v = &mut self.voices[idx];
            v.note = note;
            v.target = note as f32;
            if fresh || self.base_p[M_GLIDE] < 1.0 {
                v.pitch = note as f32;
            }
            v.held = true;
            v.pedal = false;
        }
        self.start_voice(idx, note, vel, fresh);
        self.newest = idx;
    }

    fn start_voice(&mut self, i: usize, note: u8, vel: f32, fresh: bool) {
        self.serial += 1;
        let r = self.rnd();
        let srcs: [f64; N_LAYERS] = std::array::from_fn(|l| self.sources[l].as_ref().map(|s| s.data.len() as f64).unwrap_or(1.0));
        let v = &mut self.voices[i];
        v.active = true;
        v.note = note;
        v.vel = vel;
        v.serial = self.serial;
        v.rand = r;
        v.steal = 1.0;
        v.pending = None;
        for e in v.env.iter_mut() {
            e.gate();
        }
        for (k, l) in v.lfo.iter_mut().enumerate() {
            if self.snap.lfo_mode[k] != 0 {
                l.ph = 0.0;
                l.done = false;
            }
        }
        for (k, ly) in v.layers.iter_mut().enumerate() {
            ly.scan = 0.0;
            ly.next = [0.0; 8];
            if fresh {
                ly.n = 0;
                ly.lvl = 0.0;
            }
            let _ = srcs[k];
        }
        if fresh {
            v.filt = Default::default();
        }
        // the first control update right away
        self.update_voice(i);
    }

    pub fn note_off(&mut self, note: u8) {
        for v in self.voices.iter_mut() {
            if v.active && v.held && v.note == note {
                if self.snap.vmode > 0 && v.pending.is_none() {
                    // mono: nothing else held -> release
                }
                v.held = false;
                if self.pedal {
                    v.pedal = true;
                } else {
                    for e in v.env.iter_mut() {
                        e.release();
                    }
                }
            }
        }
    }

    pub fn sustain(&mut self, down: bool) {
        self.pedal = down;
        if !down {
            for v in self.voices.iter_mut() {
                if v.active && v.pedal && !v.held {
                    v.pedal = false;
                    for e in v.env.iter_mut() {
                        e.release();
                    }
                }
            }
        }
    }

    pub fn all_off(&mut self) {
        for v in self.voices.iter_mut() {
            v.held = false;
            v.pedal = false;
            for e in v.env.iter_mut() {
                e.release();
            }
        }
    }

    // --------------------------------------------------------------------------------------
    // Control rate
    // --------------------------------------------------------------------------------------

    fn lfo_len_s(&self, k: usize, rate_hz: f32) -> f64 {
        if self.snap.lfo_sync[k] {
            SYNC_BEATS[self.snap.lfo_len[k] as usize] * 60.0 / self.tempo
        } else {
            1.0 / rate_hz.max(0.001) as f64
        }
    }

    fn lookup_lfo(&self, k: usize, ph: f32) -> f32 {
        let t = &self.lfo_tab[k];
        let pos = ph.rem_euclid(1.0) * (TABLE - 1) as f32;
        let i = pos as usize;
        let f = pos - i as f32;
        let a = t[i.min(TABLE - 1)];
        let b = t[(i + 1).min(TABLE - 1)];
        (a + (b - a) * f) * 2.0 - 1.0
    }

    /// Control values of voice `i`: sources, modulation, envelope / LFO speeds.
    fn update_voice(&mut self, i: usize) {
        let dt = SUB as f32 / self.sr;
        // LFOs
        let mut lfo_out = [0.0f32; N_LFOS];
        for k in 0..N_LFOS {
            let rate = self.voices[i].vals[lfo_d(k, 0)];
            let smooth = self.voices[i].vals[lfo_d(k, 1)];
            let phase = self.voices[i].vals[lfo_d(k, 2)] / 360.0;
            let len = self.lfo_len_s(k, rate);
            let mode = self.snap.lfo_mode[k];
            let ph = if mode == 0 { self.free_ph[k] } else { self.voices[i].lfo[k].ph };
            let raw = if mode == 2 && self.voices[i].lfo[k].done { self.lookup_lfo(k, 0.9999 + phase) } else { self.lookup_lfo(k, ph as f32 + phase) };
            let l = &mut self.voices[i].lfo[k];
            let a = if smooth < 0.001 { 1.0 } else { (dt / (smooth * len as f32 * 0.25).max(1e-4)).min(1.0) };
            l.out += (raw - l.out) * a;
            lfo_out[k] = l.out;
            if mode != 0 {
                l.ph += dt as f64 / len;
                if l.ph >= 1.0 {
                    if mode == 2 {
                        l.done = true;
                        l.ph = 0.9999;
                    } else {
                        l.ph -= 1.0;
                    }
                }
            }
        }
        let v = &self.voices[i];
        let src: [f32; N_SRC] = [
            v.env[0].lvl,
            v.env[1].lvl,
            v.env[2].lvl,
            lfo_out[0],
            lfo_out[1],
            lfo_out[2],
            lfo_out[3],
            v.vel,
            (v.note as f32 - 60.0) / 60.0,
            self.wheel,
            self.pressure,
            v.rand,
            self.base_p[M_MACRO],
            self.base_p[M_MACRO + 1],
            self.base_p[M_MACRO + 2],
            self.base_p[M_MACRO + 3],
        ];
        let vals = &mut self.voices[i].vals;
        vals.copy_from_slice(&self.base_p);
        if self.snap.n_mods > 0 {
            // sum the modulation in normalized space, then map once per target
            for d in 0..N_DEST {
                if !self.modded[d] {
                    continue;
                }
                let mut n = self.base_n[d];
                for m in &self.snap.mods[..self.snap.n_mods] {
                    if m.dst as usize == d {
                        n += m.amount * src[m.src as usize];
                    }
                }
                vals[d] = def(d).plain(n.clamp(0.0, 1.0));
            }
        }
        for l in 0..N_LAYERS {
            self.voices[i].layers[l].lvl_to = self.voices[i].vals[layer_d(l, L_LEVEL)];
        }
        // filter coefficients
        if self.snap.filter_on {
            let v = &mut self.voices[i];
            let key = (v.pitch - 60.0) / 12.0 * v.vals[F_KEY];
            let fc = (v.vals[F_CUTOFF] * fast_exp2(key)).clamp(16.0, self.sr * 0.45);
            let res = v.vals[F_RES];
            let r1 = (1.0 - res).max(0.0);
            let k = 2.0 * r1 * r1.sqrt() + 0.02;
            v.fcoef = SvfCoef::new(fc, self.sr, k);
        }
    }

    // --------------------------------------------------------------------------------------
    // Audio
    // --------------------------------------------------------------------------------------

    fn spawn(&mut self, vi: usize, l: usize, stream: usize, shared: &Shared) {
        if self.grains_alive >= GRAIN_BUDGET {
            return;
        }
        let Some(src) = self.sources[l].clone() else { return };
        let r: [f32; 5] = [self.rnd(), self.rnd(), self.rnd(), self.rnd(), self.rnd()];
        let sr = self.sr;
        let snap = self.snap;
        let v = &mut self.voices[vi];
        let ly = &mut v.layers[l];
        if ly.n >= MAX_GRAINS {
            return;
        }
        let g = |k: usize| v.vals[layer_d(l, k)];
        let len = src.data.len() as f64;
        let size_s = (g(L_SIZE) * 0.001 * sr).max(16.0);
        // pitch
        let streams = snap.streams[l] as f32;
        let det = if streams > 1.0 { (stream as f32 / (streams - 1.0) - 0.5) * 2.0 * g(L_DETUNE) / 100.0 } else { 0.0 };
        let key = if snap.keytrack[l] && snap.mode[l] == 0 { v.pitch - snap.root[l] as f32 } else { 0.0 };
        let bend = self.bend * snap.bend as f32;
        let semis = g(L_PITCH) + g(L_FINE) / 100.0 + g(L_PRAND) * r[0] + det + key + if snap.mode[l] == 0 { bend } else { 0.0 };
        let rate = (fast_exp2(semis / 12.0) * src.sr / sr) as f64;
        // position
        let base = g(L_POS) as f64 * len + ly.scan;
        let spray = g(L_SPRAY) as f64 * 0.5 * len * r[1] as f64;
        let start = (base + spray).rem_euclid(len);
        let rev = (r[2] * 0.5 + 0.5) < g(L_REVERSE);
        let (pos, inc) = if rev { (start + rate * size_s as f64, -rate) } else { (start, rate) };
        // stereo
        let pan = (g(L_PAN) + g(L_STEREO) * r[3]).clamp(-1.0, 1.0);
        let a = (pan + 1.0) * 0.25 * PI;
        // loudness: many overlapping grains add up
        let overlap = if snap.mode[l] == 1 { size_s / sr * mtof(v.pitch) } else { size_s / sr * g(L_DENSITY) } * streams;
        let amp = 1.0 / overlap.max(1.0).sqrt();
        let sk = g(L_SKEW);
        let n = ly.n;
        ly.grains[n] = Grain { pos, inc, ph: 0.0, dph: 1.0 / size_s, gl: a.cos() * amp, gr: a.sin() * amp, win: snap.window[l], c: 0.5 + 0.45 * sk };
        ly.n += 1;
        self.grains_alive += 1;
        // for the display (every voice: chords show as grains at different heights)
        {
            let w = shared.grain_w.fetch_add(1, Ordering::AcqRel) as usize % crate::GRAIN_RING;
            let gv = &shared.grains[w];
            gv.flags.store(l as u32 | if rev { 2 } else { 0 }, Ordering::Relaxed);
            gv.pos.store((start / len) as f32, Ordering::Relaxed);
            gv.len.store((rate * size_s as f64 / len) as f32, Ordering::Relaxed);
            gv.pitch.store(semis, Ordering::Relaxed);
            gv.pan.store(pan, Ordering::Relaxed);
            gv.amp.store(amp, Ordering::Relaxed);
            gv.birth.store(self.clock, Ordering::Relaxed);
            gv.life.store(size_s as u32, Ordering::Release);
        }
    }

    pub fn process(&mut self, shared: &Shared) -> (f32, f32) {
        let sr = self.sr;
        if self.sub == 0 {
            self.control_tick(shared);
        }
        self.sub = (self.sub + 1) % SUB;
        self.clock += 1;

        let mut dry = [0.0f32; 2];
        let mut wet = [0.0f32; 2];
        let dt_ms = 1000.0 / sr;
        let glide = self.base_p[M_GLIDE];
        let kg = if glide < 1.0 { 1.0 } else { coef_ms(glide, sr) };
        for vi in 0..MAX_VOICES {
            if !self.voices[vi].active {
                continue;
            }
            // envelopes
            let mut amp = 0.0;
            {
                let v = &mut self.voices[vi];
                for e in 0..N_ENVS {
                    let a = v.vals[env_d(e, 0)];
                    let h = v.vals[env_d(e, 1)];
                    let d = v.vals[env_d(e, 2)];
                    let s = v.vals[env_d(e, 3)];
                    let r = v.vals[env_d(e, 4)];
                    let lv = v.env[e].step(dt_ms, a, h, d, s, r, self.snap.curve_a[e], self.snap.curve_d[e]);
                    if e == 0 {
                        amp = lv;
                    }
                }
                v.pitch += (v.target - v.pitch) * kg;
                if v.pending.is_some() {
                    v.steal -= 1.0 / (0.004 * sr);
                }
            }
            // grain clocks
            for l in 0..N_LAYERS {
                if !self.snap.on[l] {
                    continue;
                }
                let streams = self.snap.streams[l] as usize;
                let mode = self.snap.mode[l];
                let (dens, jit, pitch) = {
                    let v = &self.voices[vi];
                    (v.vals[layer_d(l, L_DENSITY)], v.vals[layer_d(l, L_JITTER)], v.pitch)
                };
                for s in 0..streams {
                    self.voices[vi].layers[l].next[s] -= 1.0;
                    if self.voices[vi].layers[l].next[s] <= 0.0 {
                        let r = self.rnd();
                        let interval = if mode == 1 {
                            let det = if streams > 1 { (s as f32 / (streams - 1) as f32 - 0.5) * 2.0 * self.voices[vi].vals[layer_d(l, L_DETUNE)] / 1200.0 } else { 0.0 };
                            let bend = self.bend * self.snap.bend as f32 / 12.0;
                            sr / (mtof(pitch) * fast_exp2(det + bend))
                        } else {
                            sr / dens * (1.0 + jit * r * 0.9)
                        };
                        // first grains of a note: spread the streams
                        let ly = &mut self.voices[vi].layers[l];
                        ly.next[s] += interval.max(1.0);
                        if ly.next[s] < 0.0 {
                            ly.next[s] = 1.0;
                        }
                        self.spawn(vi, l, s, shared);
                    }
                }
                // scan
                let scan = self.voices[vi].vals[layer_d(l, L_SCAN)] as f64;
                if let Some(src) = self.sources[l].as_ref() {
                    let len = src.data.len() as f64;
                    let ly = &mut self.voices[vi].layers[l];
                    ly.scan = (ly.scan + scan * src.sr as f64 / sr as f64).rem_euclid(len);
                }
            }
            // grains
            let mut vout = [[0.0f32; 2]; N_LAYERS];
            for l in 0..N_LAYERS {
                let Some(src) = self.sources[l].as_ref() else { continue };
                let data = &src.data[..];
                let len = data.len() as f64;
                let wt = &self.win_tab;
                let ly = &mut self.voices[vi].layers[l];
                let mut k = 0;
                let mut acc = [0.0f32; 2];
                while k < ly.n {
                    let g = &mut ly.grains[k];
                    // window with its peak moved by the skew
                    let p = if g.ph < g.c { 0.5 * g.ph / g.c } else { 0.5 + 0.5 * (g.ph - g.c) / (1.0 - g.c) };
                    let wi = (p * (WIN_N - 1) as f32) as usize;
                    let w = wt[g.win as usize][wi.min(WIN_N - 1)];
                    let s = hermite(data, g.pos);
                    acc[0] += s[0] * w * g.gl;
                    acc[1] += s[1] * w * g.gr;
                    g.pos += g.inc;
                    if g.pos >= len {
                        g.pos -= len;
                    } else if g.pos < 0.0 {
                        g.pos += len;
                    }
                    g.ph += g.dph;
                    if g.ph >= 1.0 {
                        ly.grains[k] = ly.grains[ly.n - 1];
                        ly.n -= 1;
                        self.grains_alive = self.grains_alive.saturating_sub(1);
                    } else {
                        k += 1;
                    }
                }
                ly.lvl += (ly.lvl_to - ly.lvl) * 0.002;
                vout[l] = [acc[0] * ly.lvl, acc[1] * ly.lvl];
            }
            // filter, amp
            let v = &mut self.voices[vi];
            let vel = 1.0 - v.vals[M_VEL] * (1.0 - v.vel);
            let g = amp * vel * v.steal.max(0.0);
            let mut to_f = [0.0f32; 2];
            let mut by = [0.0f32; 2];
            for l in 0..N_LAYERS {
                if self.snap.filter_on && self.snap.to_filter[l] {
                    to_f[0] += vout[l][0];
                    to_f[1] += vout[l][1];
                } else {
                    by[0] += vout[l][0];
                    by[1] += vout[l][1];
                }
            }
            if self.snap.filter_on {
                let drive = v.vals[F_DRIVE];
                let mix = v.vals[F_MIX];
                let c = v.fcoef;
                for ch in 0..2 {
                    let mut x = to_f[ch];
                    if drive > 0.001 {
                        let d = 1.0 + drive * 8.0;
                        x = (x * d).tanh() / d.sqrt();
                    }
                    let f = &mut v.filt[ch];
                    let (l1, b1, h1) = f[0].run(&c, x);
                    let y = match self.snap.ftype {
                        0 => l1,
                        1 => f[1].run(&c, l1).0,
                        2 => h1,
                        3 => f[1].run(&c, h1).2,
                        4 => b1 * c.k,
                        5 => l1 + h1,
                        _ => x + b1 * c.k * 2.0,
                    };
                    let y = if y.is_finite() { y } else { 0.0 };
                    to_f[ch] = to_f[ch] + (y - to_f[ch]) * mix;
                }
            }
            for ch in 0..2 {
                let s = (to_f[ch] + by[ch]) * g;
                dry[ch] += s;
            }
            // voice end
            if v.env[0].stage == Stage::Idle || v.steal <= 0.0 {
                if let Some((n, vel)) = v.pending.take() {
                    v.steal = 1.0;
                    v.note = n;
                    v.target = n as f32;
                    v.pitch = n as f32;
                    v.layers = [LayerV::default(); N_LAYERS];
                    v.filt = Default::default();
                    self.start_voice(vi, n, vel, true);
                } else {
                    let n: usize = v.layers.iter().map(|l| l.n).sum();
                    self.grains_alive = self.grains_alive.saturating_sub(n);
                    v.active = false;
                    v.layers = [LayerV::default(); N_LAYERS];
                }
            }
        }
        // ---- effects (on the sum)
        let fx = self.snap.fx;
        let bp = &self.base_p;
        if fx.delay_on && bp[X_DMIX] > 0.0005 {
            let ms = if fx.delay_sync { (SYNC_BEATS[fx.delay_div as usize] * 60_000.0 / self.tempo) as f32 } else { bp[X_TIME] };
            let tone_a = coef_hz(800.0 * fast_exp2(bp[X_TONE] * 4.5), sr);
            let y = self.delay.process(dry, (ms.min(DELAY_MAX_S * 1000.0 - 10.0)) * 0.001 * sr, self.k_glide_delay, bp[X_FB], tone_a, fx.pingpong);
            wet[0] += y[0] * bp[X_DMIX];
            wet[1] += y[1] * bp[X_DMIX];
        }
        if fx.reverb_on && bp[X_RMIX] > 0.0005 {
            let inp = [dry[0] + wet[0] * 0.5, dry[1] + wet[1] * 0.5];
            let y = self.reverb.fdn.process(inp, fx.reverb_type as usize, bp[X_PRE], bp[X_DECAY], bp[X_SIZE], bp[X_WIDTH], bp[X_RTONE], false);
            wet[0] += y[0] * bp[X_RMIX];
            wet[1] += y[1] * bp[X_RMIX];
        }
        let target = db2lin(bp[M_VOLUME]);
        self.vol += (target - self.vol) * 0.002;
        let mut out = [(dry[0] + wet[0]) * self.vol, (dry[1] + wet[1]) * self.vol];
        for o in out.iter_mut() {
            *o = if o.is_finite() { safe(*o) } else { 0.0 };
        }
        self.pk[0] = self.pk[0].max(out[0].abs());
        self.pk[1] = self.pk[1].max(out[1].abs());
        (out[0], out[1])
    }

    /// Every SUB samples: smoothed base controls, free LFO clocks, every voice's modulation.
    fn control_tick(&mut self, shared: &Shared) {
        let k = 0.3;
        for d in 0..N_DEST {
            let t = self.ctl_n[d];
            let b = &mut self.base_n[d];
            if (t - *b).abs() > 1e-6 {
                *b += (t - *b) * k;
                self.base_p[d] = def(d).plain(*b);
            }
        }
        let dt = SUB as f64 / self.sr as f64;
        for kk in 0..N_LFOS {
            let len = self.lfo_len_s(kk, self.base_p[lfo_d(kk, 0)]);
            if self.snap.lfo_sync[kk] && self.playing {
                if let Some(pos) = self.pos_beats {
                    self.free_ph[kk] = (pos / SYNC_BEATS[self.snap.lfo_len[kk] as usize]).rem_euclid(1.0);
                    continue;
                }
            }
            self.free_ph[kk] = (self.free_ph[kk] + dt / len).rem_euclid(1.0);
        }
        for i in 0..MAX_VOICES {
            if self.voices[i].active {
                self.update_voice(i);
            }
        }
        let _ = shared;
    }

    pub fn end_block(&mut self, shared: &Shared) {
        shared.clock.store(self.clock, Ordering::Relaxed);
        let i = self.newest;
        let v = &self.voices[i];
        let alive = self.voices.iter().filter(|v| v.active).count();
        shared.voices.store(alive as u32, Ordering::Relaxed);
        shared.grain_count.store(self.grains_alive as u32, Ordering::Relaxed);
        if v.active {
            for d in 0..N_DEST {
                shared.live[d].store(def(d).norm(v.vals[d]), Ordering::Relaxed);
            }
            for e in 0..N_ENVS {
                shared.env_lvl[e].store(v.env[e].lvl, Ordering::Relaxed);
                shared.env_stage[e].store(((v.env[e].t.min(16_000_000.0)) as u32) | (v.env[e].stage_code() << 24), Ordering::Relaxed);
            }
            for k in 0..N_LFOS {
                let ph = if self.snap.lfo_mode[k] == 0 { self.free_ph[k] } else { v.lfo[k].ph };
                shared.lfo_ph[k].store(ph as f32, Ordering::Relaxed);
                shared.lfo_val[k].store(v.lfo[k].out, Ordering::Relaxed);
            }
            for l in 0..N_LAYERS {
                let len = self.sources[l].as_ref().map(|s| s.data.len() as f64).unwrap_or(1.0);
                shared.scan[l].store((v.layers[l].scan / len) as f32, Ordering::Relaxed);
            }
        } else {
            for d in 0..N_DEST {
                shared.live[d].store(self.base_n[d], Ordering::Relaxed);
            }
            for e in 0..N_ENVS {
                shared.env_lvl[e].store(0.0, Ordering::Relaxed);
                shared.env_stage[e].store(0, Ordering::Relaxed);
            }
            for k in 0..N_LFOS {
                shared.lfo_ph[k].store(self.free_ph[k] as f32, Ordering::Relaxed);
            }
        }
        for n in 0..128 {
            let on = self.voices.iter().any(|v| v.active && v.note as usize == n && (v.held || v.pedal));
            shared.notes_on[n].store(on, Ordering::Relaxed);
        }
        for c in 0..2 {
            let p = shared.out_pk[c].load(Ordering::Relaxed);
            shared.out_pk[c].store(p.max(self.pk[c]), Ordering::Relaxed);
            self.pk[c] = 0.0;
        }
        let ms = if self.snap.fx.delay_sync { (SYNC_BEATS[self.snap.fx.delay_div as usize] * 60_000.0 / self.tempo) as f32 } else { self.base_p[X_TIME] };
        shared.delay_ms.store(ms, Ordering::Relaxed);
    }
}

fn v_active(v: &Voice) -> bool {
    v.active
}

#[inline]
pub fn mtof(n: f32) -> f32 {
    440.0 * fast_exp2((n - 69.0) / 12.0)
}

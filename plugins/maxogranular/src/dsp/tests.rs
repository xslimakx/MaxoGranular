//! Engine tests: sound, silence, polyphony, every source / mode / window, modulation, clicks, speed.

use super::*;
use crate::sources;
use crate::wave::{Preset, Wave};
use crate::{LayerSet, State};

pub struct Rig {
    pub st: State,
    pub ctl: Controls,
    pub e: Engine,
    pub shared: Shared,
    pub t: Transport,
}

impl Rig {
    pub fn new() -> Self {
        Self { st: State::default(), ctl: Controls::defaults(), e: Engine::new(48000.0), shared: Shared::default(), t: Transport { tempo: 120.0, pos_beats: None, playing: false } }
    }
    pub fn ready(&mut self) {
        self.st.sanitize();
        self.e = Engine::new(48000.0);
        let srcs = [Arc::new(sources::builtin(match self.st.layers[0].source {
            sources::SourceRef::Builtin(i) => i as usize,
            _ => 0,
        })), Arc::new(sources::builtin(match self.st.layers[1].source {
            sources::SourceRef::Builtin(i) => i as usize,
            _ => 0,
        }))];
        self.e.prepare(&self.st, srcs);
    }
    pub fn run(&mut self, n: usize) -> Vec<[f32; 2]> {
        let mut out = Vec::with_capacity(n);
        let mut left = n;
        while left > 0 {
            let b = left.min(256);
            self.e.begin_block(&self.ctl, &self.t, &self.shared);
            for _ in 0..b {
                let (l, r) = self.e.process(&self.shared);
                out.push([l, r]);
            }
            self.e.end_block(&self.shared);
            left -= b;
        }
        out
    }
}

pub fn rms(v: &[[f32; 2]]) -> f32 {
    (v.iter().map(|s| s[0] * s[0] + s[1] * s[1]).sum::<f32>() / (2 * v.len().max(1)) as f32).sqrt()
}

pub fn peak(v: &[[f32; 2]]) -> f32 {
    v.iter().map(|s| s[0].abs().max(s[1].abs())).fold(0.0, f32::max)
}

#[test]
fn silent_without_notes() {
    let mut r = Rig::new();
    r.ready();
    let y = r.run(48000);
    assert_eq!(peak(&y), 0.0);
}

#[test]
fn a_note_sounds_and_dies_away() {
    let mut r = Rig::new();
    r.ctl.set(X_RMIX, 0.0);
    r.ctl.set(X_DMIX, 0.0);
    r.ready();
    r.e.note_on(60, 0.9);
    let y = r.run(48000);
    assert!(rms(&y[4800..]) > 0.01, "{}", rms(&y));
    assert!(y.iter().all(|s| s[0].is_finite() && s[1].is_finite()));
    r.e.note_off(60);
    let z = r.run(48000 * 2);
    assert!(peak(&z[48000..]) < 1e-4, "tail {}", peak(&z[48000..]));
    assert_eq!(r.shared.voices.load(Ordering::Relaxed), 0);
}

#[test]
fn every_source_mode_and_window_is_sane() {
    for src in 0..sources::BUILTIN.len() {
        for mode in 0..2u8 {
            let mut r = Rig::new();
            r.st.layers[0] = LayerSet { source: sources::SourceRef::Builtin(src as u16), mode, window: (src % 6) as u8, streams: 1 + (src % 3) as u8, ..LayerSet::default() };
            r.ctl.set(layer_d(0, L_SIZE), if mode == 1 { 8.0 } else { 150.0 });
            r.ready();
            r.e.note_on(48 + (src as u8 % 24), 1.0);
            let y = r.run(24000);
            let pk = peak(&y);
            assert!(y.iter().all(|s| s[0].is_finite()), "{src} {mode}");
            assert!(pk < 1.01 && rms(&y[4800..]) > 1e-3, "{} mode {mode}: peak {pk} rms {}", sources::BUILTIN[src].1, rms(&y[4800..]));
        }
    }
}

#[test]
fn polyphony_and_stealing() {
    let mut r = Rig::new();
    r.st.voice.voices = 4;
    r.ready();
    for n in 0..10u8 {
        r.e.note_on(48 + n * 3, 0.8);
        let _ = r.run(2000);
    }
    let alive = r.shared.voices.load(Ordering::Relaxed);
    assert!(alive <= 4, "{alive}");
    // stealing fades: no jump bigger than the music itself
    let y = r.run(4800);
    assert!(y.iter().all(|s| s[0].is_finite()));
}

#[test]
fn pitch_follows_the_keys() {
    // Sync mode: the grain rate is the note frequency, so the period shows in the output
    let mut r = Rig::new();
    r.st.layers[0] = LayerSet { source: sources::SourceRef::Builtin(12), mode: 1, window: 0, ..LayerSet::default() };
    r.ctl.set(layer_d(0, L_SIZE), 1.5);
    r.ctl.set(layer_d(0, L_STEREO), 0.0);
    r.ctl.set(layer_d(0, L_SPRAY), 0.0);
    r.ctl.set(X_RMIX, 0.0);
    r.ctl.set(X_DMIX, 0.0);
    r.ready();
    r.e.note_on(69, 1.0);
    let y = r.run(48000);
    // autocorrelation peak near 48000 / 440
    let x: Vec<f32> = y[12000..36000].iter().map(|s| s[0]).collect();
    let best = (60..200).max_by(|&a, &b| {
        let c = |lag: usize| x.iter().zip(x[lag..].iter()).map(|(p, q)| p * q).sum::<f32>();
        c(a).partial_cmp(&c(b)).unwrap()
    });
    let lag = best.unwrap();
    assert!((lag as f32 - 109.1).abs() < 3.0, "lag {lag}");
}

#[test]
fn modulation_moves_the_target() {
    let mut r = Rig::new();
    r.st.mods.push(crate::ModSlot { src: S_LFO as u8, dst: F_CUTOFF as u16, amount: -0.8 });
    r.st.lfos[0].wave = Wave::preset(Preset::SawUp);
    r.ctl.set(lfo_d(0, 0), 4.0);
    r.ready();
    r.e.note_on(60, 1.0);
    let mut seen = Vec::new();
    for _ in 0..40 {
        r.run(1200);
        seen.push(r.shared.live[F_CUTOFF].load(Ordering::Relaxed));
    }
    let (lo, hi) = seen.iter().fold((1.0f32, 0.0f32), |a, &v| (a.0.min(v), a.1.max(v)));
    assert!(hi - lo > 0.4, "{lo} .. {hi}");
}

#[test]
fn notes_start_and_stop_without_clicks() {
    let mut r = Rig::new();
    r.st.layers[0].source = sources::SourceRef::Builtin(12);
    r.ctl.set(X_RMIX, 0.0);
    r.ctl.set(X_DMIX, 0.0);
    r.ctl.set(env_d(0, 0), 2.0);
    r.ctl.set(env_d(0, 4), 30.0);
    r.ready();
    let mut y = Vec::new();
    for k in 0..12u8 {
        r.e.note_on(50 + k, 1.0);
        y.extend(r.run(3000));
        r.e.note_off(50 + k);
        y.extend(r.run(1000));
    }
    let step = y.windows(2).map(|w| (w[1][0] - w[0][0]).abs()).fold(0.0, f32::max);
    assert!(step < 0.25, "jump {step}");
}

#[test]
fn effects_ring_and_fade() {
    let mut r = Rig::new();
    r.ctl.set(X_RMIX, 0.5);
    r.ctl.set(X_DMIX, 0.5);
    r.ready();
    r.e.note_on(60, 1.0);
    r.run(9600);
    r.e.note_off(60);
    let y = r.run(48000);
    assert!(rms(&y[24000..36000]) > 1e-4, "no tail");
    assert!(y.iter().all(|s| s[0].is_finite()));
}

#[test]
fn cpu_budget() {
    let mut r = Rig::new();
    r.st.layers[1].on = true;
    r.st.layers[0].streams = 3;
    r.ctl.set(layer_d(0, L_DENSITY), 60.0);
    r.ctl.set(layer_d(0, L_SIZE), 250.0);
    r.ctl.set(layer_d(1, L_DENSITY), 40.0);
    r.ctl.set(X_RMIX, 0.3);
    r.ctl.set(X_DMIX, 0.3);
    r.ready();
    for n in [48u8, 55, 60, 64, 67, 72, 76, 79] {
        r.e.note_on(n, 0.8);
    }
    r.run(4800);
    let t = std::time::Instant::now();
    r.run(48000);
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    eprintln!("1 s, 8 notes, two layers, 3 streams, fx: {ms:.1} ms, grains {}", r.shared.grain_count.load(Ordering::Relaxed));
    assert!(ms < 600.0);
}

/// Every factory preset sounds, stays finite and bounded, and dies away after the keys go up.
#[test]
fn every_factory_preset_plays_clean() {
    use crate::kinds::{def, N_DEST};
    use crate::presets::{factory, param_id};
    let mut bad = Vec::new();
    for d in factory() {
        let mut r = Rig::new();
        r.st = d.state.clone();
        r.ready();
        for k in 0..N_DEST {
            let n = d.params.get(&param_id(k)).copied().unwrap_or_else(|| def(k).default_norm());
            r.ctl.set(k, def(k).plain(n));
        }
        r.run(2400);
        r.e.note_on(57, 0.8);
        r.e.note_on(64, 0.8);
        let on = r.run(72000);
        r.e.note_off(57);
        r.e.note_off(64);
        let _tail = r.run(48000 * 6);
        let end = r.run(4800);
        let finite = on.iter().all(|s| s[0].is_finite() && s[1].is_finite());
        let (pk, lv) = (peak(&on), rms(&on[24000..]));
        if !finite || pk > 1.6 || lv < 1e-4 || peak(&end) > 0.05 {
            bad.push(format!("{}: finite {finite} peak {pk:.2} rms {lv:.5} end {:.4}", d.name, peak(&end)));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

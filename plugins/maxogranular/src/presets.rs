//! Presets: every control (normalized, by parameter id) plus the saved settings.
//! Factory presets are built in code; user presets are JSON files in
//! `<data dir>/MaxoGranular/Presets`.

use crate::kinds::*;
use crate::sources::SourceRef;
use crate::wave::{Preset as W, Pt, Wave};
use crate::{GrainParams, ModSlot, State};
use nih_plug::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const CATEGORIES: [&str; 9] = ["Pads", "Textures", "Keys", "Plucks", "Bass", "Leads", "Voices", "Rhythmic", "FX"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PresetData {
    pub name: String,
    pub category: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub params: BTreeMap<String, f32>,
    pub state: State,
}

pub fn param_id(d: usize) -> String {
    format!("v_{}", d + 1)
}

// ------------------------------------------------------------------------------------------
// Building presets in code
// ------------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct Builder {
    pub st: State,
    pub n: Vec<f32>,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        let mut st = State::default();
        st.layers[1].on = false;
        let mut b = Self { st, n: (0..N_DEST).map(|d| def(d).default_norm()).collect() };
        // factory defaults: a bit of space
        b.k(X_RMIX, 0.18).k(X_DMIX, 0.0);
        b
    }
    /// A control in its own units.
    pub fn k(&mut self, d: usize, plain: f32) -> &mut Self {
        self.n[d] = def(d).norm(plain);
        self
    }
    pub fn a(&mut self, k: usize, plain: f32) -> &mut Self {
        self.k(layer_d(0, k), plain)
    }
    pub fn b(&mut self, k: usize, plain: f32) -> &mut Self {
        self.k(layer_d(1, k), plain)
    }
    pub fn src(&mut self, l: usize, i: u16) -> &mut Self {
        self.st.layers[l].source = SourceRef::Builtin(i);
        self.st.layers[l].on = true;
        self
    }
    pub fn layer(&mut self, l: usize, f: impl FnOnce(&mut crate::LayerSet)) -> &mut Self {
        f(&mut self.st.layers[l]);
        self
    }
    /// envelope: attack, hold, decay, sustain, release (ms / 0..1)
    pub fn env(&mut self, e: usize, a: f32, d: f32, s: f32, r: f32) -> &mut Self {
        self.k(env_d(e, 0), a).k(env_d(e, 2), d).k(env_d(e, 3), s).k(env_d(e, 4), r)
    }
    pub fn lfo(&mut self, k: usize, w: Wave, hz: f32) -> &mut Self {
        self.st.lfos[k].wave = w;
        self.st.lfos[k].sync = false;
        self.k(lfo_d(k, 0), hz)
    }
    pub fn lfo_sync(&mut self, k: usize, w: Wave, len: &str) -> &mut Self {
        self.st.lfos[k].wave = w;
        self.st.lfos[k].sync = true;
        self.st.lfos[k].sync_len = SYNC_NAMES.iter().position(|&s| s == len).unwrap_or(7) as u8;
        self
    }
    pub fn lfo_mode(&mut self, k: usize, mode: u8) -> &mut Self {
        self.st.lfos[k].mode = mode;
        self
    }
    pub fn m(&mut self, src: usize, dst: usize, amount: f32) -> &mut Self {
        self.st.mods.push(ModSlot { src: src as u8, dst: dst as u16, amount });
        self
    }
    pub fn filter(&mut self, ftype: u8, cutoff: f32, res: f32) -> &mut Self {
        self.st.filter.on = true;
        self.st.filter.ftype = ftype;
        self.k(F_CUTOFF, cutoff).k(F_RES, res)
    }
    pub fn verb(&mut self, t: u8, mix: f32, decay: f32) -> &mut Self {
        self.st.fx.reverb_type = t;
        self.k(X_RMIX, mix).k(X_DECAY, decay)
    }
    pub fn delay(&mut self, div: &str, mix: f32, fb: f32) -> &mut Self {
        self.st.fx.delay_sync = true;
        self.st.fx.delay_div = SYNC_NAMES.iter().position(|&s| s == div).unwrap_or(9) as u8;
        self.k(X_DMIX, mix).k(X_FB, fb)
    }
    pub fn mono(&mut self, legato: bool, glide: f32) -> &mut Self {
        self.st.voice.mode = if legato { 2 } else { 1 };
        self.k(M_GLIDE, glide)
    }
    pub fn finish(&self, name: &str, cat: &str) -> PresetData {
        let mut st = self.st.clone();
        st.preset = name.into();
        st.sanitize();
        PresetData { name: name.into(), category: cat.into(), author: "Factory".into(), params: (0..N_DEST).map(|d| (param_id(d), self.n[d])).collect(), state: st }
    }
}

fn ramp(a: f32, b: f32, c: f32) -> Wave {
    Wave::from(&[Pt::new(0.0, a, c), Pt::new(1.0, b, 0.0)])
}

fn wv(p: W) -> Wave {
    Wave::preset(p)
}

fn steps(seed: u32, n: usize) -> Wave {
    let lv: Vec<f32> = (0..n)
        .map(|i| {
            let mut r = seed.wrapping_mul(2654435761).wrapping_add(i as u32 * 40503);
            r ^= r >> 13;
            r = r.wrapping_mul(0x5bd1_e995);
            r ^= r >> 15;
            ((r % 1000) as f32 / 1000.0 * 4.0).round() / 4.0
        })
        .collect();
    Wave::steps(&lv)
}

// source numbers (sources::BUILTIN)
const CHOIR: u16 = 0;
const VOWEL: u16 = 1;
const WHISPER: u16 = 2;
const BELLS: u16 = 3;
const MUSICBOX: u16 = 4;
const PIANO: u16 = 5;
const ORGAN: u16 = 6;
const FMBELL: u16 = 7;
const STRINGS: u16 = 8;
const GUITAR: u16 = 9;
const HARP: u16 = 10;
const KALIMBA: u16 = 11;
const SAW: u16 = 12;
const SQUARE: u16 = 13;
const SUB: u16 = 14;
const FORMANT: u16 = 15;
const FLUTE: u16 = 16;
const BRASS: u16 = 17;
const RAIN: u16 = 18;
const WIND: u16 = 19;
const METAL: u16 = 20;
const VINYL: u16 = 21;
const DRUMS: u16 = 22;
const GLITCH: u16 = 23;

/// Every factory preset.
pub fn factory() -> Vec<PresetData> {
    let mut out = Vec::new();
    let mut add = |name: &str, cat: &str, f: &dyn Fn(&mut Builder)| {
        let mut b = Builder::new();
        f(&mut b);
        out.push(b.finish(name, cat));
    };

    add("Init", "Pads", &|_| {});

    // ---------------- Pads
    add("Cloud Choir", "Pads", &|b| {
        b.src(0, CHOIR).a(L_POS, 0.35).a(L_SPRAY, 0.12).a(L_SIZE, 260.0).a(L_DENSITY, 30.0).a(L_SCAN, 0.08).env(0, 900.0, 800.0, 0.8, 2200.0).verb(0, 0.4, 0.75);
        b.layer(0, |l| l.window = 1);
        b.lfo(0, wv(W::Sine), 0.15).m(S_LFO, layer_d(0, L_POS), 0.12);
    });
    add("Silk Strings", "Pads", &|b| {
        b.src(0, STRINGS).a(L_POS, 0.4).a(L_SPRAY, 0.2).a(L_SIZE, 400.0).a(L_DENSITY, 22.0).a(L_STEREO, 0.8).env(0, 1200.0, 600.0, 0.85, 2600.0).verb(7, 0.35, 0.7);
        b.layer(0, |l| l.streams = 2);
        b.filter(1, 6000.0, 0.15).m(S_WHEEL, F_CUTOFF, -0.3);
    });
    add("Frozen Saw", "Pads", &|b| {
        b.src(0, SAW).a(L_POS, 0.5).a(L_SCAN, 0.0).a(L_SPRAY, 0.03).a(L_SIZE, 180.0).a(L_DENSITY, 40.0).env(0, 600.0, 500.0, 0.9, 1800.0);
        b.layer(0, |l| l.streams = 3);
        b.filter(1, 2500.0, 0.25).lfo(0, wv(W::Triangle), 0.08).m(S_LFO, F_CUTOFF, 0.25).verb(0, 0.35, 0.65);
    });
    add("Glass Haze", "Pads", &|b| {
        b.src(0, BELLS).a(L_POS, 0.15).a(L_SPRAY, 0.4).a(L_SIZE, 500.0).a(L_DENSITY, 18.0).a(L_PRAND, 0.1).env(0, 1500.0, 1000.0, 0.8, 3500.0);
        b.layer(0, |l| l.window = 1);
        b.src(1, CHOIR).b(L_LEVEL, 0.45).b(L_SIZE, 300.0).b(L_SPRAY, 0.1).b(L_PITCH, 12.0).verb(9, 0.45, 0.8);
    });
    add("Slow Bloom", "Pads", &|b| {
        b.src(0, BRASS).a(L_POS, 0.1).a(L_SCAN, 0.15).a(L_SIZE, 350.0).a(L_DENSITY, 24.0).env(0, 2500.0, 1000.0, 0.85, 3000.0);
        b.filter(1, 800.0, 0.2).env(1, 3000.0, 2000.0, 0.7, 2000.0).m(S_ENV + 1, F_CUTOFF, 0.5).verb(0, 0.35, 0.7);
    });
    add("Organ Mist", "Pads", &|b| {
        b.src(0, ORGAN).a(L_POS, 0.5).a(L_SPRAY, 0.3).a(L_SIZE, 220.0).a(L_DENSITY, 35.0).a(L_STEREO, 0.9).env(0, 400.0, 500.0, 0.9, 1500.0);
        b.lfo(0, wv(W::Sine), 5.5).m(S_LFO, layer_d(0, L_FINE), 0.06).verb(4, 0.3, 0.6);
    });
    add("Dream Layers", "Pads", &|b| {
        b.src(0, STRINGS).a(L_SIZE, 300.0).a(L_DENSITY, 20.0).a(L_SPRAY, 0.15);
        b.src(1, FLUTE).b(L_PITCH, 12.0).b(L_LEVEL, 0.5).b(L_SIZE, 200.0).b(L_SCAN, 0.2);
        b.env(0, 1000.0, 1000.0, 0.8, 3000.0).verb(8, 0.4, 0.7).delay("1/4.", 0.15, 0.4);
    });
    add("Shimmer Hall", "Pads", &|b| {
        b.src(0, CHOIR).a(L_SIZE, 400.0).a(L_DENSITY, 25.0).a(L_SPRAY, 0.25).a(L_PITCH, 12.0).env(0, 2000.0, 1000.0, 0.8, 4000.0);
        b.src(1, SAW).b(L_LEVEL, 0.5).b(L_SIZE, 250.0);
        b.verb(9, 0.55, 0.85);
    });
    add("Wide Analog", "Pads", &|b| {
        b.src(0, SAW).a(L_SIZE, 120.0).a(L_DENSITY, 60.0).a(L_SPRAY, 0.02).a(L_STEREO, 1.0).a(L_DETUNE, 25.0).env(0, 300.0, 600.0, 0.8, 1200.0);
        b.layer(0, |l| l.streams = 4);
        b.filter(1, 3000.0, 0.3).env(1, 10.0, 900.0, 0.3, 800.0).m(S_ENV + 1, F_CUTOFF, 0.35).verb(0, 0.3, 0.6);
    });

    // ---------------- Textures
    add("Rain on Glass", "Textures", &|b| {
        b.src(0, RAIN).a(L_SIZE, 80.0).a(L_DENSITY, 60.0).a(L_SPRAY, 1.0).a(L_PRAND, 3.0).env(0, 500.0, 500.0, 1.0, 2000.0);
        b.src(1, BELLS).b(L_LEVEL, 0.3).b(L_SIZE, 60.0).b(L_DENSITY, 8.0).b(L_SPRAY, 1.0).b(L_PRAND, 12.0).verb(7, 0.4, 0.8);
    });
    add("Wind Tunnel", "Textures", &|b| {
        b.src(0, WIND).a(L_SIZE, 600.0).a(L_DENSITY, 12.0).a(L_SCAN, 0.5).env(0, 1500.0, 1000.0, 1.0, 3000.0);
        b.filter(4, 800.0, 0.6).lfo(0, wv(W::Sine), 0.1).m(S_LFO, F_CUTOFF, 0.35).verb(8, 0.4, 0.8);
    });
    add("Metal Dust", "Textures", &|b| {
        b.src(0, METAL).a(L_SIZE, 40.0).a(L_DENSITY, 90.0).a(L_SPRAY, 1.0).a(L_PRAND, 7.0).a(L_REVERSE, 0.5).env(0, 200.0, 500.0, 0.9, 2000.0).verb(12, 0.4, 0.7);
    });
    add("Vinyl Ghosts", "Textures", &|b| {
        b.src(0, VINYL).a(L_SIZE, 300.0).a(L_DENSITY, 6.0).a(L_SPRAY, 1.0).env(0, 100.0, 500.0, 1.0, 1500.0);
        b.src(1, PIANO).b(L_LEVEL, 0.6).b(L_SIZE, 450.0).b(L_DENSITY, 5.0).b(L_SPRAY, 0.6).b(L_REVERSE, 0.6);
        b.filter(1, 3500.0, 0.1).verb(3, 0.35, 0.6);
    });
    add("Particle Swarm", "Textures", &|b| {
        b.src(0, GLITCH).a(L_SIZE, 15.0).a(L_DENSITY, 150.0).a(L_SPRAY, 1.0).a(L_PRAND, 12.0).a(L_STEREO, 1.0).env(0, 50.0, 500.0, 1.0, 1200.0);
        b.filter(4, 3000.0, 0.4).lfo(0, steps(5, 8), 2.0).m(S_LFO, F_CUTOFF, 0.4).delay("1/16", 0.2, 0.5);
    });
    add("Ocean Pad", "Textures", &|b| {
        b.src(0, WIND).a(L_SIZE, 800.0).a(L_DENSITY, 8.0);
        b.src(1, STRINGS).b(L_LEVEL, 0.5).b(L_SIZE, 300.0).b(L_SPRAY, 0.3);
        b.lfo(0, wv(W::Sine), 0.07).m(S_LFO, layer_d(0, L_LEVEL), 0.3).env(0, 1500.0, 1000.0, 1.0, 3000.0).verb(8, 0.45, 0.8);
    });
    add("Scrapyard", "Textures", &|b| {
        b.src(0, METAL).a(L_SIZE, 250.0).a(L_DENSITY, 10.0).a(L_SCAN, -0.6).a(L_REVERSE, 0.3);
        b.src(1, SUB).b(L_LEVEL, 0.5).b(L_SIZE, 120.0).b(L_PITCH, -12.0).filter(0, 1500.0, 0.5).verb(12, 0.35, 0.6);
    });
    add("Whisper Room", "Textures", &|b| {
        b.src(0, WHISPER).a(L_SIZE, 180.0).a(L_DENSITY, 25.0).a(L_SPRAY, 0.6).a(L_STEREO, 1.0).env(0, 400.0, 500.0, 1.0, 2000.0).verb(3, 0.4, 0.6);
    });
    add("Frozen Moment", "Textures", &|b| {
        b.src(0, PIANO).a(L_POS, 0.25).a(L_SCAN, 0.0).a(L_SPRAY, 0.01).a(L_SIZE, 600.0).a(L_DENSITY, 12.0).a(L_STEREO, 1.0).env(0, 800.0, 1000.0, 1.0, 4000.0).verb(8, 0.5, 0.85);
        b.layer(0, |l| l.window = 1);
    });

    // ---------------- Keys
    add("Felt Grains", "Keys", &|b| {
        b.src(0, PIANO).a(L_POS, 0.0).a(L_SCAN, 1.0).a(L_SPRAY, 0.005).a(L_SIZE, 90.0).a(L_DENSITY, 40.0).env(0, 2.0, 1800.0, 0.0, 600.0).verb(3, 0.25, 0.5);
    });
    add("Music Box Dust", "Keys", &|b| {
        b.src(0, MUSICBOX).a(L_POS, 0.0).a(L_SCAN, 0.8).a(L_SPRAY, 0.02).a(L_SIZE, 70.0).a(L_DENSITY, 45.0).a(L_STEREO, 0.8).env(0, 1.0, 1500.0, 0.0, 900.0);
        b.delay("1/8.", 0.25, 0.45).verb(7, 0.3, 0.7);
    });
    add("FM Bell Grains", "Keys", &|b| {
        b.src(0, FMBELL).a(L_POS, 0.0).a(L_SCAN, 1.0).a(L_SIZE, 120.0).a(L_DENSITY, 30.0).env(0, 1.0, 2500.0, 0.0, 1200.0).verb(5, 0.3, 0.6);
    });
    add("Organ Pulse", "Keys", &|b| {
        b.src(0, ORGAN).a(L_SIZE, 40.0).a(L_DENSITY, 8.0).a(L_JITTER, 0.0).env(0, 5.0, 300.0, 0.8, 300.0);
        b.lfo_sync(0, wv(W::Square), "1/16").m(S_LFO, layer_d(0, L_LEVEL), -0.4);
    });
    add("Electric Glass", "Keys", &|b| {
        b.src(0, BELLS).a(L_POS, 0.0).a(L_SCAN, 1.0).a(L_SIZE, 60.0).a(L_DENSITY, 55.0).env(0, 1.0, 1200.0, 0.3, 700.0);
        b.src(1, SAW).b(L_LEVEL, 0.35).b(L_SIZE, 100.0).filter(1, 4000.0, 0.2).m(S_VEL, F_CUTOFF, 0.3).verb(5, 0.25, 0.5);
    });
    add("Sync Keys", "Keys", &|b| {
        b.src(0, PIANO).layer(0, |l| l.mode = 1).a(L_SIZE, 6.0).a(L_PITCH, 7.0).env(0, 2.0, 800.0, 0.4, 500.0);
        b.env(1, 1.0, 400.0, 0.0, 300.0).m(S_ENV + 1, layer_d(0, L_SIZE), 0.3).verb(3, 0.2, 0.4);
    });
    add("Tape Piano", "Keys", &|b| {
        b.src(0, PIANO).a(L_POS, 0.0).a(L_SCAN, 1.0).a(L_SIZE, 150.0).a(L_DENSITY, 20.0).a(L_FINE, -8.0).env(0, 2.0, 2000.0, 0.1, 800.0);
        b.lfo(0, wv(W::Sine), 0.6).m(S_LFO, layer_d(0, L_FINE), 0.05).filter(0, 5000.0, 0.0).verb(3, 0.25, 0.5);
    });

    // ---------------- Plucks
    add("Grain Pluck", "Plucks", &|b| {
        b.src(0, GUITAR).a(L_POS, 0.0).a(L_SCAN, 1.0).a(L_SIZE, 50.0).a(L_DENSITY, 60.0).env(0, 1.0, 500.0, 0.0, 300.0);
        b.filter(1, 6000.0, 0.3).env(1, 1.0, 250.0, 0.0, 200.0).m(S_ENV + 1, F_CUTOFF, 0.3).m(S_ENV + 1, F_CUTOFF, 0.0).delay("1/8", 0.2, 0.35);
    });
    add("Kalimba Spark", "Plucks", &|b| {
        b.src(0, KALIMBA).a(L_POS, 0.0).a(L_SCAN, 1.0).a(L_SIZE, 40.0).a(L_DENSITY, 70.0).a(L_PRAND, 0.05).env(0, 1.0, 700.0, 0.0, 400.0).verb(7, 0.25, 0.6);
    });
    add("Harp Rain", "Plucks", &|b| {
        b.src(0, HARP).a(L_SIZE, 60.0).a(L_DENSITY, 40.0).a(L_SPRAY, 0.4).a(L_STEREO, 1.0).env(0, 1.0, 900.0, 0.0, 700.0).delay("1/16", 0.25, 0.5).verb(0, 0.3, 0.7);
    });
    add("Formant Pluck", "Plucks", &|b| {
        b.src(0, VOWEL).layer(0, |l| l.mode = 1).a(L_SIZE, 4.0).env(0, 1.0, 400.0, 0.0, 250.0);
        b.env(1, 1.0, 300.0, 0.0, 200.0).m(S_ENV + 1, layer_d(0, L_POS), 0.4).m(S_ENV + 1, layer_d(0, L_PITCH), 0.2);
    });
    add("Bell Spray", "Plucks", &|b| {
        b.src(0, BELLS).a(L_POS, 0.0).a(L_SIZE, 30.0).a(L_DENSITY, 80.0).a(L_SPRAY, 0.15).a(L_PRAND, 0.2).env(0, 1.0, 600.0, 0.0, 600.0).delay("1/8.", 0.3, 0.5).verb(9, 0.3, 0.7);
    });
    add("Muted Box", "Plucks", &|b| {
        b.src(0, MUSICBOX).a(L_POS, 0.0).a(L_SCAN, 1.0).a(L_SIZE, 25.0).a(L_DENSITY, 90.0).env(0, 1.0, 250.0, 0.0, 150.0).filter(1, 2500.0, 0.2);
    });

    // ---------------- Bass
    add("Grain Sub", "Bass", &|b| {
        b.src(0, SUB).layer(0, |l| l.mode = 1).a(L_SIZE, 12.0).a(L_STEREO, 0.0).env(0, 2.0, 400.0, 0.9, 150.0).mono(true, 40.0);
        b.filter(1, 900.0, 0.2).verb(2, 0.0, 0.3);
    });
    add("Wobble Grains", "Bass", &|b| {
        b.src(0, SAW).layer(0, |l| l.mode = 1).a(L_SIZE, 8.0).a(L_STEREO, 0.0).env(0, 2.0, 300.0, 0.9, 120.0).mono(true, 30.0);
        b.filter(1, 400.0, 0.5).lfo_sync(0, wv(W::Sine), "1/8").m(S_LFO, F_CUTOFF, 0.4).k(X_RMIX, 0.0);
    });
    add("Formant Bass", "Bass", &|b| {
        b.src(0, FORMANT).layer(0, |l| l.mode = 1).a(L_SIZE, 5.0).a(L_STEREO, 0.0).env(0, 1.0, 400.0, 0.8, 150.0).mono(false, 0.0);
        b.lfo_sync(0, ramp(0.0, 1.0, 0.0), "1/4").m(S_LFO, layer_d(0, L_POS), 0.3).k(X_RMIX, 0.0);
    });
    add("Dark Reese", "Bass", &|b| {
        b.src(0, SAW).layer(0, |l| {
            l.mode = 1;
            l.streams = 2;
        });
        b.a(L_SIZE, 10.0).a(L_DETUNE, 20.0).a(L_STEREO, 0.3).env(0, 3.0, 400.0, 0.9, 200.0).filter(1, 700.0, 0.25).mono(true, 60.0).k(X_RMIX, 0.0);
    });
    add("Pluck Bass", "Bass", &|b| {
        b.src(0, GUITAR).layer(0, |l| l.mode = 1).a(L_SIZE, 9.0).a(L_STEREO, 0.0).env(0, 1.0, 350.0, 0.2, 150.0);
        b.filter(1, 1500.0, 0.3).env(1, 1.0, 200.0, 0.0, 100.0).m(S_ENV + 1, F_CUTOFF, 0.35).k(X_RMIX, 0.0);
    });

    // ---------------- Leads
    add("Vowel Lead", "Leads", &|b| {
        b.src(0, VOWEL).layer(0, |l| l.mode = 1).a(L_SIZE, 6.0).env(0, 5.0, 300.0, 0.9, 300.0).mono(true, 80.0);
        b.lfo(0, wv(W::Sine), 5.0).m(S_WHEEL, layer_d(0, L_FINE), 0.0).m(S_LFO, layer_d(0, L_POS), 0.08).delay("1/8.", 0.2, 0.4);
    });
    add("Granular Flute", "Leads", &|b| {
        b.src(0, FLUTE).a(L_SIZE, 90.0).a(L_DENSITY, 45.0).a(L_SCAN, 0.6).env(0, 40.0, 300.0, 0.9, 400.0).mono(true, 60.0).verb(5, 0.3, 0.6);
        b.lfo(0, wv(W::Sine), 5.5).m(S_WHEEL, layer_d(0, L_FINE), 0.1);
    });
    add("Pulsar Lead", "Leads", &|b| {
        b.src(0, SQUARE).layer(0, |l| l.mode = 1).a(L_SIZE, 2.5).a(L_PITCH, 12.0).env(0, 2.0, 400.0, 0.8, 300.0);
        b.lfo(0, wv(W::Triangle), 0.5).m(S_LFO, layer_d(0, L_SIZE), 0.2).delay("1/8", 0.2, 0.4);
    });
    add("Brass Cloud Lead", "Leads", &|b| {
        b.src(0, BRASS).a(L_POS, 0.8).a(L_SIZE, 70.0).a(L_DENSITY, 60.0).a(L_SPRAY, 0.02).env(0, 20.0, 400.0, 0.9, 300.0).mono(true, 50.0);
        b.filter(1, 2500.0, 0.3).m(S_VEL, F_CUTOFF, 0.3).verb(5, 0.2, 0.5);
    });

    // ---------------- Voices
    add("Angel Choir", "Voices", &|b| {
        b.src(0, CHOIR).a(L_SIZE, 300.0).a(L_DENSITY, 28.0).a(L_SPRAY, 0.1).a(L_PITCH, 12.0).env(0, 1200.0, 800.0, 0.85, 2500.0);
        b.src(1, CHOIR).b(L_SIZE, 300.0).b(L_LEVEL, 0.7).verb(7, 0.5, 0.8);
    });
    add("Talking Grains", "Voices", &|b| {
        b.src(0, VOWEL).a(L_SIZE, 150.0).a(L_DENSITY, 25.0).a(L_SCAN, 1.0).env(0, 50.0, 500.0, 0.9, 500.0).verb(3, 0.25, 0.5);
    });
    add("Vowel Morph", "Voices", &|b| {
        b.src(0, VOWEL).layer(0, |l| l.mode = 1).a(L_SIZE, 7.0).env(0, 100.0, 500.0, 0.9, 800.0);
        b.lfo(0, ramp(0.0, 1.0, 0.0), 0.25).m(S_LFO, layer_d(0, L_POS), 0.5).verb(0, 0.3, 0.6);
    });
    add("Whisper Pad", "Voices", &|b| {
        b.src(0, WHISPER).a(L_SIZE, 400.0).a(L_DENSITY, 15.0).a(L_SPRAY, 0.5).env(0, 1500.0, 1000.0, 1.0, 3000.0);
        b.src(1, CHOIR).b(L_LEVEL, 0.5).b(L_SIZE, 300.0).verb(8, 0.45, 0.8);
    });
    add("Robot Monks", "Voices", &|b| {
        b.src(0, CHOIR).layer(0, |l| l.mode = 1).a(L_SIZE, 3.0).a(L_PITCH, -5.0).env(0, 200.0, 500.0, 0.9, 1000.0).verb(7, 0.4, 0.75);
    });

    // ---------------- Rhythmic
    add("Gated Grains 1/16", "Rhythmic", &|b| {
        b.src(0, STRINGS).a(L_SIZE, 200.0).a(L_DENSITY, 30.0).env(0, 5.0, 500.0, 1.0, 400.0);
        b.lfo_sync(0, steps(9, 16), "1 bar").m(S_LFO, layer_d(0, L_LEVEL), 0.5).delay("1/8", 0.2, 0.3);
    });
    add("Drum Shredder", "Rhythmic", &|b| {
        b.src(0, DRUMS).a(L_SIZE, 60.0).a(L_DENSITY, 16.0).a(L_JITTER, 0.0).a(L_SCAN, 1.0).a(L_REVERSE, 0.2).env(0, 1.0, 500.0, 1.0, 300.0);
        b.lfo_sync(0, steps(3, 8), "1/2").m(S_LFO, layer_d(0, L_POS), 0.3).k(X_RMIX, 0.1);
    });
    add("Stutter Pad", "Rhythmic", &|b| {
        b.src(0, SAW).a(L_SIZE, 100.0).a(L_DENSITY, 8.0).a(L_JITTER, 0.0).env(0, 5.0, 500.0, 1.0, 600.0);
        b.lfo_sync(0, wv(W::SawDown), "1/8").m(S_LFO, layer_d(0, L_SIZE), 0.3).verb(3, 0.3, 0.5).delay("1/16", 0.2, 0.3);
    });
    add("Arp Particles", "Rhythmic", &|b| {
        b.src(0, SQUARE).a(L_SIZE, 50.0).a(L_DENSITY, 8.0).a(L_JITTER, 0.0).a(L_SPRAY, 0.3).env(0, 2.0, 500.0, 1.0, 500.0);
        b.lfo_sync(0, steps(17, 8), "1 bar").m(S_LFO, layer_d(0, L_PITCH), 0.25).delay("1/8.", 0.3, 0.5);
    });
    add("Pulse Choir", "Rhythmic", &|b| {
        b.src(0, CHOIR).a(L_SIZE, 120.0).a(L_DENSITY, 4.0).a(L_JITTER, 0.0).env(0, 5.0, 500.0, 1.0, 900.0).verb(0, 0.35, 0.7).delay("1/4", 0.2, 0.4);
    });

    // ---------------- FX
    add("Rising Dust", "FX", &|b| {
        b.src(0, GLITCH).a(L_SIZE, 30.0).a(L_DENSITY, 20.0).a(L_SPRAY, 1.0).env(0, 4000.0, 500.0, 1.0, 2000.0);
        b.env(1, 6000.0, 100.0, 1.0, 1000.0).m(S_ENV + 1, layer_d(0, L_DENSITY), 0.6).m(S_ENV + 1, layer_d(0, L_PITCH), 0.4).verb(9, 0.4, 0.8);
    });
    add("Reverse Swell", "FX", &|b| {
        b.src(0, PIANO).a(L_SIZE, 500.0).a(L_DENSITY, 10.0).a(L_REVERSE, 1.0).a(L_SPRAY, 0.2).env(0, 2000.0, 500.0, 1.0, 1000.0);
        b.layer(0, |l| l.window = 5);
        b.verb(8, 0.45, 0.8);
    });
    add("Alien Radio", "FX", &|b| {
        b.src(0, FORMANT).a(L_SIZE, 40.0).a(L_DENSITY, 60.0).a(L_PRAND, 12.0).a(L_SPRAY, 1.0).filter(4, 1500.0, 0.7);
        b.lfo(0, steps(77, 16), 6.0).m(S_LFO, F_CUTOFF, 0.4).delay("1/16", 0.3, 0.6);
    });
    add("Downer", "FX", &|b| {
        b.src(0, SAW).a(L_SIZE, 120.0).a(L_DENSITY, 40.0).env(0, 10.0, 500.0, 1.0, 800.0);
        b.env(1, 1.0, 2500.0, 0.0, 500.0).m(S_ENV + 1, layer_d(0, L_PITCH), 0.5).verb(0, 0.35, 0.7);
    });
    add("Glitch Storm", "FX", &|b| {
        b.src(0, DRUMS).a(L_SIZE, 20.0).a(L_DENSITY, 120.0).a(L_SPRAY, 1.0).a(L_REVERSE, 0.5).a(L_PRAND, 5.0);
        b.src(1, GLITCH).b(L_LEVEL, 0.5).b(L_SIZE, 10.0).b(L_DENSITY, 80.0).b(L_SPRAY, 1.0);
        b.lfo(0, steps(4, 8), 8.0).m(S_LFO, layer_d(0, L_SIZE), 0.3).delay("1/32", 0.25, 0.6);
    });
    add("Space Drone", "FX", &|b| {
        b.src(0, METAL).a(L_SIZE, 900.0).a(L_DENSITY, 6.0).a(L_PITCH, -12.0).a(L_SCAN, 0.05).env(0, 3000.0, 1000.0, 1.0, 5000.0);
        b.src(1, WIND).b(L_LEVEL, 0.5).b(L_SIZE, 700.0).verb(8, 0.6, 0.9);
    });
    out
}

// ------------------------------------------------------------------------------------------
// Capture / apply
// ------------------------------------------------------------------------------------------

pub fn capture(p: &GrainParams, name: &str, category: &str) -> PresetData {
    let mut st = p.state.read().clone();
    st.preset = name.into();
    PresetData { name: name.into(), category: category.into(), author: "User".into(), params: (0..N_DEST).map(|d| (param_id(d), p.knob(d).unmodulated_normalized_value())).collect(), state: st }
}

pub fn apply(setter: &ParamSetter, p: &GrainParams, d: &PresetData) {
    {
        let mut st = p.state.write();
        *st = d.state.clone();
        st.preset = d.name.clone();
        st.sanitize();
    }
    for k in 0..N_DEST {
        let prm = p.knob(k);
        let v = d.params.get(&param_id(k)).copied().unwrap_or_else(|| prm.default_normalized_value()).clamp(0.0, 1.0);
        if (prm.unmodulated_normalized_value() - v).abs() > 1e-7 {
            setter.begin_set_parameter(prm);
            setter.set_parameter_normalized(prm, v);
            setter.end_set_parameter(prm);
        }
    }
}

// ------------------------------------------------------------------------------------------
// Files
// ------------------------------------------------------------------------------------------

pub fn dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("MaxoGranular").join("Presets"))
}

pub fn file_name(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' }).collect();
    let s = s.trim();
    format!("{}.json", if s.is_empty() { "Preset" } else { s })
}

pub fn list_user() -> Vec<PresetData> {
    let Some(d) = dir() else { return vec![] };
    let Ok(rd) = std::fs::read_dir(&d) else { return vec![] };
    let mut out: Vec<PresetData> = rd
        .flatten()
        .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|s| serde_json::from_str::<PresetData>(&s).ok())
        .map(|mut p| {
            p.author = "User".into();
            p.state.sanitize();
            p
        })
        .collect();
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

pub fn save_user(d: &PresetData) -> Result<PathBuf, String> {
    let dir = dir().ok_or("no data folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(file_name(&d.name));
    std::fs::write(&path, serde_json::to_string_pretty(d).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn delete_user(name: &str) -> bool {
    dir().map(|d| std::fs::remove_file(d.join(file_name(name))).is_ok()).unwrap_or(false)
}

pub fn load_favorites() -> Vec<String> {
    dir().and_then(|d| std::fs::read_to_string(d.join("favorites.json")).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save_favorites(f: &[String]) {
    if let Some(d) = dir() {
        let _ = std::fs::create_dir_all(&d);
        let _ = std::fs::write(d.join("favorites.json"), serde_json::to_string(f).unwrap_or_default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_presets_are_valid() {
        let all = factory();
        assert!(all.len() >= 50, "{}", all.len());
        let mut names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), all.len());
        let p = GrainParams::default();
        let ids: Vec<String> = p.param_map().into_iter().map(|(i, _, _)| i).collect();
        for d in &all {
            assert!(CATEGORIES.contains(&d.category.as_str()), "{}", d.category);
            assert!(d.params.keys().all(|k| ids.contains(k)), "{}", d.name);
            let s = serde_json::to_string(d).unwrap();
            let back: PresetData = serde_json::from_str(&s).unwrap();
            assert_eq!(&back, d);
        }
        for c in CATEGORIES {
            assert!(all.iter().any(|d| d.category == c), "{c}");
        }
    }
}

//! Grain sources: built-in sounds made by code (so every preset works everywhere) and the
//! user's own WAV files. A source is stereo, at its own sample rate, with a waveform overview
//! for the editor.

use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const SR: f32 = 48000.0;
/// waveform overview columns
pub const OVERVIEW: usize = 600;

/// Where a layer's sound comes from.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum SourceRef {
    Builtin(u16),
    /// path relative to the Samples folder, or absolute
    File(String),
}

impl Default for SourceRef {
    fn default() -> Self {
        SourceRef::Builtin(0)
    }
}

pub struct Source {
    pub name: String,
    pub data: Vec<[f32; 2]>,
    pub sr: f32,
    /// (min, max) of both channels per column
    pub overview: Vec<(f32, f32)>,
}

impl Source {
    pub fn new(name: String, mut data: Vec<[f32; 2]>, sr: f32) -> Self {
        if data.len() < 64 {
            data.resize(64, [0.0; 2]);
        }
        // normalise to -1 dBFS peak (quiet files are as usable as loud ones)
        let pk = data.iter().map(|s| s[0].abs().max(s[1].abs())).fold(0.0f32, f32::max);
        if pk > 1e-6 {
            let g = 0.89 / pk;
            for s in data.iter_mut() {
                s[0] *= g;
                s[1] *= g;
            }
        }
        let n = data.len();
        let overview = (0..OVERVIEW)
            .map(|c| {
                let a = c * n / OVERVIEW;
                let b = ((c + 1) * n / OVERVIEW).max(a + 1).min(n);
                let mut lo = 0.0f32;
                let mut hi = 0.0f32;
                for s in &data[a..b] {
                    let v = 0.5 * (s[0] + s[1]);
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
                (lo, hi)
            })
            .collect();
        Self { name, data, sr, overview }
    }

    pub fn seconds(&self) -> f32 {
        self.data.len() as f32 / self.sr
    }
}

// ------------------------------------------------------------------------------------------
// Built-in sounds
// ------------------------------------------------------------------------------------------

/// (category, name)
pub const BUILTIN: [(&str, &str); 24] = [
    ("Voices", "Choir Aah"),
    ("Voices", "Vowel Chop"),
    ("Voices", "Whisper"),
    ("Keys", "Glass Bells"),
    ("Keys", "Music Box"),
    ("Keys", "Felt Piano"),
    ("Keys", "Organ"),
    ("Keys", "FM Bell"),
    ("Strings", "Bowed Strings"),
    ("Strings", "Plucked Guitar"),
    ("Strings", "Harp Gliss"),
    ("Strings", "Kalimba"),
    ("Synth", "Supersaw Pad"),
    ("Synth", "Square Arp"),
    ("Synth", "Sub Wobble"),
    ("Synth", "Formant Sweep"),
    ("Winds", "Breathy Flute"),
    ("Winds", "Brass Swell"),
    ("Texture", "Rain"),
    ("Texture", "Wind"),
    ("Texture", "Metal Scrape"),
    ("Texture", "Vinyl Crackle"),
    ("Rhythm", "Drum Loop"),
    ("Rhythm", "Glitch Clicks"),
];

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn mtof(n: f32) -> f32 {
    440.0 * 2f32.powf((n - 69.0) / 12.0)
}

/// Band-limited-ish saw (a few harmonics fewer near Nyquist).
fn saw(ph: f32, f: f32) -> f32 {
    let n = ((SR * 0.45 / f.max(1.0)) as usize).clamp(1, 60);
    let mut v = 0.0;
    for k in 1..=n {
        v += (TAU * ph * k as f32).sin() / k as f32;
    }
    v * 0.6
}

/// Two-pole resonator (formants, bodies).
struct Reso {
    a1: f32,
    a2: f32,
    g: f32,
    y1: f32,
    y2: f32,
}
impl Reso {
    fn new(f: f32, bw: f32) -> Self {
        let r = (-PI * bw / SR).exp();
        let a1 = 2.0 * r * (TAU * f / SR).cos();
        Self { a1, a2: -r * r, g: 1.0 - r, y1: 0.0, y2: 0.0 }
    }
    fn set(&mut self, f: f32, bw: f32) {
        let r = (-PI * bw / SR).exp();
        self.a1 = 2.0 * r * (TAU * f / SR).cos();
        self.a2 = -r * r;
        self.g = 1.0 - r;
    }
    fn run(&mut self, x: f32) -> f32 {
        let y = self.g * x + self.a1 * self.y1 + self.a2 * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Vowel formants (F1, F2, F3) for a, e, i, o, u.
const VOWELS: [[f32; 3]; 5] = [[800.0, 1150.0, 2900.0], [400.0, 1600.0, 2700.0], [280.0, 2250.0, 2900.0], [450.0, 800.0, 2830.0], [325.0, 700.0, 2530.0]];

fn stereo_from(l: Vec<f32>, r: Vec<f32>) -> Vec<[f32; 2]> {
    l.into_iter().zip(r).map(|(a, b)| [a, b]).collect()
}

/// A built-in sound (deterministic).
pub fn builtin(idx: usize) -> Source {
    let idx = idx.min(BUILTIN.len() - 1);
    let name = BUILTIN[idx].1.to_string();
    let secs = 4.0;
    let n = (SR * secs) as usize;
    let mut rng = Rng(0x9e37_79b9 ^ (idx as u32 * 7919));
    let t = |i: usize| i as f32 / SR;
    let data: Vec<[f32; 2]> = match idx {
        // ---- voices
        0 | 1 => {
            // a chord of voices through formant filters; Vowel Chop changes vowel in steps
            let notes = if idx == 0 { vec![57.0, 60.0, 64.0, 69.0] } else { vec![60.0] };
            let mut out = vec![[0.0f32; 2]; n];
            for (v, &nn) in notes.iter().enumerate() {
                let f0 = mtof(nn);
                let mut forms: Vec<Reso> = VOWELS[0].iter().map(|&f| Reso::new(f, 90.0)).collect();
                let mut ph = v as f32 * 0.17;
                for (i, o) in out.iter_mut().enumerate() {
                    let tt = t(i);
                    let vib = 1.0 + 0.006 * (TAU * (5.1 + v as f32 * 0.3) * tt).sin();
                    ph = (ph + f0 * vib / SR).fract();
                    if i % 64 == 0 {
                        let (a, b, k) = if idx == 0 {
                            (0, 0, 0.0)
                        } else {
                            let seg = tt * 4.0;
                            let a = (seg as usize) % 5;
                            (a, (a + 1) % 5, (seg.fract() * 6.0 - 5.0).clamp(0.0, 1.0))
                        };
                        for (j, r) in forms.iter_mut().enumerate() {
                            let f = VOWELS[a][j] + (VOWELS[b][j] - VOWELS[a][j]) * k;
                            r.set(f, 70.0 + 40.0 * j as f32);
                        }
                    }
                    let src = saw(ph, f0) + rng.next() * 0.03;
                    let y: f32 = forms.iter_mut().enumerate().map(|(j, r)| r.run(src) * [1.0, 0.6, 0.3][j]).sum();
                    let env = (tt * 3.0).min(1.0) * if idx == 1 { 0.6 + 0.4 * (1.0 - (tt * 4.0).fract()).powf(0.3) } else { 1.0 };
                    let pan = if v % 2 == 0 { 0.4 } else { -0.4 };
                    o[0] += y * env * (1.0 - pan * 0.5);
                    o[1] += y * env * (1.0 + pan * 0.5);
                }
            }
            out
        }
        2 => {
            // whisper: noise through moving formants
            let mut forms: Vec<Reso> = VOWELS[0].iter().map(|&f| Reso::new(f, 150.0)).collect();
            let mut forms_r: Vec<Reso> = VOWELS[0].iter().map(|&f| Reso::new(f, 150.0)).collect();
            (0..n)
                .map(|i| {
                    let tt = t(i);
                    if i % 64 == 0 {
                        let seg = tt * 2.5;
                        let a = (seg as usize) % 5;
                        let b = (a + 2) % 5;
                        let k = seg.fract();
                        for j in 0..3 {
                            let f = VOWELS[a][j] + (VOWELS[b][j] - VOWELS[a][j]) * k;
                            forms[j].set(f * 1.1, 160.0);
                            forms_r[j].set(f * 1.08, 170.0);
                        }
                    }
                    let env = 0.5 + 0.5 * (TAU * 1.25 * tt).sin().abs();
                    let (nl, nr) = (rng.next(), rng.next());
                    let l: f32 = forms.iter_mut().map(|r| r.run(nl)).sum::<f32>() * env;
                    let r: f32 = forms_r.iter_mut().map(|r| r.run(nr)).sum::<f32>() * env;
                    [l, r]
                })
                .collect()
        }
        // ---- keys
        3 | 4 | 5 | 7 | 11 => {
            // struck notes: (note list, partial ratios, decays)
            let (notes, step): (Vec<f32>, f32) = match idx {
                3 => (vec![72.0, 76.0, 79.0, 84.0, 83.0, 79.0, 76.0, 74.0], 0.5),
                4 => (vec![79.0, 84.0, 88.0, 86.0, 84.0, 91.0, 88.0, 84.0, 81.0, 79.0, 76.0, 79.0], 0.33),
                5 => (vec![48.0, 55.0, 60.0, 64.0, 67.0, 64.0, 60.0, 55.0], 0.5),
                7 => (vec![60.0, 67.0, 63.0, 70.0], 1.0),
                _ => (vec![62.0, 65.0, 69.0, 72.0, 74.0, 72.0, 69.0, 65.0], 0.5),
            };
            let partials: Vec<(f32, f32, f32)> = match idx {
                3 => vec![(1.0, 1.0, 1.5), (2.76, 0.5, 1.0), (5.4, 0.3, 0.6), (8.93, 0.15, 0.4)],
                4 => vec![(1.0, 1.0, 1.2), (3.0, 0.25, 0.5), (5.0, 0.1, 0.3)],
                5 => vec![(1.0, 1.0, 1.8), (2.0, 0.5, 1.2), (3.0, 0.25, 0.8), (4.0, 0.12, 0.6), (5.0, 0.06, 0.4)],
                7 => vec![(1.0, 1.0, 2.5), (3.5, 0.6, 1.0), (7.1, 0.25, 0.4)],
                _ => vec![(1.0, 1.0, 1.0), (4.1, 0.4, 0.3), (8.7, 0.15, 0.15)],
            };
            let mut l = vec![0.0f32; n];
            let mut r = vec![0.0f32; n];
            for (k, &nn) in notes.iter().enumerate() {
                let start = (k as f32 * step * SR) as usize;
                let f0 = mtof(nn);
                let pan = ((k as f32 * 0.7).sin()) * 0.5;
                for (pi, &(ratio, amp, dec)) in partials.iter().enumerate() {
                    let f = f0 * ratio;
                    if f > SR * 0.45 {
                        continue;
                    }
                    let fm = if idx == 7 { 2.0 } else { 0.0 };
                    for i in start..n {
                        let tt = (i - start) as f32 / SR;
                        let e = (-tt / dec).exp() * (tt * 400.0).min(1.0);
                        if e < 1e-4 && tt > 0.05 {
                            break;
                        }
                        let ph = TAU * f * tt + fm * e * (TAU * f0 * 1.4 * tt).sin() + pi as f32;
                        let v = ph.sin() * amp * e * 0.3;
                        l[i] += v * (1.0 - pan);
                        r[i] += v * (1.0 + pan);
                    }
                }
                if idx == 5 {
                    // hammer felt noise
                    for i in start..(start + 2400).min(n) {
                        let tt = (i - start) as f32 / SR;
                        let v = rng.next() * (-tt * 80.0).exp() * 0.05;
                        l[i] += v;
                        r[i] += v;
                    }
                }
            }
            stereo_from(l, r)
        }
        6 => {
            // organ: drawbars, slow leslie
            let chord = [48.0, 55.0, 60.0, 64.0];
            let bars = [(0.5, 0.6), (1.0, 1.0), (1.5, 0.5), (2.0, 0.7), (3.0, 0.4), (4.0, 0.3), (6.0, 0.15)];
            (0..n)
                .map(|i| {
                    let tt = t(i);
                    let mut v = 0.0;
                    for &nn in &chord {
                        for &(h, a) in &bars {
                            v += (TAU * mtof(nn) * h * tt).sin() * a;
                        }
                    }
                    let les = (TAU * 0.8 * tt).sin();
                    let v = v * 0.05 * (tt * 20.0).min(1.0);
                    [v * (1.0 + 0.3 * les), v * (1.0 - 0.3 * les)]
                })
                .collect()
        }
        // ---- strings
        8 => {
            let chord = [45.0, 52.0, 57.0, 61.0, 64.0];
            let mut out = vec![[0.0f32; 2]; n];
            for (v, &nn) in chord.iter().enumerate() {
                for d in 0..3 {
                    let f = mtof(nn) * (1.0 + (d as f32 - 1.0) * 0.004);
                    let mut ph = (v * 3 + d) as f32 * 0.13;
                    let mut lp = 0.0f32;
                    let pan = ((v * 3 + d) as f32 * 1.7).sin() * 0.6;
                    for (i, o) in out.iter_mut().enumerate() {
                        let tt = t(i);
                        ph = (ph + f * (1.0 + 0.004 * (TAU * (5.5 + d as f32 * 0.4) * tt).sin()) / SR).fract();
                        let s = 2.0 * ph - 1.0;
                        lp += (s - lp) * 0.08;
                        let env = (tt * 1.5).min(1.0);
                        let y = lp * env * 0.08;
                        o[0] += y * (1.0 - pan);
                        o[1] += y * (1.0 + pan);
                    }
                }
            }
            out
        }
        9 | 10 => {
            // Karplus-Strong
            let notes: Vec<f32> = if idx == 9 { vec![40.0, 47.0, 52.0, 56.0, 59.0, 64.0, 59.0, 56.0] } else { (0..16).map(|k| [60.0, 62.0, 64.0, 67.0, 69.0][k % 5] + 12.0 * (k / 5) as f32).collect() };
            let step = if idx == 9 { 0.45 } else { 0.12 };
            let mut l = vec![0.0f32; n];
            let mut r = vec![0.0f32; n];
            for (k, &nn) in notes.iter().enumerate() {
                let start = (k as f32 * step * SR) as usize;
                let per = (SR / mtof(nn)) as usize;
                let mut buf: Vec<f32> = (0..per.max(2)).map(|_| rng.next()).collect();
                let pan = (k as f32 * 0.9).sin() * 0.5;
                let mut p = 0;
                for i in start..n {
                    let a = buf[p];
                    let b = buf[(p + 1) % buf.len()];
                    let y = 0.5 * (a + b) * 0.996;
                    buf[p] = y;
                    p = (p + 1) % buf.len();
                    l[i] += a * 0.3 * (1.0 - pan);
                    r[i] += a * 0.3 * (1.0 + pan);
                }
            }
            stereo_from(l, r)
        }
        // ---- synth
        12 => {
            let chord = [48.0, 55.0, 60.0, 63.0, 67.0];
            let mut out = vec![[0.0f32; 2]; n];
            for (v, &nn) in chord.iter().enumerate() {
                for d in 0..5 {
                    let f = mtof(nn) * 2f32.powf((d as f32 - 2.0) * 0.12 / 12.0);
                    let mut ph = (v * 5 + d) as f32 * 0.21;
                    let pan = (d as f32 - 2.0) / 2.0 * 0.8;
                    for (i, o) in out.iter_mut().enumerate() {
                        ph = (ph + f / SR).fract();
                        let y = (2.0 * ph - 1.0) * 0.03 * (t(i) * 2.0).min(1.0);
                        o[0] += y * (1.0 - pan);
                        o[1] += y * (1.0 + pan);
                    }
                }
            }
            // gentle low-pass
            let mut lp = [0.0f32; 2];
            for o in out.iter_mut() {
                for c in 0..2 {
                    lp[c] += (o[c] - lp[c]) * 0.25;
                    o[c] = lp[c];
                }
            }
            out
        }
        13 => {
            let pat = [57.0, 60.0, 64.0, 69.0, 72.0, 69.0, 64.0, 60.0];
            (0..n)
                .map(|i| {
                    let tt = t(i);
                    let k = (tt * 8.0) as usize;
                    let f = mtof(pat[k % 8]);
                    let ph = (f * tt).fract();
                    let e = (-(tt * 8.0).fract() * 4.0).exp();
                    let v = if ph < 0.5 { 0.25 } else { -0.25 } * e;
                    let pan = if k % 2 == 0 { 0.3 } else { -0.3 };
                    [v * (1.0 - pan), v * (1.0 + pan)]
                })
                .collect()
        }
        14 => {
            let mut lp = 0.0f32;
            (0..n)
                .map(|i| {
                    let tt = t(i);
                    let f = mtof(36.0);
                    let s = saw((f * tt).fract(), f);
                    let cut = 0.02 + 0.2 * (0.5 + 0.5 * (TAU * 2.0 * tt).sin());
                    lp += (s - lp) * cut;
                    let v = lp * 0.8 + (TAU * f * tt).sin() * 0.3;
                    [v, v]
                })
                .collect()
        }
        15 => {
            let mut forms: Vec<Reso> = VOWELS[0].iter().map(|&f| Reso::new(f, 80.0)).collect();
            (0..n)
                .map(|i| {
                    let tt = t(i);
                    let f0 = mtof(48.0);
                    if i % 32 == 0 {
                        let k = 0.5 + 0.5 * (TAU * tt / secs).sin();
                        let fs = [300.0 + 600.0 * k, 900.0 + 1500.0 * (1.0 - k), 2600.0];
                        for (j, r) in forms.iter_mut().enumerate() {
                            r.set(fs[j], 60.0);
                        }
                    }
                    let s = saw((f0 * tt).fract(), f0);
                    let y: f32 = forms.iter_mut().map(|r| r.run(s)).sum();
                    [y * 0.8, y * 0.8]
                })
                .collect()
        }
        // ---- winds
        16 => {
            let mut bp = Reso::new(mtof(72.0), 300.0);
            (0..n)
                .map(|i| {
                    let tt = t(i);
                    let f = mtof([72.0, 74.0, 76.0, 79.0][((tt * 1.0) as usize) % 4]) * (1.0 + 0.005 * (TAU * 5.0 * tt).sin());
                    let tone = (TAU * f * tt).sin() * 0.5 + (TAU * 2.0 * f * tt).sin() * 0.1;
                    let breath = bp.run(rng.next()) * 2.0;
                    let env = 0.3 + 0.7 * (tt * 3.0).min(1.0);
                    let v = (tone + breath) * env * 0.5;
                    [v, v * 0.95]
                })
                .collect()
        }
        17 => {
            let chord = [48.0, 52.0, 55.0, 60.0];
            let mut out = vec![[0.0f32; 2]; n];
            for (v, &nn) in chord.iter().enumerate() {
                let f = mtof(nn);
                let mut lp = 0.0f32;
                let pan = (v as f32 - 1.5) * 0.3;
                for (i, o) in out.iter_mut().enumerate() {
                    let tt = t(i);
                    let swell = (tt / 2.5).min(1.0).powf(1.5);
                    let s = saw((f * tt + v as f32 * 0.3).fract(), f);
                    lp += (s - lp) * (0.02 + 0.3 * swell);
                    let y = lp * swell * 0.25;
                    o[0] += y * (1.0 - pan);
                    o[1] += y * (1.0 + pan);
                }
            }
            out
        }
        // ---- textures
        18 => {
            let mut out = vec![[0.0f32; 2]; n];
            let mut lp = [0.0f32; 2];
            for o in out.iter_mut() {
                for c in 0..2 {
                    lp[c] += (rng.next() - lp[c]) * 0.3;
                    o[c] = (rng.next() - lp[c]) * 0.05;
                }
            }
            // drops
            for _ in 0..180 {
                let at = ((rng.next() * 0.5 + 0.5) * (n as f32 - 4000.0)) as usize;
                let f = 1500.0 + (rng.next() * 0.5 + 0.5) * 3000.0;
                let pan = rng.next() * 0.8;
                let a = 0.1 + 0.25 * (rng.next() * 0.5 + 0.5);
                for k in 0..3000 {
                    let tt = k as f32 / SR;
                    let v = (TAU * f * (1.0 + tt * 8.0) * tt).sin() * (-tt * 120.0).exp() * a;
                    out[at + k][0] += v * (1.0 - pan);
                    out[at + k][1] += v * (1.0 + pan);
                }
            }
            out
        }
        19 => {
            let mut b1 = Reso::new(600.0, 200.0);
            let mut b2 = Reso::new(650.0, 220.0);
            (0..n)
                .map(|i| {
                    let tt = t(i);
                    if i % 64 == 0 {
                        let f = 400.0 + 900.0 * (0.5 + 0.5 * (TAU * 0.3 * tt).sin()) + 300.0 * (TAU * 0.71 * tt).sin();
                        b1.set(f, 250.0);
                        b2.set(f * 1.15, 260.0);
                    }
                    let g = 0.6 + 0.4 * (TAU * 0.5 * tt).sin();
                    [b1.run(rng.next()) * g * 3.0, b2.run(rng.next()) * g * 3.0]
                })
                .collect()
        }
        20 => {
            let mut rs: Vec<Reso> = [523.0, 1270.0, 2210.0, 3480.0, 5120.0].iter().map(|&f| Reso::new(f, 8.0)).collect();
            (0..n)
                .map(|i| {
                    let tt = t(i);
                    let exc = rng.next() * (0.3 + 0.7 * (0.5 + 0.5 * (TAU * 1.7 * tt).sin()).powf(3.0)) * 0.3;
                    let v: f32 = rs.iter_mut().map(|r| r.run(exc)).sum();
                    [v, v * 0.9 + rng.next() * 0.002]
                })
                .collect()
        }
        21 => {
            let mut lp = 0.0f32;
            (0..n)
                .map(|_| {
                    lp += (rng.next() - lp) * 0.05;
                    let crack = if rng.next() > 0.9993 { rng.next() * 0.9 } else { 0.0 };
                    let v = lp * 0.08 + crack;
                    [v, v * 0.97]
                })
                .collect()
        }
        // ---- rhythm
        22 => {
            let beat = 0.25;
            (0..n)
                .map(|i| {
                    let tt = t(i);
                    let step = (tt / (beat / 2.0)) as usize;
                    let st = (tt / (beat / 2.0)).fract() * beat / 2.0;
                    let mut v = 0.0;
                    if step % 4 == 0 {
                        let f = 50.0 + 120.0 * (-st * 30.0).exp();
                        v += (TAU * f * st).sin() * (-st * 9.0).exp() * 0.9;
                    }
                    if step % 8 == 4 {
                        v += rng.next() * (-st * 25.0).exp() * 0.5 + (TAU * 190.0 * st).sin() * (-st * 20.0).exp() * 0.3;
                    }
                    let hat = rng.next() * (-st * 90.0).exp() * if step % 2 == 1 { 0.25 } else { 0.12 };
                    [v + hat * 0.8, v + hat * 1.2]
                })
                .collect()
        }
        _ => {
            let mut out = vec![[0.0f32; 2]; n];
            for _ in 0..120 {
                let at = ((rng.next() * 0.5 + 0.5) * (n as f32 - 3000.0)) as usize;
                let f = 200.0 * 2f32.powf((rng.next() * 0.5 + 0.5) * 6.0);
                let len = 100 + ((rng.next() * 0.5 + 0.5) * 2500.0) as usize;
                let pan = rng.next();
                for k in 0..len {
                    let v = if ((k as f32 * f / SR) as usize) % 2 == 0 { 0.3 } else { -0.3 } * (1.0 - k as f32 / len as f32);
                    out[at + k][0] += v * (1.0 - pan);
                    out[at + k][1] += v * (1.0 + pan);
                }
            }
            out
        }
    };
    Source::new(name, data, SR)
}

// ------------------------------------------------------------------------------------------
// User files
// ------------------------------------------------------------------------------------------

pub fn samples_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("MaxoGranular").join("Samples"))
}

/// WAV files under the samples folder (two folder levels), relative paths, sorted.
pub fn list_files() -> Vec<String> {
    let Some(root) = samples_dir() else { return vec![] };
    let mut out = Vec::new();
    fn walk(dir: &Path, root: &Path, depth: usize, out: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && depth < 2 {
                walk(&p, root, depth + 1, out);
            } else if p.extension().map(|x| x.eq_ignore_ascii_case("wav")).unwrap_or(false) {
                if let Ok(rel) = p.strip_prefix(root) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
            if out.len() >= 2000 {
                return;
            }
        }
    }
    walk(&root, &root, 0, &mut out);
    out.sort_by_key(|s| s.to_lowercase());
    out
}

/// Loads a WAV (any bit depth, mono or stereo; at most 60 s).
pub fn load_wav(path: &Path) -> Option<Source> {
    if std::fs::metadata(path).ok()?.len() > 200 << 20 {
        return None;
    }
    let mut r = hound::WavReader::open(path).ok()?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let max = (spec.sample_rate as usize) * 60 * ch;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().take(max).map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample.clamp(8, 32) - 1)) as f32;
            r.samples::<i32>().take(max).map(|s| s.unwrap_or(0) as f32 * scale).collect()
        }
    };
    let data: Vec<[f32; 2]> = samples.chunks(ch).map(|c| [c[0], if ch > 1 { c[1] } else { c[0] }]).collect();
    if data.is_empty() {
        return None;
    }
    let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    Some(Source::new(name, data, spec.sample_rate as f32))
}

/// Loads what a layer points at (built-in or file; a missing file falls back to the first sound).
pub fn load(r: &SourceRef) -> Arc<Source> {
    match r {
        SourceRef::Builtin(i) => Arc::new(builtin(*i as usize)),
        SourceRef::File(p) => {
            let path = Path::new(p);
            let full = if path.is_absolute() { path.to_path_buf() } else { samples_dir().map(|d| d.join(path)).unwrap_or_else(|| path.to_path_buf()) };
            match load_wav(&full) {
                Some(s) => Arc::new(s),
                None => {
                    let mut s = builtin(0);
                    s.name = format!("missing: {p}");
                    Arc::new(s)
                }
            }
        }
    }
}

pub fn display_name(r: &SourceRef) -> String {
    match r {
        SourceRef::Builtin(i) => BUILTIN.get(*i as usize).map(|b| b.1.to_string()).unwrap_or_default(),
        SourceRef::File(p) => Path::new(p).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| p.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_is_sound() {
        for i in 0..BUILTIN.len() {
            let s = builtin(i);
            assert!(s.data.len() > 10000, "{}", s.name);
            let fin = s.data.iter().all(|v| v[0].is_finite() && v[1].is_finite());
            let rms = (s.data.iter().map(|v| v[0] * v[0]).sum::<f32>() / s.data.len() as f32).sqrt();
            assert!(fin && rms > 0.01, "{} rms {rms}", s.name);
            let pk = s.data.iter().map(|v| v[0].abs().max(v[1].abs())).fold(0.0f32, f32::max);
            assert!(pk <= 0.9, "{}", s.name);
            assert_eq!(s.overview.len(), OVERVIEW);
        }
    }

    #[test]
    fn wav_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ng_wav_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.wav");
        let spec = hound::WavSpec { channels: 2, sample_rate: 44100, bits_per_sample: 24, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&p, spec).unwrap();
        for i in 0..44100 {
            let v = ((i as f32 * 0.05).sin() * 4_000_000.0) as i32;
            w.write_sample(v).unwrap();
            w.write_sample(-v).unwrap();
        }
        w.finalize().unwrap();
        let s = load_wav(&p).unwrap();
        assert_eq!(s.data.len(), 44100);
        assert_eq!(s.sr, 44100.0);
        assert!((s.data[10][0] + s.data[10][1]).abs() < 1e-5);
        let _ = std::fs::remove_dir_all(dir);
    }
}

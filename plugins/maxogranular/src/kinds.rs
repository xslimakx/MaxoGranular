//! Tables: every automatable / modulatable control (its range, unit, name), the modulation
//! sources, and the names of the discrete settings.


#[derive(Clone, Copy, PartialEq)]
pub enum Scale {
    Lin,
    Log,
    /// normalized^p
    Pow(f32),
}

#[derive(Clone, Copy, PartialEq)]
pub enum Unit {
    Db,
    Pct,
    Hz,
    Ms,
    St,
    Ratio,
    Deg,
    Ct,
    Speed,
    Hzs,
}

#[derive(Clone, Copy)]
pub struct KnobDef {
    pub name: &'static str,
    pub tip: &'static str,
    pub min: f32,
    pub max: f32,
    pub def: f32,
    pub scale: Scale,
    pub unit: Unit,
    pub bipolar: bool,
}

impl KnobDef {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(name: &'static str, tip: &'static str, min: f32, max: f32, def: f32, scale: Scale, unit: Unit, bipolar: bool) -> Self {
        Self { name, tip, min, max, def, scale, unit, bipolar }
    }
    pub fn plain(&self, n: f32) -> f32 {
        let n = n.clamp(0.0, 1.0);
        match self.scale {
            Scale::Lin => self.min + (self.max - self.min) * n,
            Scale::Log => self.min * (self.max / self.min).powf(n),
            Scale::Pow(p) => self.min + (self.max - self.min) * n.powf(p),
        }
    }
    pub fn norm(&self, v: f32) -> f32 {
        let v = v.clamp(self.min.min(self.max), self.max.max(self.min));
        match self.scale {
            Scale::Lin => (v - self.min) / (self.max - self.min),
            Scale::Log => (v / self.min).ln() / (self.max / self.min).ln(),
            Scale::Pow(p) => ((v - self.min) / (self.max - self.min)).max(0.0).powf(1.0 / p),
        }
        .clamp(0.0, 1.0)
    }
    pub fn default_norm(&self) -> f32 {
        self.norm(self.def)
    }
    pub fn fmt(&self, n: f32) -> String {
        fmt_unit(self.plain(n), self.unit)
    }
    pub fn parse(&self, s: &str) -> Option<f32> {
        let t = s.trim().to_lowercase();
        let t = t.trim_end_matches("db").trim_end_matches('%').trim_end_matches("hz").trim_end_matches("ms").trim_end_matches("st").trim_end_matches(":1").trim_end_matches('°').trim_end_matches("ct").trim_end_matches("/s").trim_end_matches('×').trim();
        let (t, mul) = if let Some(k) = t.strip_suffix('k') { (k.trim(), 1000.0) } else { (t, 1.0) };
        let v: f32 = t.parse().ok()?;
        let v = v * mul;
        Some(self.norm(if self.unit == Unit::Pct { v / 100.0 } else { v }))
    }
}

pub fn fmt_unit(v: f32, u: Unit) -> String {
    match u {
        Unit::Db => format!("{:.1} dB", v + 0.0),
        Unit::Pct => format!("{:.0} %", v * 100.0 + 0.0),
        Unit::Hz => {
            if v >= 1000.0 {
                format!("{:.1}k Hz", v / 1000.0)
            } else {
                format!("{:.0} Hz", v)
            }
        }
        Unit::Ms => {
            if v < 10.0 {
                format!("{v:.2} ms")
            } else if v < 100.0 {
                format!("{v:.1} ms")
            } else {
                format!("{v:.0} ms")
            }
        }
        Unit::St => format!("{:.1} st", v + 0.0),
        Unit::Deg => format!("{:.0}°", v + 0.0),
        Unit::Ct => format!("{:+.0} ct", v + 0.0),
        Unit::Speed => {
            if v.abs() < 0.005 {
                "frozen".into()
            } else {
                format!("{:+.2}×", v)
            }
        }
        Unit::Hzs => {
            if v < 10.0 {
                format!("{v:.2} /s")
            } else {
                format!("{v:.0} /s")
            }
        }
        Unit::Ratio => {
            if v >= 99.5 {
                "∞:1".into()
            } else {
                format!("{v:.1}:1")
            }
        }
    }
}


// ------------------------------------------------------------------------------------------
// The controls
// ------------------------------------------------------------------------------------------

/// Controls of a grain layer, in this order (layer B: + LAYER_N).
pub const LAYER_N: usize = 15;
pub const L_LEVEL: usize = 0;
pub const L_PAN: usize = 1;
pub const L_POS: usize = 2;
pub const L_SCAN: usize = 3;
pub const L_SPRAY: usize = 4;
pub const L_SIZE: usize = 5;
pub const L_DENSITY: usize = 6;
pub const L_PITCH: usize = 7;
pub const L_FINE: usize = 8;
pub const L_PRAND: usize = 9;
pub const L_SKEW: usize = 10;
pub const L_REVERSE: usize = 11;
pub const L_STEREO: usize = 12;
pub const L_DETUNE: usize = 13;
pub const L_JITTER: usize = 14;

const LAYER: [KnobDef; LAYER_N] = [
    KnobDef::new("Level", "Volume of the layer.", 0.0, 1.0, 0.8, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Pan", "Left / right.", -1.0, 1.0, 0.0, Scale::Lin, Unit::Pct, true),
    KnobDef::new("Position", "Where in the source the grains come from (drag it on the waveform too).", 0.0, 1.0, 0.3, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Scan", "Moves the position while the note plays: forwards, backwards or frozen (0).", -2.0, 2.0, 0.0, Scale::Lin, Unit::Speed, true),
    KnobDef::new("Spray", "Random spread of the position around it (shaded on the waveform).", 0.0, 1.0, 0.05, Scale::Pow(2.0), Unit::Pct, false),
    KnobDef::new("Size", "Length of a grain (Sync mode: short grains buzz at the note, long ones blur).", 1.0, 2000.0, 120.0, Scale::Log, Unit::Ms, false),
    KnobDef::new("Density", "Cloud: grains per second. Sync: grains overlap more or less.", 0.5, 200.0, 25.0, Scale::Log, Unit::Hzs, false),
    KnobDef::new("Pitch", "Transposition of the grains (Sync mode: the formant).", -24.0, 24.0, 0.0, Scale::Lin, Unit::St, true),
    KnobDef::new("Fine", "Fine tuning.", -100.0, 100.0, 0.0, Scale::Lin, Unit::Ct, true),
    KnobDef::new("Pitch rnd", "Random pitch of every grain.", 0.0, 12.0, 0.0, Scale::Pow(2.0), Unit::St, false),
    KnobDef::new("Skew", "Moves the peak of the grain window: early (percussive) or late (swelling).", -1.0, 1.0, 0.0, Scale::Lin, Unit::Pct, true),
    KnobDef::new("Reverse", "Chance that a grain plays backwards.", 0.0, 1.0, 0.0, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Stereo", "Random left / right placement of the grains.", 0.0, 1.0, 0.4, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Detune", "Spread of the streams (Streams > 1).", 0.0, 100.0, 12.0, Scale::Lin, Unit::Ct, false),
    KnobDef::new("Jitter", "Cloud: random timing of the grains (0: a steady pulse).", 0.0, 1.0, 0.5, Scale::Lin, Unit::Pct, false),
];

pub const ENV_N: usize = 5;
const ENV: [KnobDef; ENV_N] = [
    KnobDef::new("Attack", "Time to rise to full.", 0.1, 10000.0, 5.0, Scale::Log, Unit::Ms, false),
    KnobDef::new("Hold", "Time at full before the decay.", 0.0, 5000.0, 0.0, Scale::Pow(3.0), Unit::Ms, false),
    KnobDef::new("Decay", "Time to fall to the sustain level.", 1.0, 15000.0, 400.0, Scale::Log, Unit::Ms, false),
    KnobDef::new("Sustain", "Level while the key is held.", 0.0, 1.0, 0.7, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Release", "Time to fade out after the key is released.", 1.0, 20000.0, 600.0, Scale::Log, Unit::Ms, false),
];

pub const LFO_N: usize = 3;
const LFO: [KnobDef; LFO_N] = [
    KnobDef::new("Rate", "Speed (Hz), or a note length when synced to the tempo.", 0.02, 40.0, 1.0, Scale::Log, Unit::Hz, false),
    KnobDef::new("Smooth", "Rounds off the edges of the shape.", 0.0, 1.0, 0.0, Scale::Pow(2.0), Unit::Pct, false),
    KnobDef::new("Phase", "Start point of the shape.", 0.0, 360.0, 0.0, Scale::Lin, Unit::Deg, false),
];

const FILTER: [KnobDef; 5] = [
    KnobDef::new("Cutoff", "Cutoff frequency.", 20.0, 20000.0, 20000.0, Scale::Log, Unit::Hz, false),
    KnobDef::new("Resonance", "Emphasis at the cutoff.", 0.0, 1.0, 0.1, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Drive", "Saturation into the filter.", 0.0, 1.0, 0.0, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Key track", "How far the cutoff follows the note (100 %: one octave per octave).", 0.0, 1.0, 0.0, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Mix", "Dry / filtered.", 0.0, 1.0, 1.0, Scale::Lin, Unit::Pct, false),
];

const FX: [KnobDef; 10] = [
    KnobDef::new("Time", "Delay time (or a note length when synced).", 1.0, 2000.0, 375.0, Scale::Log, Unit::Ms, false),
    KnobDef::new("Feedback", "How many repeats.", 0.0, 0.95, 0.4, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Tone", "Dark to bright repeats.", 0.0, 1.0, 0.6, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Delay mix", "Level of the repeats.", 0.0, 1.0, 0.0, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Size", "Size of the space.", 0.5, 2.0, 1.0, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Decay", "Length of the tail.", 0.0, 1.0, 0.6, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Tone ", "Dark to bright tail.", 0.0, 1.0, 0.55, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Pre-delay", "Time before the tail starts.", 0.0, 250.0, 10.0, Scale::Pow(2.0), Unit::Ms, false),
    KnobDef::new("Width", "Stereo width of the tail.", 0.0, 2.0, 1.0, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Reverb mix", "Level of the tail.", 0.0, 1.0, 0.0, Scale::Lin, Unit::Pct, false),
];

const MISC: [KnobDef; 7] = [
    KnobDef::new("Glide", "Time the pitch slides from note to note.", 0.0, 2000.0, 0.0, Scale::Pow(3.0), Unit::Ms, false),
    KnobDef::new("Macro 1", "A modulation source you turn by hand (or automate).", 0.0, 1.0, 0.0, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Macro 2", "A modulation source you turn by hand (or automate).", 0.0, 1.0, 0.0, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Macro 3", "A modulation source you turn by hand (or automate).", 0.0, 1.0, 0.0, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Macro 4", "A modulation source you turn by hand (or automate).", 0.0, 1.0, 0.0, Scale::Lin, Unit::Pct, false),
    KnobDef::new("Volume", "Master volume.", -36.0, 6.0, -6.0, Scale::Lin, Unit::Db, true),
    KnobDef::new("Velocity", "How much the key velocity changes the volume.", 0.0, 1.0, 0.6, Scale::Lin, Unit::Pct, false),
];

/// Index of the first control of every group in the flat table.
pub const D_LAYER: usize = 0;
pub const D_ENV: usize = 2 * LAYER_N;
pub const D_LFO: usize = D_ENV + 3 * ENV_N;
pub const D_FILTER: usize = D_LFO + 4 * LFO_N;
pub const D_FX: usize = D_FILTER + 5;
pub const D_MISC: usize = D_FX + 10;
pub const N_DEST: usize = D_MISC + 7;

pub const F_CUTOFF: usize = D_FILTER;
pub const F_RES: usize = D_FILTER + 1;
pub const F_DRIVE: usize = D_FILTER + 2;
pub const F_KEY: usize = D_FILTER + 3;
pub const F_MIX: usize = D_FILTER + 4;
pub const X_TIME: usize = D_FX;
pub const X_FB: usize = D_FX + 1;
pub const X_TONE: usize = D_FX + 2;
pub const X_DMIX: usize = D_FX + 3;
pub const X_SIZE: usize = D_FX + 4;
pub const X_DECAY: usize = D_FX + 5;
pub const X_RTONE: usize = D_FX + 6;
pub const X_PRE: usize = D_FX + 7;
pub const X_WIDTH: usize = D_FX + 8;
pub const X_RMIX: usize = D_FX + 9;
pub const M_GLIDE: usize = D_MISC;
pub const M_MACRO: usize = D_MISC + 1;
pub const M_VOLUME: usize = D_MISC + 5;
pub const M_VEL: usize = D_MISC + 6;

pub fn layer_d(layer: usize, k: usize) -> usize {
    D_LAYER + layer * LAYER_N + k
}
pub fn env_d(env: usize, k: usize) -> usize {
    D_ENV + env * ENV_N + k
}
pub fn lfo_d(lfo: usize, k: usize) -> usize {
    D_LFO + lfo * LFO_N + k
}

/// The definition of control `d` of the flat table.
pub fn def(d: usize) -> &'static KnobDef {
    match d {
        _ if d < D_ENV => &LAYER[(d - D_LAYER) % LAYER_N],
        _ if d < D_LFO => &ENV[(d - D_ENV) % ENV_N],
        _ if d < D_FILTER => &LFO[(d - D_LFO) % LFO_N],
        _ if d < D_FX => &FILTER[d - D_FILTER],
        _ if d < D_MISC => &FX[d - D_FX],
        _ => &MISC[(d - D_MISC).min(MISC.len() - 1)],
    }
}

/// Full name (for the host and the matrix): "A Position", "Env 2 Attack", "LFO 1 Rate"...
pub fn dest_name(d: usize) -> String {
    let n = def(d).name.trim();
    match d {
        _ if d < D_ENV => format!("{} {}", ["A", "B"][(d - D_LAYER) / LAYER_N], n),
        _ if d < D_LFO => format!("Env {} {}", (d - D_ENV) / ENV_N + 1, n),
        _ if d < D_FILTER => format!("LFO {} {}", (d - D_LFO) / LFO_N + 1, n),
        _ if d < D_FX => format!("Filter {n}"),
        _ if d < D_FX + 4 => format!("Delay {}", n.trim_start_matches("Delay ")),
        _ if d < D_MISC => format!("Reverb {}", n.trim_start_matches("Reverb ")),
        _ => n.to_string(),
    }
}

/// Can this control be a modulation target (macros and the velocity amount cannot)?
pub fn modulatable(d: usize) -> bool {
    !(M_MACRO..M_MACRO + 4).contains(&d) && d != M_VEL
}

// ------------------------------------------------------------------------------------------
// Modulation sources
// ------------------------------------------------------------------------------------------

pub const N_SRC: usize = 16;
pub const SRC_NAMES: [&str; N_SRC] = ["ENV 1", "ENV 2", "ENV 3", "LFO 1", "LFO 2", "LFO 3", "LFO 4", "VELO", "NOTE", "WHEEL", "PRESS", "RAND", "MACRO 1", "MACRO 2", "MACRO 3", "MACRO 4"];
pub const SRC_TIPS: [&str; N_SRC] = [
    "Envelope 1 (also the volume of every note)",
    "Envelope 2",
    "Envelope 3",
    "LFO 1",
    "LFO 2",
    "LFO 3",
    "LFO 4",
    "Key velocity",
    "The note played (C3 = 0, ±5 octaves = ±1)",
    "Mod wheel (CC 1)",
    "Aftertouch / pressure",
    "A random value for every note",
    "Macro 1",
    "Macro 2",
    "Macro 3",
    "Macro 4",
];
pub const S_ENV: usize = 0;
pub const S_LFO: usize = 3;
pub const S_VEL: usize = 7;
pub const S_NOTE: usize = 8;
pub const S_WHEEL: usize = 9;
pub const S_PRESS: usize = 10;
pub const S_RAND: usize = 11;
pub const S_MACRO: usize = 12;

// ------------------------------------------------------------------------------------------
// Discrete settings
// ------------------------------------------------------------------------------------------

pub const WINDOWS: [&str; 6] = ["Hann", "Gauss", "Triangle", "Tukey", "Decay", "Swell"];
pub const MODES: [&str; 2] = ["Cloud", "Sync"];
pub const FILTER_TYPES: [&str; 7] = ["Low-pass 12", "Low-pass 24", "High-pass 12", "High-pass 24", "Band-pass", "Notch", "Peak"];
pub const LFO_MODES: [&str; 3] = ["Free", "Retrig", "1-shot"];
pub const VOICE_MODES: [&str; 3] = ["Poly", "Mono", "Legato"];
/// Synced lengths (quarter notes)
pub const SYNC_NAMES: [&str; 16] = ["8 bars", "4 bars", "2 bars", "1 bar", "1/2", "1/2T", "1/4.", "1/4", "1/4T", "1/8.", "1/8", "1/8T", "1/16", "1/16T", "1/32", "1/64"];
pub const SYNC_BEATS: [f64; 16] = [32.0, 16.0, 8.0, 4.0, 2.0, 4.0 / 3.0, 1.5, 1.0, 2.0 / 3.0, 0.75, 0.5, 1.0 / 3.0, 0.25, 1.0 / 6.0, 0.125, 0.0625];

/// Reverb spaces (names used by the reverb module)
pub const REVERBS: [(&str, &str); 14] = [
    ("Hall", "Concert hall"),
    ("Hall", "Bright hall"),
    ("Room", "Small room"),
    ("Room", "Studio room"),
    ("Room", "Chamber"),
    ("Plate", "Realistic plate"),
    ("Plate", "Vocal plate"),
    ("Epic", "Cathedral"),
    ("Epic", "Endless"),
    ("Special", "Shimmer cloud"),
    ("Special", "Modulated"),
    ("Mechanical", "Spring"),
    ("Mechanical", "Metal tank"),
    ("Non-linear", "Gated"),
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn table_is_consistent() {
        assert_eq!(N_DEST, 2 * 15 + 15 + 12 + 5 + 10 + 7);
        for d in 0..N_DEST {
            let k = def(d);
            for n in [0.0, 0.25, 0.5, 1.0] {
                assert!((k.norm(k.plain(n)) - n).abs() < 1e-3, "{}", dest_name(d));
            }
            assert!(!dest_name(d).is_empty());
        }
        assert_eq!(dest_name(layer_d(1, L_POS)), "B Position");
        assert_eq!(dest_name(X_RMIX), "Reverb mix");
        assert_eq!(dest_name(X_DMIX), "Delay mix");
    }
}

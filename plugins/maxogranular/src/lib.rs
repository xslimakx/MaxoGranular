//! MaxoGranular - a granular synthesizer.
//!
//! Two grain layers (each a built-in sound or a WAV file, played as a cloud of grains or as
//! pitch-synchronous grains), a filter, three envelopes, four drawn LFOs, a modulation matrix
//! (drag a source onto any knob), delay and reverb.

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

pub mod dsp;
mod editor;
pub mod kinds;
pub mod presets;
pub mod sources;
pub mod wave;

use kinds::{dest_name, def, N_DEST};
use sources::{Source, SourceRef};
use wave::Wave;

pub const N_LAYERS: usize = 2;
pub const N_ENVS: usize = 3;
pub const N_LFOS: usize = 4;
pub const MAX_VOICES: usize = 16;
pub const MAX_MODS: usize = 32;
/// grain events kept for the display
pub const GRAIN_RING: usize = 1024;

// ------------------------------------------------------------------------------------------
// Saved settings (not automatable)
// ------------------------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct LayerSet {
    pub on: bool,
    pub source: SourceRef,
    /// 0 cloud, 1 sync
    pub mode: u8,
    /// kinds::WINDOWS
    pub window: u8,
    /// the grains follow the keys
    pub keytrack: bool,
    /// 1 .. 8 grain streams (unison)
    pub streams: u8,
    /// through the filter
    pub filter: bool,
    /// note that plays the source at its own pitch
    pub root: u8,
}

impl Default for LayerSet {
    fn default() -> Self {
        Self { on: true, source: SourceRef::Builtin(0), mode: 0, window: 0, keytrack: true, streams: 1, filter: true, root: 60 }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct LfoSet {
    pub wave: Wave,
    /// rate as a note length
    pub sync: bool,
    /// kinds::SYNC_NAMES
    pub sync_len: u8,
    /// kinds::LFO_MODES
    pub mode: u8,
}

impl Default for LfoSet {
    fn default() -> Self {
        Self { wave: Wave::preset(wave::Preset::Sine), sync: false, sync_len: 7, mode: 1 }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct EnvSet {
    /// -1 .. 1: attack curve (negative = fast start)
    pub curve_a: f32,
    /// -1 .. 1: decay / release curve
    pub curve_d: f32,
}

impl Default for EnvSet {
    fn default() -> Self {
        Self { curve_a: 0.0, curve_d: -0.6 }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct FilterSet {
    pub on: bool,
    pub ftype: u8,
}

impl Default for FilterSet {
    fn default() -> Self {
        Self { on: true, ftype: 1 }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct VoiceSet {
    /// kinds::VOICE_MODES
    pub mode: u8,
    pub voices: u8,
    /// pitch bend range, semitones
    pub bend: u8,
}

impl Default for VoiceSet {
    fn default() -> Self {
        Self { mode: 0, voices: 12, bend: 2 }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct FxSet {
    pub delay_on: bool,
    pub delay_sync: bool,
    pub delay_div: u8,
    pub pingpong: bool,
    pub reverb_on: bool,
    pub reverb_type: u8,
}

impl Default for FxSet {
    fn default() -> Self {
        Self { delay_on: true, delay_sync: true, delay_div: 9, pingpong: true, reverb_on: true, reverb_type: 0 }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ModSlot {
    pub src: u8,
    pub dst: u16,
    /// -1 .. 1 of the destination's range
    pub amount: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct State {
    pub layers: Vec<LayerSet>,
    pub lfos: Vec<LfoSet>,
    pub envs: Vec<EnvSet>,
    pub filter: FilterSet,
    pub voice: VoiceSet,
    pub fx: FxSet,
    pub mods: Vec<ModSlot>,
    pub preset: String,
}

impl Default for State {
    fn default() -> Self {
        let mut b = LayerSet { on: false, source: SourceRef::Builtin(12), ..LayerSet::default() };
        b.window = 1;
        Self {
            layers: vec![LayerSet::default(), b],
            lfos: vec![LfoSet::default(); N_LFOS],
            envs: vec![EnvSet::default(); N_ENVS],
            filter: FilterSet::default(),
            voice: VoiceSet::default(),
            fx: FxSet::default(),
            mods: vec![],
            preset: "Init".into(),
        }
    }
}

impl State {
    pub fn sanitize(&mut self) {
        self.layers.resize(N_LAYERS, LayerSet::default());
        self.lfos.resize(N_LFOS, LfoSet::default());
        self.envs.resize(N_ENVS, EnvSet::default());
        for l in self.layers.iter_mut() {
            l.mode = l.mode.min(1);
            l.window = l.window.min(kinds::WINDOWS.len() as u8 - 1);
            l.streams = l.streams.clamp(1, 8);
            l.root = l.root.min(127);
            if let SourceRef::Builtin(i) = l.source {
                l.source = SourceRef::Builtin(i.min(sources::BUILTIN.len() as u16 - 1));
            }
        }
        for l in self.lfos.iter_mut() {
            l.wave.sanitize();
            l.sync_len = l.sync_len.min(kinds::SYNC_NAMES.len() as u8 - 1);
            l.mode = l.mode.min(2);
        }
        for e in self.envs.iter_mut() {
            e.curve_a = e.curve_a.clamp(-1.0, 1.0);
            e.curve_d = e.curve_d.clamp(-1.0, 1.0);
        }
        self.filter.ftype = self.filter.ftype.min(kinds::FILTER_TYPES.len() as u8 - 1);
        self.voice.mode = self.voice.mode.min(2);
        self.voice.voices = self.voice.voices.clamp(1, MAX_VOICES as u8);
        self.voice.bend = self.voice.bend.clamp(0, 24);
        self.fx.delay_div = self.fx.delay_div.min(kinds::SYNC_NAMES.len() as u8 - 1);
        self.fx.reverb_type = self.fx.reverb_type.min(kinds::REVERBS.len() as u8 - 1);
        self.mods.retain(|m| (m.src as usize) < kinds::N_SRC && (m.dst as usize) < N_DEST && kinds::modulatable(m.dst as usize));
        self.mods.truncate(MAX_MODS);
        for m in self.mods.iter_mut() {
            m.amount = if m.amount.is_finite() { m.amount.clamp(-1.0, 1.0) } else { 0.0 };
        }
    }
}

// ------------------------------------------------------------------------------------------
// Parameters
// ------------------------------------------------------------------------------------------

/// One automatable control (its range and units come from `kinds::def`).
#[derive(Params)]
pub struct Knob {
    #[id = "v"]
    pub v: FloatParam,
}

impl Knob {
    fn new(d: usize) -> Self {
        let k = def(d);
        Self {
            v: FloatParam::new(dest_name(d), k.default_norm(), FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(move |n| def(d).fmt(n)))
                .with_string_to_value(Arc::new(move |s| def(d).parse(s))),
        }
    }
}

#[derive(Params)]
pub struct GrainParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,
    #[persist = "state"]
    pub state: Arc<RwLock<State>>,
    #[nested(array, group = "Controls")]
    pub k: Vec<Knob>,
}

impl Default for GrainParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(editor::BASE_W as u32, editor::BASE_H as u32),
            state: Arc::new(RwLock::new(State::default())),
            k: (0..N_DEST).map(Knob::new).collect(),
        }
    }
}

impl GrainParams {
    pub fn knob(&self, d: usize) -> &FloatParam {
        &self.k[d].v
    }
}

// ------------------------------------------------------------------------------------------
// Shared with the GUI
// ------------------------------------------------------------------------------------------

/// One grain for the display: written by the audio thread, read by the GUI.
pub struct GrainVis {
    /// layer (0 / 1) | 2 = reverse
    pub flags: AtomicU32,
    /// position in the source, 0..1
    pub pos: AtomicF32,
    /// length in the source, 0..1 (negative: backwards)
    pub len: AtomicF32,
    pub pitch: AtomicF32,
    pub pan: AtomicF32,
    pub amp: AtomicF32,
    /// birth (engine sample clock) and life in samples
    pub birth: AtomicU64,
    pub life: AtomicU32,
}

pub struct Shared {
    pub set_version: AtomicU32,
    pub wave_version: AtomicU32,
    /// sources for the two layers, handed to the audio thread
    pub sources: Mutex<[Option<Arc<Source>>; N_LAYERS]>,
    /// replaced sources, kept until the audio thread has let go of them: (version, source)
    pub graveyard: Mutex<Vec<(u32, Arc<Source>)>>,
    pub src_version: AtomicU32,
    /// the version the audio thread has taken (old sources can be freed after that)
    pub src_taken: AtomicU32,
    pub sr: AtomicF32,
    pub clock: AtomicU64,
    pub grains: Vec<GrainVis>,
    pub grain_w: AtomicU32,
    /// modulated value (normalized) of every control, newest voice
    pub live: Vec<AtomicF32>,
    /// envelope level / LFO value of the newest voice
    pub env_lvl: [AtomicF32; N_ENVS],
    /// time since the stage started (ms) | stage << 24
    pub env_stage: [AtomicU32; N_ENVS],
    pub lfo_ph: [AtomicF32; N_LFOS],
    pub lfo_val: [AtomicF32; N_LFOS],
    /// scan position (0..1) of the newest voice per layer
    pub scan: [AtomicF32; N_LAYERS],
    pub voices: AtomicU32,
    pub grain_count: AtomicU32,
    pub out_pk: [AtomicF32; 2],
    pub notes_on: Vec<AtomicBool>,
    /// on-screen keyboard: (note, on)
    pub kb: Mutex<Vec<(u8, bool, f32)>>,
    pub wheel: AtomicF32,
    /// pitch bend as the engine plays it, -1..1 (MIDI or the on-screen wheel)
    pub bend: AtomicF32,
    /// the on-screen pitch / mod wheels: -1..1, 0..1, changed when `ui_ctl` moves
    pub ui_bend: AtomicF32,
    pub ui_wheel: AtomicF32,
    pub ui_ctl: AtomicU32,
    pub tempo: AtomicF32,
    pub delay_ms: AtomicF32,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            set_version: AtomicU32::new(1),
            wave_version: AtomicU32::new(1),
            sources: Mutex::new([None, None]),
            graveyard: Mutex::new(Vec::new()),
            src_version: AtomicU32::new(0),
            src_taken: AtomicU32::new(0),
            sr: AtomicF32::new(48000.0),
            clock: AtomicU64::new(0),
            grains: (0..GRAIN_RING)
                .map(|_| GrainVis {
                    flags: AtomicU32::new(0),
                    pos: AtomicF32::new(0.0),
                    len: AtomicF32::new(0.0),
                    pitch: AtomicF32::new(0.0),
                    pan: AtomicF32::new(0.0),
                    amp: AtomicF32::new(0.0),
                    birth: AtomicU64::new(0),
                    life: AtomicU32::new(0),
                })
                .collect(),
            grain_w: AtomicU32::new(0),
            live: (0..N_DEST).map(|d| AtomicF32::new(def(d).default_norm())).collect(),
            env_lvl: std::array::from_fn(|_| AtomicF32::new(0.0)),
            env_stage: std::array::from_fn(|_| AtomicU32::new(0)),
            lfo_ph: std::array::from_fn(|_| AtomicF32::new(0.0)),
            lfo_val: std::array::from_fn(|_| AtomicF32::new(0.0)),
            scan: std::array::from_fn(|_| AtomicF32::new(0.0)),
            voices: AtomicU32::new(0),
            grain_count: AtomicU32::new(0),
            out_pk: std::array::from_fn(|_| AtomicF32::new(0.0)),
            notes_on: (0..128).map(|_| AtomicBool::new(false)).collect(),
            kb: Mutex::new(Vec::with_capacity(64)),
            wheel: AtomicF32::new(0.0),
            bend: AtomicF32::new(0.0),
            ui_bend: AtomicF32::new(0.0),
            ui_wheel: AtomicF32::new(0.0),
            ui_ctl: AtomicU32::new(0),
            tempo: AtomicF32::new(120.0),
            delay_ms: AtomicF32::new(0.0),
        }
    }
}

impl Shared {
    pub fn bump_settings(&self) {
        self.set_version.fetch_add(1, Ordering::AcqRel);
    }
    pub fn bump_waves(&self) {
        self.wave_version.fetch_add(1, Ordering::AcqRel);
    }
    /// Hands new sources to the audio thread.
    pub fn set_sources(&self, s: [Option<Arc<Source>>; N_LAYERS]) {
        let v = self.src_version.load(Ordering::Acquire) + 1;
        let old = std::mem::replace(&mut *self.sources.lock(), s);
        self.graveyard.lock().extend(old.into_iter().flatten().map(|o| (v, o)));
        self.src_version.store(v, Ordering::Release);
        self.purge();
    }
    /// Frees the sources the audio thread no longer uses (call from the GUI).
    pub fn purge(&self) {
        let taken = self.src_taken.load(Ordering::Acquire);
        self.graveyard.lock().retain(|(v, _)| *v > taken);
    }
    /// The source a layer plays now.
    pub fn source(&self, l: usize) -> Option<Arc<Source>> {
        self.sources.lock()[l].clone()
    }
}

// ------------------------------------------------------------------------------------------
// Plugin
// ------------------------------------------------------------------------------------------

pub struct MaxoGranular {
    params: Arc<GrainParams>,
    shared: Arc<Shared>,
    engine: dsp::Engine,
    demo: dsp::Demo,
    ui_ctl: u32,
}

impl Default for MaxoGranular {
    fn default() -> Self {
        Self { params: Arc::new(GrainParams::default()), shared: Arc::new(Shared::default()), engine: dsp::Engine::new(48000.0), demo: dsp::Demo::new(), ui_ctl: 0 }
    }
}

/// Loads the sources the state names (not on the audio thread).
pub fn load_sources(st: &State, shared: &Shared) -> [Arc<Source>; N_LAYERS] {
    let s = [sources::load(&st.layers[0].source), sources::load(&st.layers[1].source)];
    shared.set_sources([Some(s[0].clone()), Some(s[1].clone())]);
    s
}

impl Plugin for MaxoGranular {
    const NAME: &'static str = "MaxoGranular";
    const VENDOR: &'static str = "Maxo";
    const URL: &'static str = "https://github.com/xslimakx/MaxoGranular";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout { main_input_channels: None, main_output_channels: NonZeroU32::new(2), ..AudioIOLayout::const_default() }];

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone(), self.shared.clone())
    }

    fn initialize(&mut self, _layout: &AudioIOLayout, config: &BufferConfig, _context: &mut impl InitContext<Self>) -> bool {
        let st = {
            let mut st = self.params.state.write();
            st.sanitize();
            st.clone()
        };
        self.shared.sr.store(config.sample_rate, Ordering::Relaxed);
        self.engine = dsp::Engine::new(config.sample_rate);
        let srcs = load_sources(&st, &self.shared);
        self.engine.prepare(&st, srcs);
        self.shared.src_taken.store(self.shared.src_version.load(Ordering::Acquire), Ordering::Release);
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, context: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let t = context.transport();
        let transport = dsp::Transport { tempo: t.tempo.unwrap_or(120.0), pos_beats: t.pos_beats(), playing: t.playing };
        self.engine.sync(&self.params, &self.shared);
        let ctl = dsp::Controls::from_params(&self.params);
        self.engine.begin_block(&ctl, &transport, &self.shared);
        // the on-screen wheels
        let uc = self.shared.ui_ctl.load(Ordering::Acquire);
        if uc != self.ui_ctl {
            self.ui_ctl = uc;
            self.engine.bend = self.shared.ui_bend.load(Ordering::Relaxed);
            self.engine.wheel = self.shared.ui_wheel.load(Ordering::Relaxed);
        }
        // the on-screen keyboard
        if let Some(mut kb) = self.shared.kb.try_lock() {
            for (n, on, v) in kb.drain(..) {
                if on {
                    self.engine.note_on(n, v);
                } else {
                    self.engine.note_off(n);
                }
            }
        }
        let n = buffer.samples();
        let mut next = context.next_event();
        let out = buffer.as_slice();
        let mut i = 0;
        while i < n {
            // events at this sample
            while let Some(ev) = next {
                if ev.timing() as usize > i {
                    break;
                }
                match ev {
                    NoteEvent::NoteOn { note, velocity, .. } => self.engine.note_on(note, velocity),
                    NoteEvent::NoteOff { note, .. } => self.engine.note_off(note),
                    NoteEvent::Choke { note, .. } => self.engine.note_off(note),
                    NoteEvent::MidiPitchBend { value, .. } => self.engine.bend = value * 2.0 - 1.0,
                    NoteEvent::MidiChannelPressure { pressure, .. } => self.engine.pressure = pressure,
                    NoteEvent::PolyPressure { pressure, .. } => self.engine.pressure = pressure,
                    NoteEvent::MidiCC { cc, value, .. } => match cc {
                        1 => self.engine.wheel = value,
                        64 => self.engine.sustain(value >= 0.5),
                        120 | 123 => self.engine.all_off(),
                        _ => {}
                    },
                    _ => {}
                }
                next = context.next_event();
            }
            self.demo.tick(&mut self.engine, i == 0, n);
            let (l, r) = self.engine.process(&self.shared);
            out[0][i] = l;
            if out.len() > 1 {
                out[1][i] = r;
            }
            i += 1;
        }
        self.engine.end_block(&self.shared);
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for MaxoGranular {
    const VST3_CLASS_ID: [u8; 16] = *b"MaxoGranularSynt";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_vst3!(MaxoGranular);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_roundtrip() {
        let mut st = State::default();
        st.mods.push(ModSlot { src: 3, dst: 2, amount: 0.4 });
        let s = serde_json::to_string(&st).unwrap();
        let mut b: State = serde_json::from_str(&s).unwrap();
        b.sanitize();
        assert_eq!(b, st);
        let mut bad: State = serde_json::from_str(r#"{"layers":[],"mods":[{"src":99,"dst":1,"amount":3.0},{"src":1,"dst":2,"amount":9.0}]}"#).unwrap();
        bad.sanitize();
        assert_eq!(bad.layers.len(), 2);
        assert_eq!(bad.mods.len(), 1);
        assert_eq!(bad.mods[0].amount, 1.0);
    }

    #[test]
    fn param_ids_are_unique_and_named() {
        let p = GrainParams::default();
        let map = p.param_map();
        assert_eq!(map.len(), N_DEST);
        let mut ids: Vec<&str> = map.iter().map(|(i, _, _)| i.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), N_DEST);
        assert_eq!(p.knob(kinds::layer_d(0, kinds::L_POS)).name(), "A Position");
    }
}

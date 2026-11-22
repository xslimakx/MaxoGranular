//! The pre-rendered photographic textures (see tools/render_textures.py).

use nih_plug_egui::egui::{self, Color32, ColorImage, TextureHandle, TextureOptions};

macro_rules! asset {
    ($n:literal) => {
        include_bytes!(concat!("../../assets/ui/", $n))
    };
}

fn placeholder() -> ColorImage {
    ColorImage::new([4, 4], Color32::from_gray(60))
}

pub fn jpeg(data: &[u8]) -> ColorImage {
    let mut dec = zune_jpeg::JpegDecoder::new(data);
    if let Ok(px) = dec.decode() {
        if let Some((w, h)) = dec.dimensions() {
            if px.len() == w * h * 3 {
                return ColorImage::from_rgb([w, h], &px);
            }
        }
    }
    placeholder()
}

pub fn png(data: &[u8]) -> ColorImage {
    let mut dec = zune_png::PngDecoder::new(data);
    if let Ok(px) = dec.decode_raw() {
        if let Some((w, h)) = dec.get_dimensions() {
            if px.len() == w * h * 4 {
                return ColorImage::from_rgba_unmultiplied([w, h], &px);
            }
            if px.len() == w * h * 3 {
                return ColorImage::from_rgb([w, h], &px);
            }
        }
    }
    placeholder()
}

pub struct Tex {
    pub face: TextureHandle,
    pub knob_base: TextureHandle,
    pub knob_cap: TextureHandle,
    pub knob_light: TextureHandle,
    pub knob_rot: TextureHandle,
    pub reel: TextureHandle,
    pub tape: TextureHandle,
    /// layer, mod, filter: (bezel, glass)
    pub crt: [(TextureHandle, TextureHandle); 3],
    pub toggle_up: TextureHandle,
    pub toggle_down: TextureHandle,
    /// amber, green, red: (on, off)
    pub lamp: [(TextureHandle, TextureHandle); 3],
    pub btn_on: TextureHandle,
    pub btn_off: TextureHandle,
    pub jack: TextureHandle,
    pub plug: TextureHandle,
    pub key_w: TextureHandle,
    pub key_w_dn: TextureHandle,
    pub key_b: TextureHandle,
    pub key_b_dn: TextureHandle,
    pub wheel_slot: TextureHandle,
    pub wheel_rubber: TextureHandle,
    pub wheel_light: TextureHandle,
    pub glow: TextureHandle,
    pub vu: TextureHandle,
    pub badge: TextureHandle,
}

impl Tex {
    pub fn load(ctx: &egui::Context) -> Self {
        let opt = TextureOptions { magnification: egui::TextureFilter::Linear, minification: egui::TextureFilter::Linear, wrap_mode: egui::TextureWrapMode::ClampToEdge, mipmap_mode: Some(egui::TextureFilter::Linear) };
        let rep = TextureOptions { wrap_mode: egui::TextureWrapMode::Repeat, ..opt };
        let p = |name: &str, data: &[u8]| ctx.load_texture(name, png(data), opt);
        Self {
            face: ctx.load_texture("face", jpeg(asset!("faceplate.jpg")), opt),
            knob_base: p("knob_base", asset!("knob_base.png")),
            knob_cap: p("knob_cap", asset!("knob_cap.png")),
            knob_light: p("knob_light", asset!("knob_light.png")),
            knob_rot: p("knob_rot", asset!("knob_rot.png")),
            reel: p("reel", asset!("reel.png")),
            tape: p("tape", asset!("tape.png")),
            crt: [
                (p("crt_layer", asset!("crt_layer.png")), p("glass_layer", asset!("glass_layer.png"))),
                (p("crt_mod", asset!("crt_mod.png")), p("glass_mod", asset!("glass_mod.png"))),
                (p("crt_filter", asset!("crt_filter.png")), p("glass_filter", asset!("glass_filter.png"))),
            ],
            toggle_up: p("toggle_up", asset!("toggle_up.png")),
            toggle_down: p("toggle_down", asset!("toggle_down.png")),
            lamp: [
                (p("la_on", asset!("lamp_amber_on.png")), p("la_off", asset!("lamp_amber_off.png"))),
                (p("lg_on", asset!("lamp_green_on.png")), p("lg_off", asset!("lamp_green_off.png"))),
                (p("lr_on", asset!("lamp_red_on.png")), p("lr_off", asset!("lamp_red_off.png"))),
            ],
            btn_on: p("btn_on", asset!("btn_on.png")),
            btn_off: p("btn_off", asset!("btn_off.png")),
            jack: p("jack", asset!("jack.png")),
            plug: p("plug", asset!("plug.png")),
            key_w: p("key_w", asset!("key_white.png")),
            key_w_dn: p("key_w_dn", asset!("key_white_down.png")),
            key_b: p("key_b", asset!("key_black.png")),
            key_b_dn: p("key_b_dn", asset!("key_black_down.png")),
            wheel_slot: p("wheel_slot", asset!("wheel_slot.png")),
            wheel_rubber: ctx.load_texture("wheel_rubber", png(asset!("wheel_rubber.png")), rep),
            wheel_light: p("wheel_light", asset!("wheel_light.png")),
            glow: p("glow", asset!("glow.png")),
            vu: ctx.load_texture("vu", jpeg(asset!("vu_face.jpg")), opt),
            badge: p("badge", asset!("badge.png")),
        }
    }
}

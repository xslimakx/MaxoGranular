//! The top strip (badge, preset display, macros, volume, VU meters), the preset browser and
//! the modulation matrix.

use nih_plug_egui::egui::{self, epaint::PathStroke, Align2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, UiBuilder, Vec2};
use std::sync::atomic::Ordering;

use super::hw::*;
use super::knobs::{jack, knob};
use super::tex::Tex;
use super::{edit, load_preset, open_folder, Cx, Gui, Overlay, P_HEADER};
use crate::kinds::*;
use crate::presets::{self, PresetData, CATEGORIES};

fn all_presets(g: &Gui) -> Vec<PresetData> {
    let mut v = g.factory.clone();
    v.extend(g.user.iter().cloned());
    v
}

/// Presets in the order the browser shows them (respects the filters).
fn visible(g: &Gui) -> Vec<PresetData> {
    let s = g.search.to_lowercase();
    all_presets(g)
        .into_iter()
        .filter(|d| match g.cat {
            None => true,
            Some(i) if i < CATEGORIES.len() => d.category == CATEGORIES[i] && d.author != "User",
            Some(_) => d.author == "User",
        })
        .filter(|d| !g.fav_only || g.favs.contains(&d.name))
        .filter(|d| s.is_empty() || d.name.to_lowercase().contains(&s) || d.category.to_lowercase().contains(&s))
        .collect()
}

pub fn draw(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex) {
    let (x0, y0) = (P_HEADER[0], P_HEADER[1]);
    let at = |x: f32, y: f32| cx.at(x0 + x, y0 + y);
    let rr = |x: f32, y: f32, w: f32, h: f32| cx.r(x0 + x, y0 + y, w, h);
    let p = ui.painter().clone();

    sprite(&p, &tex.badge, rr(16.0, 2.0, 242.0, 50.0), Color32::WHITE);

    // preset display
    let lr = rr(270.0, 11.0, 300.0, 32.0);
    recess(&p, lr, LCD_BG);
    let name = cx.st.preset.clone();
    let cat = g.factory.iter().chain(g.user.iter()).find(|d| d.name == name).map(|d| if d.author == "User" { "USER".to_string() } else { d.category.to_uppercase() }).unwrap_or_else(|| "—".into());
    let cl = p.with_clip_rect(lr.shrink(2.0));
    lcd_text(&cl, Pos2::new(lr.left() + 8.0, lr.top() + 12.0), &name.to_uppercase(), 13.0, AMBER, Align2::LEFT_CENTER);
    let grains = cx.shared.grain_count.load(Ordering::Relaxed);
    // messages (a patched cable, a saved preset) take the second line for a moment
    let line2 = match &g.status {
        Some((s, _)) => s.to_uppercase(),
        None => format!("{cat}   ·   {} VOICES   ·   {} GRAINS", cx.voices, grains),
    };
    let blink = if g.status.is_some() { 1.0 } else { 0.7 };
    lcd_text(&cl, Pos2::new(lr.left() + 8.0, lr.bottom() - 7.0), &line2, 7.5, with_a(AMBER, blink), Align2::LEFT_CENTER);
    if g.favs.contains(&name) {
        lcd_text(&cl, Pos2::new(lr.right() - 10.0, lr.top() + 12.0), "★", 12.0, AMBER, Align2::RIGHT_CENTER);
    }
    gloss(&p, lr);
    let resp = ui.interact(lr, egui::Id::new("preset_lcd"), Sense::click()).on_hover_text("Open the preset browser");
    if resp.hovered() {
        p.rect_filled(lr, 2.0, Color32::from_white_alpha(6));
    }
    if resp.clicked() {
        g.overlay = if g.overlay == Overlay::Presets { Overlay::None } else { Overlay::Presets };
    }
    // ◀ ▶ step through the list
    let list = visible(g);
    let cur = list.iter().position(|d| d.name == name);
    let step = |k: i32| -> Option<PresetData> {
        if list.is_empty() {
            return None;
        }
        let n = list.len() as i32;
        let i = cur.map(|c| (c as i32 + k).rem_euclid(n)).unwrap_or(0);
        list.get(i as usize).cloned()
    };
    if arrow_btn(ui, rr(576.0, 11.0, 22.0, 32.0), egui::Id::new("pprev"), true, "Previous preset").clicked() {
        if let Some(d) = step(-1) {
            load_preset(cx, &d);
        }
    }
    if arrow_btn(ui, rr(601.0, 11.0, 22.0, 32.0), egui::Id::new("pnext"), false, "Next preset").clicked() {
        if let Some(d) = step(1) {
            load_preset(cx, &d);
        }
    }
    let sv = g.ease(egui::Id::new("savelit"), if g.save_name.is_some() { 1.0 } else { 0.0 }, 14.0, cx.dt);
    let sr = rr(630.0, 14.0, 48.0, 26.0);
    if lit_button(ui, tex, sr, egui::Id::new("saveb"), "SAVE", sv, AMBER, "Save the sound as a user preset").clicked() {
        g.save_name = if g.save_name.is_some() { None } else { Some(name.clone()) };
    }
    if let Some(mut buf) = g.save_name.clone() {
        egui::Area::new(egui::Id::new("savearea")).fixed_pos(sr.left_bottom() + Vec2::new(-200.0, 8.0)).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(RichText::new("SAVE PRESET").font(cond(11.0)).color(AMBER));
                let te = ui.add(egui::TextEdit::singleline(&mut buf).desired_width(220.0));
                te.request_focus();
                ui.horizontal(|ui| {
                    let go = ui.button("Save").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        g.save_name = None;
                        return;
                    }
                    if go && !buf.trim().is_empty() {
                        let nm = buf.trim().to_string();
                        let base_cat = g.factory.iter().find(|d| d.name == name).map(|d| d.category.clone()).unwrap_or_else(|| "Pads".into());
                        let d = presets::capture(cx.p, &nm, &base_cat);
                        match presets::save_user(&d) {
                            Ok(_) => {
                                edit(cx, |st| st.preset = nm.clone());
                                g.user = presets::list_user();
                                g.say(format!("Saved “{nm}”"), cx.now);
                            }
                            Err(e) => g.say(format!("Could not save: {e}"), cx.now),
                        }
                        g.save_name = None;
                    } else {
                        g.save_name = Some(buf.clone());
                    }
                });
            });
        });
    }

    // macros
    p.line_segment([at(690.0, 10.0), at(690.0, 46.0)], Stroke::new(1.0f32, INK_DIM));
    let tint = Color32::from_rgb(255, 228, 150);
    for i in 0..4 {
        let x = 712.0 + i as f32 * 64.0;
        knob(ui, cx, g, tex, M_MACRO + i, at(x, 24.0), 12.0, tint, &format!("MACRO {}", i + 1));
        jack(ui, cx, g, tex, S_MACRO + i, at(x + 30.0, 22.0), "");
    }
    p.line_segment([at(952.0, 10.0), at(952.0, 46.0)], Stroke::new(1.0f32, INK_DIM));
    knob(ui, cx, g, tex, M_VOLUME, at(980.0, 24.0), 14.0, Color32::from_rgb(240, 236, 226), "VOLUME");

    // VU meters
    for c in 0..2 {
        let pk = cx.shared.out_pk[c].swap(0.0, Ordering::Relaxed);
        let db = 20.0 * pk.max(1e-6).log10();
        let target = vu_t(db).clamp(0.0, 1.05);
        let (pos, vel) = g.vu[c];
        let dt = cx.dt.max(1.0 / 240.0);
        let mut v = vel + ((target - pos) * 190.0 - vel * 20.0) * dt;
        let mut x = pos + v * dt;
        if x < 0.0 {
            x = 0.0;
            v = v.abs() * 0.3;
        }
        if x > 1.08 {
            x = 1.08;
            v = -v.abs() * 0.3;
        }
        g.vu[c] = (x, v);
        vu(ui, tex, rr(1010.0 + c as f32 * 84.0, 6.0, 78.0, 42.0), x, if c == 0 { "L" } else { "R" });
    }
}

/// Needle position of a level (dB): -20 at the left end, +3 at the right, 0 at about 3/4.
fn vu_t(db: f32) -> f32 {
    let a = 10f32.powf(db / 20.0);
    ((a - 0.1) / (1.413 - 0.1)).powf(0.62)
}

fn vu(ui: &Ui, tex: &Tex, r: Rect, t: f32, label: &str) {
    let p = ui.painter();
    p.rect_filled(r.expand(3.0), CornerRadius::same(3), Color32::from_rgb(14, 12, 10));
    p.rect_filled(r.expand(3.0).translate(Vec2::new(1.0, 2.0)), CornerRadius::same(3), Color32::from_black_alpha(60));
    sprite(p, &tex.vu, r, Color32::WHITE);
    let p = &p.with_clip_rect(r.shrink(1.0));
    let piv = Pos2::new(r.center().x, r.bottom() + r.height() * 1.0);
    let rad = r.height() * 1.42;
    let ang = |t: f32| (-30.0 + 60.0 * t).to_radians();
    let pt = |t: f32, rr: f32| Pos2::new(piv.x + rr * ang(t).sin(), piv.y - rr * ang(t).cos());
    let ink_c = Color32::from_rgb(40, 32, 22);
    let red = Color32::from_rgb(190, 40, 30);
    let t0 = vu_t(0.0);
    let arc: Vec<Pos2> = (0..=30).map(|i| pt(i as f32 / 30.0 * t0, rad)).collect();
    p.add(Shape::line(arc, PathStroke::new(1.0f32, ink_c)));
    let red_arc: Vec<Pos2> = (0..=10).map(|i| pt(t0 + i as f32 / 10.0 * (1.0 - t0), rad)).collect();
    p.add(Shape::line(red_arc, PathStroke::new(2.5f32, red)));
    for (db, lab) in [(-20.0, "20"), (-10.0, "10"), (-5.0, "5"), (-2.0, ""), (0.0, "0"), (3.0, "+3")] {
        let t = vu_t(db);
        let col = if db > -0.5 { red } else { ink_c };
        p.line_segment([pt(t, rad), pt(t, rad + 4.0)], Stroke::new(1.0f32, col));
        p.text(pt(t, rad + 7.5), Align2::CENTER_CENTER, lab, cond(6.0), col);
    }
    p.text(Pos2::new(r.left() + 7.0, r.bottom() - 6.0), Align2::LEFT_CENTER, label, cond(7.5), ink_c);
    p.text(Pos2::new(r.right() - 7.0, r.bottom() - 6.0), Align2::RIGHT_CENTER, "VU", cond(7.0), ink_c);
    // needle (clipped to the face)
    let cl = p.with_clip_rect(r);
    let tip = pt(t, rad + 6.0);
    let base = pt(t, r.height() * 1.05);
    cl.line_segment([base + Vec2::new(1.5, 1.5), tip + Vec2::new(1.5, 1.5)], Stroke::new(1.2f32, Color32::from_black_alpha(60)));
    cl.line_segment([base, tip], Stroke::new(1.2f32, Color32::from_rgb(25, 20, 15)));
    // glass
    let p = ui.painter();
    gloss(p, r);
    vgrad(p, Rect::from_min_max(r.left_bottom() - Vec2::new(0.0, 6.0), r.right_bottom()), Color32::from_black_alpha(0), Color32::from_black_alpha(80));
    let _ = tex;
}

/// The preset browser, over the grain modules.
pub fn browser(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex) {
    let r = cx.r(30.0, 68.0, 1180.0, 324.0);
    let p = ui.painter().clone();
    p.rect_filled(r.expand(6.0).translate(Vec2::new(3.0, 6.0)), 6.0, Color32::from_black_alpha(140));
    p.rect_filled(r.expand(6.0), 6.0, Color32::from_rgb(20, 18, 16));
    recess(&p, r, LCD_BG);
    glow(&p, tex, r.center(), r.width() * 0.6, AMBER, 0.04);
    // swallow clicks behind the panel
    ui.interact(r, egui::Id::new("browser_bg"), Sense::click_and_drag());
    let name = cx.st.preset.clone();
    let mut load: Option<PresetData> = None;
    ui.allocate_new_ui(UiBuilder::new().max_rect(r.shrink(10.0)), |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("PRESETS").font(mono(13.0)).color(AMBER));
            ui.add_space(20.0);
            ui.add(egui::TextEdit::singleline(&mut g.search).hint_text("search…").desired_width(220.0));
            ui.add_space(10.0);
            ui.checkbox(&mut g.fav_only, RichText::new("★ favourites only").color(AMBER));
            ui.add_space(10.0);
            if ui.button("📂 Presets folder").clicked() {
                if let Some(d) = presets::dir() {
                    open_folder(&d);
                }
            }
            if ui.button("⟳ Rescan").clicked() {
                g.user = presets::list_user();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(RichText::new("✕ Close").color(AMBER)).clicked() {
                    g.overlay = Overlay::None;
                }
            });
        });
        ui.add_space(6.0);
        ui.horizontal_top(|ui| {
            // categories
            ui.vertical(|ui| {
                ui.set_width(130.0);
                let mut cats: Vec<(Option<usize>, String)> = vec![(None, "All".into())];
                cats.extend(CATEGORIES.iter().enumerate().map(|(i, c)| (Some(i), c.to_string())));
                cats.push((Some(CATEGORIES.len()), "User".into()));
                for (i, c) in cats {
                    let n = all_presets(g)
                        .iter()
                        .filter(|d| match i {
                            None => true,
                            Some(k) if k < CATEGORIES.len() => d.category == CATEGORIES[k] && d.author != "User",
                            Some(_) => d.author == "User",
                        })
                        .count();
                    let sel = g.cat == i;
                    if ui.selectable_label(sel, RichText::new(format!("{c}  ({n})")).font(mono(11.0)).color(if sel { Color32::BLACK } else { AMBER })).clicked() {
                        g.cat = i;
                    }
                }
            });
            ui.separator();
            // the list in columns
            let list = visible(g);
            egui::ScrollArea::vertical().max_height(r.height() - 46.0).show(ui, |ui| {
                let cols = 4;
                egui::Grid::new("plist").num_columns(cols).spacing([14.0, 3.0]).min_col_width(240.0).show(ui, |ui| {
                    for (i, d) in list.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let fav = g.favs.contains(&d.name);
                            if ui.add(egui::Button::new(RichText::new(if fav { "★" } else { "☆" }).color(AMBER)).frame(false)).on_hover_text("Favourite").clicked() {
                                if fav {
                                    g.favs.retain(|f| f != &d.name);
                                } else {
                                    g.favs.push(d.name.clone());
                                }
                                presets::save_favorites(&g.favs);
                            }
                            let cur = d.name == name;
                            let txt = RichText::new(&d.name).font(mono(11.5)).color(if cur { Color32::BLACK } else { Color32::from_rgb(255, 200, 120) });
                            let resp = ui.selectable_label(cur, txt).on_hover_text(format!("{} · {}", d.category, d.author));
                            if resp.clicked() {
                                load = Some(d.clone());
                            }
                            if d.author == "User" {
                                resp.context_menu(|ui| {
                                    if ui.button("Delete").clicked() {
                                        presets::delete_user(&d.name);
                                        g.user = presets::list_user();
                                        ui.close_menu();
                                    }
                                });
                            }
                        });
                        if (i + 1) % cols == 0 {
                            ui.end_row();
                        }
                    }
                });
            });
        });
    });
    if let Some(d) = load {
        load_preset(cx, &d);
    }
    gloss(&p, r);
}

/// Every modulation as a row: source, destination, amount.
pub fn matrix(ui: &mut Ui, cx: &Cx, g: &mut Gui, tex: &Tex) {
    let r = cx.r(30.0, 404.0, 870.0, 284.0);
    let p = ui.painter().clone();
    p.rect_filled(r.expand(6.0).translate(Vec2::new(3.0, 6.0)), 6.0, Color32::from_black_alpha(140));
    p.rect_filled(r.expand(6.0), 6.0, Color32::from_rgb(20, 18, 16));
    recess(&p, r, LCD_BG);
    glow(&p, tex, r.center(), r.width() * 0.6, AMBER, 0.04);
    ui.interact(r, egui::Id::new("matrix_bg"), Sense::click_and_drag());
    let mods = cx.st.mods.clone();
    let dests: Vec<usize> = (0..N_DEST).filter(|d| modulatable(*d)).collect();
    ui.allocate_new_ui(UiBuilder::new().max_rect(r.shrink(10.0)), |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("MODULATION MATRIX").font(mono(13.0)).color(AMBER));
            ui.label(RichText::new(format!("{} / {}", mods.len(), crate::MAX_MODS)).font(mono(11.0)).color(with_a(AMBER, 0.6)));
            ui.add_space(16.0);
            if ui.button("+ Add").on_hover_text("A new row (or drag a cable from a jack onto a knob)").clicked() && mods.len() < crate::MAX_MODS {
                edit(cx, |st| st.mods.push(crate::ModSlot { src: S_LFO as u8, dst: layer_d(0, L_POS) as u16, amount: 0.25 }));
            }
            if !mods.is_empty() && ui.button("Clear all").clicked() {
                edit(cx, |st| st.mods.clear());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(RichText::new("✕ Close").color(AMBER)).clicked() {
                    g.overlay = Overlay::None;
                }
            });
        });
        ui.add_space(6.0);
        if mods.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No modulation yet.\nDrag a cable from any jack (ENV, LFO, WHEEL, VELO, MACRO…) onto a knob.").font(mono(12.0)).color(with_a(AMBER, 0.7)));
            });
            return;
        }
        egui::ScrollArea::vertical().max_height(r.height() - 46.0).show(ui, |ui| {
            egui::Grid::new("mtx").num_columns(5).spacing([12.0, 4.0]).show(ui, |ui| {
                for (i, m) in mods.iter().enumerate() {
                    let col = SRC_COL[m.src as usize];
                    let (dot, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                    ui.painter().circle_filled(dot.center(), 5.0, col);
                    let mut src = m.src as usize;
                    egui::ComboBox::from_id_salt(("msrc", i)).width(90.0).selected_text(RichText::new(SRC_NAMES[src]).color(col)).show_ui(ui, |ui| {
                        for (k, n) in SRC_NAMES.iter().enumerate() {
                            ui.selectable_value(&mut src, k, RichText::new(*n).color(SRC_COL[k]));
                        }
                    });
                    if src != m.src as usize {
                        edit(cx, |st| {
                            if let Some(x) = st.mods.get_mut(i) {
                                x.src = src as u8;
                            }
                        });
                    }
                    let mut dst = m.dst as usize;
                    egui::ComboBox::from_id_salt(("mdst", i)).width(170.0).height(400.0).selected_text(format!("→ {}", dest_name(dst))).show_ui(ui, |ui| {
                        for d in &dests {
                            ui.selectable_value(&mut dst, *d, dest_name(*d));
                        }
                    });
                    if dst != m.dst as usize {
                        edit(cx, |st| {
                            if let Some(x) = st.mods.get_mut(i) {
                                x.dst = dst as u16;
                            }
                        });
                    }
                    let mut a = m.amount * 100.0;
                    ui.spacing_mut().slider_width = 360.0;
                    if ui.add(egui::Slider::new(&mut a, -100.0..=100.0).suffix(" %").fixed_decimals(0)).changed() {
                        edit(cx, |st| {
                            if let Some(x) = st.mods.get_mut(i) {
                                x.amount = a / 100.0;
                            }
                        });
                    }
                    if ui.button("✕").on_hover_text("Remove").clicked() {
                        edit(cx, |st| {
                            if i < st.mods.len() {
                                st.mods.remove(i);
                            }
                        });
                    }
                    ui.end_row();
                }
            });
        });
    });
    gloss(&p, r);
}

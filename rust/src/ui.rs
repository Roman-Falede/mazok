//! Интерфейс: меню, панели, галерея, диалоги, иконки.

use crate::app::App;
use crate::blend::{Blend, BLENDS, GROUP_STARTS};
use crate::brush::{self, Brush, Sink, Stamp, Stroke, Tip, CATS, GRAINS, SHAPES};
use crate::doc::{Props, TextSpec};
use crate::editor::{self, Editor, GradKind, SelOp, SelShape, ShapeKind, Tool};
use crate::filters::{self, FILTERS};
use crate::raster::Img;
use crate::state::Shared;
use crate::util::{self};
use crate::vector::EraseMode;
use egui::{Color32, Pos2, Rect, Sense, Stroke as EStroke, Ui, Vec2};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

type Tex = HashMap<String, (u64, egui::TextureHandle)>;

pub struct NewCanvas {
    pub name: String,
    pub w: usize,
    pub h: usize,
    pub bg: [u8; 3],
    pub transparent: bool,
}
impl Default for NewCanvas {
    fn default() -> Self {
        NewCanvas { name: "Без названия".into(), w: 2048, h: 2048, bg: [255, 255, 255], transparent: false }
    }
}

pub struct ResizeDlg {
    pub w: usize,
    pub h: usize,
    pub keep: bool,
    pub scale: bool,
    pub anchor: (i32, i32),
}

pub struct TextDlg {
    pub spec: TextSpec,
    pub layer: Option<usize>,
    pub font: usize,
}

#[derive(Default)]
pub struct Wins {
    pub color: bool,
    pub brushes: bool,
    pub brushset: bool,
    pub layers: bool,
    pub history: bool,
    pub menu: bool,
}

pub struct UiState {
    pub new_canvas: Option<NewCanvas>,
    pub rename: Option<(String, String)>,
    pub layer_rename: Option<(usize, String)>,
    pub resize: Option<ResizeDlg>,
    pub text: Option<TextDlg>,
    pub settings: bool,
    pub help: bool,
    pub confirm_delete: Option<String>,
    pub feather: Option<f32>,
    pub dock_tab: [usize; 3],
    pub brush_cat: String,
    pub win: Wins,
    pub hex: String,
    pub hsva: Option<(egui::ecolor::Hsva, [u8; 3])>,
    pub op_before: Option<(usize, Props)>,
    pub bset_open: [bool; 9],
    pub palette_export: Option<(usize, &'static str)>,
    pub font_cache: HashMap<String, Vec<u8>>,
}

impl Default for UiState {
    fn default() -> Self {
        UiState {
            new_canvas: None, rename: None, layer_rename: None, resize: None, text: None, settings: false, help: false, confirm_delete: None,
            feather: None, dock_tab: [0, 0, 0], brush_cat: String::new(), win: Wins::default(), hex: String::new(), hsva: None, op_before: None,
            bset_open: [true, false, false, false, false, false, false, false, false], palette_export: None, font_cache: HashMap::new(),
        }
    }
}

impl UiState {
    pub fn reset_for_doc(&mut self) {
        self.op_before = None;
        self.text = None;
        self.layer_rename = None;
    }
}

const ACCENT: Color32 = Color32::from_rgb(61, 143, 230);

/* ---------- иконки ---------- */

pub fn paint_icon(p: &egui::Painter, r: Rect, name: &str, c: Color32) {
    let s = EStroke::new(1.6, c);
    let k = r.width().min(r.height()) / 24.0;
    let o = r.center() - Vec2::new(12.0 * k, 12.0 * k);
    let q = |x: f32, y: f32| Pos2::new(o.x + x * k, o.y + y * k);
    let line = |pts: &[(f32, f32)]| { p.add(egui::Shape::line(pts.iter().map(|&(x, y)| q(x, y)).collect(), s)); };
    let dashed = |pts: &[(f32, f32)]| { p.extend(egui::Shape::dashed_line(&pts.iter().map(|&(x, y)| q(x, y)).collect::<Vec<_>>(), s, 2.5 * k, 2.0 * k)); };
    let circ = |x: f32, y: f32, rr: f32| { p.circle_stroke(q(x, y), rr * k, s); };
    match name {
        "brush" => { line(&[(19.5, 3.5), (21.0, 5.0), (12.0, 14.0), (10.0, 12.0), (19.5, 3.5)]); line(&[(9.5, 12.5), (7.0, 13.5), (6.0, 16.5), (3.5, 19.5), (7.5, 20.0), (11.0, 17.5), (11.5, 14.5)]); }
        "eraser" => { line(&[(14.8, 4.6), (19.4, 9.2), (11.0, 18.7), (7.5, 18.7), (4.5, 15.7), (14.8, 4.6)]); line(&[(9.5, 8.8), (15.2, 14.5)]); line(&[(11.0, 18.7), (20.0, 18.7)]); }
        "smudge" => { let pts: Vec<(f32, f32)> = (0..=20).map(|i| { let t = i as f32 / 20.0 * std::f32::consts::PI; (12.0 + 6.0 * t.cos(), 14.0 + 6.0 * t.sin()) }).collect(); let mut v = vec![(6.0, 14.0), (12.0, 3.5), (18.0, 14.0)]; v.extend(pts.iter().rev()); line(&v); }
        "clone" => { line(&[(9.5, 3.5), (14.5, 3.5), (14.5, 8.7), (17.0, 11.0), (7.0, 11.0), (9.5, 8.7), (9.5, 3.5)]); line(&[(5.5, 11.0), (18.5, 11.0), (18.5, 15.0), (5.5, 15.0), (5.5, 11.0)]); line(&[(8.0, 15.0), (8.0, 20.5), (16.0, 20.5), (16.0, 15.0)]); }
        "fill" => { line(&[(4.5, 11.5), (11.0, 5.0), (18.0, 12.0), (11.5, 18.5), (4.5, 11.5)]); line(&[(4.5, 12.5), (17.5, 12.5)]); circ(20.0, 18.0, 1.6); }
        "gradient" => { line(&[(3.5, 5.0), (20.5, 5.0), (20.5, 19.0), (3.5, 19.0), (3.5, 5.0)]); for (i, x) in [8.0, 12.0, 16.0].iter().enumerate() { p.line_segment([q(*x, 5.0), q(*x, 19.0)], EStroke::new(1.4, c.gamma_multiply(1.0 - i as f32 * 0.3))); } }
        "picker" => { line(&[(14.7, 7.7), (6.0, 16.4), (5.2, 18.8), (7.6, 18.0), (16.3, 9.3)]); line(&[(14.0, 4.0), (20.0, 10.0)]); line(&[(15.5, 4.5), (17.0, 3.0), (21.0, 7.0), (19.5, 8.5)]); }
        "rect" => dashed(&[(4.0, 6.0), (20.0, 6.0), (20.0, 18.0), (4.0, 18.0), (4.0, 6.0)]),
        "ellipse" => { let pts: Vec<(f32, f32)> = (0..=32).map(|i| { let t = i as f32 / 32.0 * std::f32::consts::TAU; (12.0 + 8.5 * t.cos(), 12.0 + 6.5 * t.sin()) }).collect(); dashed(&pts); }
        "lasso" => { let pts: Vec<(f32, f32)> = (0..=28).map(|i| { let t = i as f32 / 28.0 * std::f32::consts::TAU * 0.85 + 2.2; (12.0 + 8.0 * t.cos(), 10.0 + 5.5 * t.sin()) }).collect(); dashed(&pts); line(&[(7.5, 14.5), (6.5, 17.5), (9.0, 18.0)]); }
        "wand" => { line(&[(4.5, 19.5), (14.5, 9.5)]); line(&[(13.0, 8.0), (16.0, 11.0)]); line(&[(17.0, 3.5), (17.0, 6.5)]); line(&[(15.5, 5.0), (18.5, 5.0)]); line(&[(20.0, 9.5), (20.0, 11.5)]); line(&[(19.0, 10.5), (21.0, 10.5)]); }
        "move" => { line(&[(12.0, 3.0), (12.0, 21.0)]); line(&[(3.0, 12.0), (21.0, 12.0)]); for (a, b, cc, d) in [(9.4, 5.6, 12.0, 3.0), (14.6, 5.6, 12.0, 3.0), (9.4, 18.4, 12.0, 21.0), (14.6, 18.4, 12.0, 21.0), (5.6, 9.4, 3.0, 12.0), (5.6, 14.6, 3.0, 12.0), (18.4, 9.4, 21.0, 12.0), (18.4, 14.6, 21.0, 12.0)] { line(&[(a, b), (cc, d)]); } }
        "text" => { line(&[(5.0, 7.0), (5.0, 4.5), (19.0, 4.5), (19.0, 7.0)]); line(&[(12.0, 4.5), (12.0, 19.5)]); line(&[(9.0, 19.5), (15.0, 19.5)]); }
        "shapes" => { circ(8.0, 8.0, 4.5); line(&[(11.5, 11.5), (20.5, 11.5), (20.5, 20.5), (11.5, 20.5), (11.5, 11.5)]); }
        "hand" => { line(&[(8.0, 13.0), (8.0, 5.5)]); line(&[(11.0, 11.0), (11.0, 4.5)]); line(&[(14.0, 11.0), (14.0, 5.0)]); line(&[(17.0, 12.0), (17.0, 7.0), (17.0, 14.0)]); line(&[(8.0, 13.0), (6.0, 11.5), (4.5, 13.0), (8.0, 18.5), (11.0, 20.5), (15.0, 20.5), (17.0, 17.5), (17.0, 13.0)]); }
        "zoom" => { circ(10.5, 10.5, 6.5); line(&[(15.5, 15.5), (20.5, 20.5)]); }
        "undo" => { line(&[(9.0, 14.0), (4.0, 9.0), (9.0, 4.0)]); line(&[(4.0, 9.0), (14.5, 9.0), (18.0, 10.5), (19.5, 14.5), (18.0, 18.5), (14.5, 20.0), (11.0, 20.0)]); }
        "redo" => { line(&[(15.0, 14.0), (20.0, 9.0), (15.0, 4.0)]); line(&[(20.0, 9.0), (9.5, 9.0), (6.0, 10.5), (4.5, 14.5), (6.0, 18.5), (9.5, 20.0), (13.0, 20.0)]); }
        "gallery" => { for (x, y) in [(3.5, 3.5), (13.5, 3.5), (3.5, 13.5), (13.5, 13.5)] { line(&[(x, y), (x + 7.0, y), (x + 7.0, y + 7.0), (x, y + 7.0), (x, y)]); } }
        "layers" => { line(&[(12.0, 3.5), (20.5, 8.0), (12.0, 12.5), (3.5, 8.0), (12.0, 3.5)]); line(&[(3.5, 12.0), (12.0, 16.5), (20.5, 12.0)]); line(&[(3.5, 16.0), (12.0, 20.5), (20.5, 16.0)]); }
        "menu" => { line(&[(4.0, 6.0), (20.0, 6.0)]); line(&[(4.0, 12.0), (20.0, 12.0)]); line(&[(4.0, 18.0), (20.0, 18.0)]); }
        "settings" => { circ(12.0, 12.0, 3.0); for i in 0..8 { let a = i as f32 * std::f32::consts::FRAC_PI_4; line(&[(12.0 + a.cos() * 6.0, 12.0 + a.sin() * 6.0), (12.0 + a.cos() * 9.0, 12.0 + a.sin() * 9.0)]); } }
        "plus" => { line(&[(12.0, 5.0), (12.0, 19.0)]); line(&[(5.0, 12.0), (19.0, 12.0)]); }
        "trash" => { line(&[(4.5, 7.0), (19.5, 7.0)]); line(&[(9.5, 7.0), (9.5, 4.5), (14.5, 4.5), (14.5, 7.0)]); line(&[(6.5, 7.0), (7.5, 20.0), (16.5, 20.0), (17.5, 7.0)]); }
        "copy" => { line(&[(8.0, 8.0), (20.0, 8.0), (20.0, 20.0), (8.0, 20.0), (8.0, 8.0)]); line(&[(16.0, 8.0), (16.0, 4.0), (4.0, 4.0), (4.0, 16.0), (8.0, 16.0)]); }
        "merge" => { line(&[(7.0, 4.0), (7.0, 9.0), (10.5, 12.5), (13.5, 12.5), (17.0, 16.0), (17.0, 20.0)]); line(&[(17.0, 4.0), (17.0, 9.0), (13.5, 12.5)]); line(&[(14.0, 17.0), (17.0, 20.0), (20.0, 17.0)]); }
        "up" => line(&[(6.0, 15.0), (12.0, 9.0), (18.0, 15.0)]),
        "down" => line(&[(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)]),
        "more" => { for x in [5.5, 12.0, 18.5] { p.circle_filled(q(x, 12.0), 1.6 * k, c); } }
        "eye" => { let pts: Vec<(f32, f32)> = (0..=24).map(|i| { let t = i as f32 / 24.0 * std::f32::consts::TAU; (12.0 + 9.5 * t.cos(), 12.0 + 5.5 * t.sin()) }).collect(); line(&pts); circ(12.0, 12.0, 2.8); }
        "lock" => { line(&[(5.0, 10.5), (19.0, 10.5), (19.0, 20.5), (5.0, 20.5), (5.0, 10.5)]); line(&[(8.0, 10.5), (8.0, 8.0), (10.0, 4.5), (14.0, 4.5), (16.0, 8.0), (16.0, 10.5)]); }
        "alphalock" => { dashed(&[(4.0, 4.0), (20.0, 4.0), (20.0, 20.0), (4.0, 20.0), (4.0, 4.0)]); line(&[(9.0, 12.5), (15.0, 12.5)]); line(&[(12.0, 9.5), (12.0, 15.5)]); }
        "clip" => { line(&[(7.0, 4.0), (7.0, 13.0), (10.0, 16.0), (19.0, 16.0)]); line(&[(16.0, 13.0), (19.0, 16.0), (16.0, 19.0)]); }
        "vector" => { let pts: Vec<(f32, f32)> = (0..=16).map(|i| { let t = i as f32 / 16.0; let u = 1.0 - t; (u * u * 4.0 + 2.0 * u * t * 12.0 + t * t * 20.0, u * u * 19.0 + 2.0 * u * t * 1.0 + t * t * 19.0) }).collect(); line(&pts); for (x, y) in [(4.0, 19.0), (20.0, 19.0)] { p.rect_stroke(Rect::from_center_size(q(x, y), Vec2::splat(4.0 * k)), 0.0, s, egui::StrokeKind::Middle); } p.circle_filled(q(12.0, 10.0), 1.8 * k, c); }
        _ => { p.circle_filled(r.center(), 2.0 * k, c); }
    }
}

fn icon_btn(ui: &mut Ui, icon: &str, selected: bool, tip: &str, size: Vec2) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    let vis = ui.visuals();
    if selected {
        ui.painter().rect_filled(r, 4.0, ACCENT.gamma_multiply(0.85));
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 4.0, vis.widgets.hovered.bg_fill);
    }
    let c = if selected { Color32::WHITE } else { vis.text_color() };
    paint_icon(ui.painter(), r.shrink(size.y * 0.16), icon, c);
    resp.on_hover_text(tip)
}

/* ---------- образец кисти ---------- */

struct AlphaSink<'a> {
    a: &'a mut Vec<f32>,
    c: &'a mut Vec<[f32; 3]>,
    w: usize,
    h: usize,
    b: Brush,
    tips: [Option<Arc<Tip>>; 7],
}
impl<'a> Sink for AlphaSink<'a> {
    fn stamp(&mut self, s: &Stamp) {
        let (w, h) = (self.w, self.h);
        let al = s.alpha.min(1.0);
        if self.b.pixel {
            let q = s.size.round().max(1.0) as i32;
            let (x0, y0) = ((s.x - q as f32 / 2.0).round() as i32, (s.y - q as f32 / 2.0).round() as i32);
            for y in y0.max(0)..(y0 + q).min(h as i32) {
                for x in x0.max(0)..(x0 + q).min(w as i32) {
                    let i = y as usize * w + x as usize;
                    self.a[i] = self.a[i] + al * (1.0 - self.a[i]);
                    self.c[i] = s.color;
                }
            }
            return;
        }
        let tip = editor::tip_for(&mut self.tips, &self.b.shape, self.b.hardness, s.size);
        let (a, c) = (&mut *self.a, &mut *self.c);
        editor::cover(&tip, s, self.b.roundness, w, h, |x, y, cv| {
            let i = y * w + x;
            let v = cv * al;
            let na = a[i] + v * (1.0 - a[i]);
            if na > 0.0 {
                for k in 0..3 {
                    c[i][k] = (c[i][k] * a[i] * (1.0 - v) + s.color[k] * v) / na;
                }
            }
            a[i] = na;
        });
    }
}

pub fn brush_sample(b: &Brush, w: usize, h: usize, col: [u8; 3]) -> Img {
    let mut a = vec![0f32; w * h];
    let mut c = vec![[0f32; 3]; w * h];
    let size = (brush::size_px(b, b.size.max(0.25)) * 0.6).clamp(if b.pixel { 2.0 } else { 3.0 }, h as f32 * 0.55);
    let bb = Brush { streamline: 0.0, ..b.clone() };
    let mut st = Stroke::new(bb.clone(), size, col, vec![], 1.0, 1.0);
    {
        let mut sink = AlphaSink { a: &mut a, c: &mut c, w, h, b: bb.clone(), tips: Default::default() };
        for i in 0..=60 {
            let t = i as f32 / 60.0;
            st.add(w as f32 * (0.08 + 0.84 * t), h as f32 / 2.0 + (t * std::f32::consts::TAU).sin() * h as f32 * 0.18, (t * std::f32::consts::PI).sin() * 0.9 + 0.1, 0.0, &mut sink);
        }
        if b.taper_end > 0.0 {
            sink.a.iter_mut().for_each(|v| *v = 0.0);
            st.rerender(&mut sink);
        } else {
            st.end(&mut sink);
        }
    }
    let g = b.grain.as_ref().map(|g| brush::grain(g));
    let mut img = Img::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let mut al = a[i] * b.opacity;
            if let Some(g) = &g {
                al *= g.at(x as i32, y as i32, b.grain_scale * 0.6, b.grain_strength);
            }
            let o = i * 4;
            for k in 0..3 {
                img.px[o + k] = (c[i][k] * al * 255.0).round() as u8;
            }
            img.px[o + 3] = (al * 255.0).round() as u8;
        }
    }
    img
}

fn texture(ctx: &egui::Context, tex: &mut Tex, key: String, ver: u64, make: impl FnOnce() -> Img) -> egui::TextureHandle {
    if let Some((v, t)) = tex.get(&key) {
        if *v == ver {
            return t.clone();
        }
    }
    let img = make();
    let t = ctx.load_texture(&key, egui::ColorImage::from_rgba_premultiplied([img.w, img.h], &img.px), egui::TextureOptions::LINEAR);
    tex.insert(key, (ver, t.clone()));
    t
}

/* ---------- контекст панелей ---------- */

struct Cx<'a> {
    ed: &'a mut Editor,
    sh: &'a mut Shared,
    st: &'a mut UiState,
    tex: &'a mut Tex,
    acts: &'a mut Vec<&'static str>,
}

fn pslider(ui: &mut Ui, label: &str, v: &mut f32, min: f32, max: f32, pct: bool) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let s = egui::Slider::new(v, min..=max);
            let s = if pct { s.custom_formatter(|x, _| format!("{:.0}%", x * 100.0)).custom_parser(|t| t.trim_end_matches('%').trim().parse::<f64>().ok().map(|x| x / 100.0)) } else { s.max_decimals(2) };
            ui.add(s).changed()
        }).inner
    }).inner
}

/* ---------- меню ---------- */

fn menu_bar(ui: &mut Ui, cx: &mut Cx) {
    egui::menu::bar(ui, |ui| {
        ui.menu_button("Файл", |ui| {
            if ui.add(egui::Button::new("Создать…").shortcut_text("Ctrl+N")).clicked() { cx.acts.push("new"); ui.close_menu(); }
            if ui.add(egui::Button::new("Открыть / импорт…").shortcut_text("Ctrl+O")).clicked() { cx.acts.push("open"); ui.close_menu(); }
            if ui.button("Галерея").clicked() { cx.acts.push("gallery"); ui.close_menu(); }
            ui.separator();
            if ui.add(egui::Button::new("Сохранить").shortcut_text("Ctrl+S")).clicked() { cx.acts.push("save"); ui.close_menu(); }
            if ui.add(egui::Button::new("Сохранить как…").shortcut_text("Ctrl+Shift+S")).on_hover_text("Сохранить проект .mazok в выбранную папку").clicked() { cx.acts.push("save_as"); ui.close_menu(); }
            if ui.button("Быстрый экспорт PNG").on_hover_text("В папку для сохранения из настроек").clicked() { cx.acts.push("quick_png"); ui.close_menu(); }
            ui.menu_button("Экспорт", |ui| {
                for (k, n) in [("export_png", "PNG"), ("export_jpeg", "JPEG"), ("export_psd", "PSD со слоями"), ("export_mazok", "Проект Мазок (.mazok)"), ("export_layers", "Слои в PNG (ZIP)"), ("export_gif", "GIF (слои — кадры)")] {
                    if ui.button(n).clicked() { cx.acts.push(k); ui.close_menu(); }
                }
            });
            if ui.button("Папка для сохранения…").clicked() { cx.acts.push("pick_export_dir"); ui.close_menu(); }
            ui.separator();
            if ui.button("Импорт кистей (.abr, .sut)…").clicked() { cx.acts.push("import_brushes"); ui.close_menu(); }
            if ui.button("Импорт палитры…").clicked() { cx.acts.push("import_palette"); ui.close_menu(); }
            ui.separator();
            if ui.button("Закрыть").clicked() { cx.acts.push("gallery"); ui.close_menu(); }
        });
        ui.menu_button("Редактирование", |ui| edit_menu(ui, cx));
        ui.menu_button("Изображение", |ui| image_menu(ui, cx));
        ui.menu_button("Слои", |ui| layer_menu(ui, cx));
        ui.menu_button("Выделение", |ui| select_menu(ui, cx));
        ui.menu_button("Фильтр", |ui| filter_menu(ui, cx));
        ui.menu_button("Просмотр", |ui| view_menu(ui, cx));
        ui.menu_button("Окно", |ui| {
            for (i, (g, t, n)) in [(0, 0, "Цвет"), (0, 1, "Образцы"), (1, 0, "Кисти"), (1, 1, "Настройки кисти"), (2, 0, "Слои"), (2, 1, "История")].iter().enumerate() {
                let _ = i;
                if ui.button(*n).clicked() { cx.st.dock_tab[*g] = *t; ui.close_menu(); }
            }
            ui.separator();
            if ui.button("Настройки…").clicked() { cx.st.settings = true; ui.close_menu(); }
        });
        ui.menu_button("Справка", |ui| {
            if ui.button("Горячие клавиши и жесты").clicked() { cx.st.help = true; ui.close_menu(); }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let name = cx.ed.doc.name.clone();
            if ui.add(egui::Label::new(egui::RichText::new(name).weak()).sense(Sense::click())).on_hover_text("Переименовать").clicked() {
                cx.st.rename = Some((cx.ed.doc.id.clone(), cx.ed.doc.name.clone()));
            }
        });
    });
}

fn edit_menu(ui: &mut Ui, cx: &mut Cx) {
    let ed = &mut *cx.ed;
    let un = ed.hist.list.get(ed.hist.pos.wrapping_sub(1)).map(|e| e.name.clone()).unwrap_or_default();
    if ui.add_enabled(ed.hist.can_undo(), egui::Button::new(format!("Отменить {un}")).shortcut_text("Ctrl+Z")).clicked() { ed.undo(); ui.close_menu(); }
    if ui.add_enabled(ed.hist.can_redo(), egui::Button::new("Повторить").shortcut_text("Ctrl+Shift+Z")).clicked() { ed.redo(); ui.close_menu(); }
    ui.separator();
    if ui.add(egui::Button::new("Вырезать").shortcut_text("Ctrl+X")).clicked() { cx.acts.push("cut"); ui.close_menu(); }
    if ui.add(egui::Button::new("Копировать").shortcut_text("Ctrl+C")).clicked() { cx.acts.push("copy"); ui.close_menu(); }
    if ui.add(egui::Button::new("Скопировать совмещённые").shortcut_text("Ctrl+Shift+C")).clicked() { cx.acts.push("copy_merged"); ui.close_menu(); }
    if ui.add(egui::Button::new("Вставить").shortcut_text("Ctrl+V")).on_hover_text("Картинка из буфера обмена — новым слоем").clicked() { cx.acts.push("paste"); ui.close_menu(); }
    if ui.add(egui::Button::new("Очистить").shortcut_text("Delete")).clicked() { ed.layer_clear(); ui.close_menu(); }
    if ui.button("Залить основным цветом").clicked() { let c = cx.sh.colors.fg; ed.layer_fill(c); ui.close_menu(); }
    ui.separator();
    if ui.add(egui::Button::new("Свободное трансформирование").shortcut_text("Ctrl+T")).clicked() { ed.set_tool(Tool::Transform); ui.close_menu(); }
    if ui.button("Отразить слой по горизонтали").clicked() { ed.layer_flip(true); ui.close_menu(); }
    if ui.button("Отразить слой по вертикали").clicked() { ed.layer_flip(false); ui.close_menu(); }
    if ui.button("Вставить изображение…").clicked() { cx.acts.push("insert_image"); ui.close_menu(); }
    ui.separator();
    if ui.button("Настройки…").clicked() { cx.st.settings = true; ui.close_menu(); }
}

fn image_menu(ui: &mut Ui, cx: &mut Cx) {
    let ed = &mut *cx.ed;
    ui.menu_button("Коррекция", |ui| {
        for f in FILTERS.iter().filter(|f| f.group == "Цвет") {
            if ui.button(format!("{}…", f.name)).clicked() { ed.filter_open(f.id); ui.close_menu(); }
        }
    });
    ui.separator();
    if ui.button("Размер изображения и холста…").clicked() { cx.st.resize = Some(ResizeDlg { w: ed.doc.w, h: ed.doc.h, keep: true, scale: false, anchor: (1, 1) }); ui.close_menu(); }
    if ui.button("Повернуть на 90° по часовой").clicked() { ed.rotate_canvas(true); ui.close_menu(); }
    if ui.button("Повернуть на 90° против часовой").clicked() { ed.rotate_canvas(false); ui.close_menu(); }
    if ui.button("Отразить холст по горизонтали").clicked() { ed.flip_canvas(true); ui.close_menu(); }
    if ui.button("Отразить холст по вертикали").clicked() { ed.flip_canvas(false); ui.close_menu(); }
    ui.separator();
    let mut vis = ed.doc.bg_visible;
    if ui.checkbox(&mut vis, "Показывать фон").changed() { let c = ed.doc.bg; ed.set_bg(c, vis); }
    ui.horizontal(|ui| {
        ui.label("Цвет фона");
        let mut c = ed.doc.bg;
        if ui.color_edit_button_srgb(&mut c).changed() { let v = ed.doc.bg_visible; ed.set_bg(c, v); }
    });
}

fn layer_menu(ui: &mut Ui, cx: &mut Cx) {
    let ed = &mut *cx.ed;
    let li = ed.doc.active;
    if ui.add(egui::Button::new("Новый слой").shortcut_text("Ctrl+Shift+N")).clicked() { ed.layer_add(); ui.close_menu(); }
    if ui.button("Новый векторный слой").on_hover_text("Линии остаются линиями: их можно стирать до пересечения и масштабировать без потерь").clicked() { ed.layer_add_vector(); ui.close_menu(); }
    if ed.doc.layer().is_vector() && ui.button("Растеризовать слой").clicked() { ed.layer_rasterize(); ui.close_menu(); }
    if ui.add(egui::Button::new("Дублировать слой").shortcut_text("Ctrl+J")).clicked() { ed.layer_duplicate(); ui.close_menu(); }
    if ui.button("Удалить слой").clicked() { ed.layer_delete(); ui.close_menu(); }
    if ui.button("Переименовать…").clicked() { cx.st.layer_rename = Some((li, ed.doc.layer().name.clone())); ui.close_menu(); }
    ui.separator();
    ui.menu_button("Режим наложения", |ui| {
        for (i, (b, _, n, _)) in BLENDS.iter().enumerate() {
            if GROUP_STARTS.contains(&i) { ui.separator(); }
            if ui.selectable_label(ed.doc.layer().blend == *b, *n).clicked() { let b = *b; ed.layer_props(li, |l| l.blend = b, "Режим наложения"); ui.close_menu(); }
        }
    });
    let l = ed.doc.layer();
    let (clip, al, lk, vis) = (l.clip, l.alpha_lock, l.locked, l.visible);
    if ui.selectable_label(clip, "Обтравочная маска").clicked() { ed.layer_props(li, |l| l.clip = !clip, "Обтравочная маска"); ui.close_menu(); }
    if ui.selectable_label(al, "Защитить прозрачность").clicked() { ed.layer_props(li, |l| l.alpha_lock = !al, "Защита прозрачности"); ui.close_menu(); }
    if ui.selectable_label(lk, "Закрепить слой").clicked() { ed.layer_props(li, |l| l.locked = !lk, "Блокировка слоя"); ui.close_menu(); }
    if ui.selectable_label(!vis, "Скрыть слой").clicked() { ed.layer_props(li, |l| l.visible = !vis, "Видимость слоя"); ui.close_menu(); }
    ui.separator();
    if ui.add(egui::Button::new("Объединить с нижним").shortcut_text("Ctrl+E")).clicked() { ed.merge_down(); ui.close_menu(); }
    if ui.add(egui::Button::new("Выполнить сведение").shortcut_text("Ctrl+Shift+E")).clicked() { ed.flatten_all(); ui.close_menu(); }
}

fn select_menu(ui: &mut Ui, cx: &mut Cx) {
    let ed = &mut *cx.ed;
    if ui.add(egui::Button::new("Всё").shortcut_text("Ctrl+A")).clicked() { ed.select_all(); ui.close_menu(); }
    if ui.add_enabled(ed.sel.is_some(), egui::Button::new("Отменить выделение").shortcut_text("Ctrl+D")).clicked() { ed.select_none(); ui.close_menu(); }
    if ui.add(egui::Button::new("Инверсия").shortcut_text("Ctrl+Shift+I")).clicked() { ed.select_invert(); ui.close_menu(); }
    if ui.add_enabled(ed.sel.is_some(), egui::Button::new("Растушёвка…")).clicked() { cx.st.feather = Some(8.0); ui.close_menu(); }
    if ui.button("Загрузить из слоя").clicked() { ed.select_from_layer(); ui.close_menu(); }
    ui.separator();
    if ui.add_enabled(ed.sel.is_some(), egui::Button::new("Скопировать на новый слой").shortcut_text("Ctrl+J")).clicked() { ed.copy_to_layer(false); ui.close_menu(); }
    if ui.add_enabled(ed.sel.is_some(), egui::Button::new("Вырезать на новый слой").shortcut_text("Ctrl+Shift+J")).clicked() { ed.copy_to_layer(true); ui.close_menu(); }
}

fn filter_menu(ui: &mut Ui, cx: &mut Cx) {
    for g in ["Размытие", "Эффекты", "Цвет"] {
        ui.menu_button(g, |ui| {
            for f in FILTERS.iter().filter(|f| f.group == g) {
                if ui.button(format!("{}…", f.name)).clicked() { cx.ed.filter_open(f.id); ui.close_menu(); }
            }
        });
    }
}

fn view_menu(ui: &mut Ui, cx: &mut Cx) {
    for (k, n, s) in [("zoom_in", "Увеличить", "Ctrl++"), ("zoom_out", "Уменьшить", "Ctrl+-"), ("fit", "Показать во весь экран", "Ctrl+0"), ("actual", "100%", "Ctrl+1"), ("reset_rotation", "Сбросить поворот", ""), ("flip_view", "Зеркальный вид", "F"), ("toggle_ui", "Скрыть панели", "Tab")] {
        if ui.add(egui::Button::new(n).shortcut_text(s)).clicked() { cx.acts.push(k); ui.close_menu(); }
    }
    ui.separator();
    let g = &mut cx.ed.doc.guide;
    ui.checkbox(&mut g.grid, "Сетка");
    if g.grid { ui.add(egui::Slider::new(&mut g.grid_size, 4.0..=512.0).text("Шаг сетки")); }
    ui.label("Симметрия");
    egui::ComboBox::from_id_salt("sym").selected_text(sym_name(&g.sym)).show_ui(ui, |ui| {
        for s in ["none", "vertical", "horizontal", "quadrant", "radial"] { ui.selectable_value(&mut g.sym, s.to_string(), sym_name(s)); }
    });
    if g.sym == "radial" {
        ui.add(egui::Slider::new(&mut g.segments, 2..=24).text("Сегменты"));
        ui.checkbox(&mut g.rotational, "Без зеркала");
    }
    if g.sym != "none" && ui.button("Центр симметрии — по центру холста").clicked() { g.cx = cx.ed.doc.w as f32 / 2.0; g.cy = cx.ed.doc.h as f32 / 2.0; }
}

fn sym_name(s: &str) -> &'static str {
    match s { "vertical" => "Вертикальная", "horizontal" => "Горизонтальная", "quadrant" => "Квадранты", "radial" => "Радиальная", _ => "Нет" }
}

/* ---------- панель параметров ---------- */

fn options_bar(ui: &mut Ui, cx: &mut Cx) {
    ui.horizontal(|ui| {
        let ed = &mut *cx.ed;
        ui.label(egui::RichText::new(ed.tool.name()).strong());
        ui.separator();
        match ed.tool {
            Tool::Brush | Tool::Eraser | Tool::Smudge | Tool::Clone => {
                let tool = ed.tool;
                ui.spacing_mut().slider_width = 76.0;
                if tool == Tool::Eraser && ed.doc.layer().is_vector() {
                    ui.label(egui::RichText::new("Векторный ластик:").strong());
                    vector_erase_modes(ui, &mut cx.sh.prefs.vector_erase);
                    ui.separator();
                }
                let ts = ed.tool_state_mut(cx.sh).clone();
                let b = cx.sh.brushes.get(&ts.brush_id).clone();
                if ui.button(format!("{}  ...", b.name)).on_hover_text("Выбрать кисть").clicked() { cx.st.dock_tab[1] = 0; cx.st.win.brushes = !cx.st.win.brushes; }
                let ts = ed.tool_state_mut(cx.sh);
                let (mn, mx, px) = (b.size_min, b.size_max, b.pixel);
                ui.label("Размер");
                ui.add(egui::Slider::new(&mut ts.size, 0.0..=1.0).custom_formatter(move |v, _| { let s = mn + (mx - mn) * (v * v) as f32; if px { format!("{:.0} px", s.round().max(1.0)) } else { format!("{:.1} px", s) } }));
                ui.label(if tool == Tool::Smudge { "Сила" } else { "Непрозр." });
                ui.add(egui::Slider::new(&mut ts.opacity, 0.01..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
                if tool == Tool::Brush {
                    let mut mode = Blend::from_id(&ts.mode);
                    egui::ComboBox::from_id_salt("bmode").selected_text(mode.name()).width(150.0).show_ui(ui, |ui| {
                        for (i, (bl, _, n, _)) in BLENDS.iter().enumerate() { if GROUP_STARTS.contains(&i) { ui.separator(); } ui.selectable_value(&mut mode, *bl, *n); }
                    });
                    ts.mode = mode.id().to_string();
                }
                let mut bb = b.clone();
                ui.label("Жёсткость");
                let c1 = ui.add(egui::Slider::new(&mut bb.hardness, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).changed();
                ui.label("Нажим");
                let c2 = ui.add(egui::Slider::new(&mut bb.flow, 0.01..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).changed();
                ui.label("Сглаживание");
                let c3 = ui.add(egui::Slider::new(&mut bb.streamline, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).changed();
                if tool != Tool::Smudge {
                    ui.label("Стабилизатор");
                    ui.add(egui::Slider::new(&mut cx.sh.prefs.stabilizer, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).on_hover_text("«Ленивая кисть»: линия тянется за пером на нити — ровные линии без дрожания");
                }
                let mut ps = bb.p_size > 0.0;
                ui.label("Перо:");
                let c4 = ui.toggle_value(&mut ps, "размер").on_hover_text("Нажим пера управляет размером").changed();
                if c4 { bb.p_size = if ps { 0.7 } else { 0.0 }; }
                let mut pf = bb.p_flow > 0.0;
                let c5 = ui.toggle_value(&mut pf, "плотность").on_hover_text("Нажим пера управляет плотностью").changed();
                if c5 { bb.p_flow = if pf { 0.6 } else { 0.0 }; }
                if c1 || c2 || c3 || c4 || c5 { cx.sh.brushes.update(bb); }
                if ui.button("Настройки кисти").clicked() { cx.st.dock_tab[1] = 1; cx.st.win.brushset = !cx.st.win.brushset; }
                if tool == Tool::Clone {
                    if ui.button("Задать источник").clicked() { ed.clone_src = None; ed.toast("Коснитесь холста, чтобы задать источник (Alt+щелчок)"); }
                    ui.checkbox(&mut cx.sh.prefs.clone_aligned, "Выровненный");
                }
            }
            Tool::Fill => {
                ui.label("Допуск");
                ui.add(egui::Slider::new(&mut cx.sh.prefs.fill_tol, 0.0..=100.0).custom_formatter(|v, _| format!("{v:.0}%")));
                ui.label("Расширить");
                ui.add(egui::Slider::new(&mut cx.sh.prefs.fill_expand, 0..=4).suffix(" px"));
                ui.checkbox(&mut cx.sh.prefs.fill_all_layers, "По всем слоям");
                ui.label(egui::RichText::new("Тяните вбок при заливке, чтобы менять допуск").weak());
            }
            Tool::Select => {
                for (s, ic, n) in [(SelShape::Rect, "rect", "Прямоугольник"), (SelShape::Ellipse, "ellipse", "Эллипс"), (SelShape::Lasso, "lasso", "Лассо"), (SelShape::Wand, "wand", "Волшебная палочка")] {
                    if icon_btn(ui, ic, ed.sel_shape == s, n, Vec2::new(30.0, 24.0)).clicked() { ed.sel_shape = s; }
                }
                ui.separator();
                for (o, n) in [(SelOp::Replace, "Новое"), (SelOp::Add, "Добавить"), (SelOp::Subtract, "Вычесть"), (SelOp::Intersect, "Пересечь")] { ui.selectable_value(&mut ed.sel_op, o, n); }
                if ed.sel_shape == SelShape::Wand { ui.label("Допуск"); ui.add(egui::Slider::new(&mut ed.wand_tol, 0.0..=100.0)); ui.checkbox(&mut cx.sh.prefs.fill_all_layers, "По всем слоям"); }
                ui.separator();
                if ui.button("Инверсия").clicked() { ed.select_invert(); }
                if ui.button("Растушевать").clicked() { cx.st.feather = Some(8.0); }
                if ui.button("На новый слой").clicked() { ed.copy_to_layer(false); }
                if ui.button("Залить").clicked() { let c = cx.sh.colors.fg; ed.layer_fill(c); }
                if ui.button("Очистить").clicked() { ed.layer_clear(); }
                if ui.add_enabled(ed.sel.is_some(), egui::Button::new("Снять")).clicked() { ed.select_none(); }
            }
            Tool::Transform => {
                if let Some(x) = &ed.xform {
                    let (u, s) = (x.uniform, x.smooth);
                    if ui.selectable_label(u, "Пропорционально").clicked() { ed.xform_act("uniform"); }
                    if ui.button("Отразить по горизонтали").clicked() { ed.xform_act("flipH"); }
                    if ui.button("Отразить по вертикали").clicked() { ed.xform_act("flipV"); }
                    if ui.button("Повернуть на 45°").clicked() { ed.xform_act("rot45"); }
                    if ui.button("По размеру холста").clicked() { ed.xform_act("fit"); }
                    if ui.selectable_label(s, "Сглаживание").clicked() { ed.xform_act("smooth"); }
                    if ui.button("Сброс").clicked() { ed.xform_act("reset"); }
                    ui.separator();
                    if ui.button("Отмена").clicked() { cx.acts.push("escape"); }
                    if ui.add(egui::Button::new(egui::RichText::new("Готово").color(Color32::WHITE)).fill(ACCENT)).clicked() { cx.acts.push("enter"); }
                } else if ui.button("Начать трансформацию").clicked() { ed.xform_begin(); }
            }
            Tool::Gradient => {
                for (k, n) in [(GradKind::Linear, "Линейный"), (GradKind::Radial, "Радиальный"), (GradKind::Reflected, "Зеркальный"), (GradKind::Angle, "Угловой")] { ui.selectable_value(&mut ed.grad_kind, k, n); }
                let presets = Editor::grad_presets();
                egui::ComboBox::from_id_salt("gcol").selected_text(presets[ed.grad_colors.min(presets.len() - 1)]).show_ui(ui, |ui| {
                    for (i, n) in presets.iter().enumerate() { ui.selectable_value(&mut ed.grad_colors, i, *n); }
                });
                ui.checkbox(&mut ed.grad_reverse, "Инверсия");
                ui.label("Непрозр.");
                ui.add(egui::Slider::new(&mut ed.grad_opacity, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
            }
            Tool::Shapes => {
                for (k, n) in [(ShapeKind::Rect, "Прямоугольник"), (ShapeKind::Ellipse, "Эллипс"), (ShapeKind::Line, "Линия"), (ShapeKind::Polygon, "Многоугольник"), (ShapeKind::Star, "Звезда")] { ui.selectable_value(&mut ed.shape_kind, k, n); }
                ui.separator();
                if ed.shape_kind != ShapeKind::Line { ui.checkbox(&mut ed.shape_fill, "Заливка"); ui.checkbox(&mut ed.shape_stroke, "Обводка"); }
                ui.label("Толщина");
                ui.add(egui::Slider::new(&mut ed.shape_width, 1.0..=200.0).suffix(" px"));
                if matches!(ed.shape_kind, ShapeKind::Polygon | ShapeKind::Star) { ui.label("Углов"); ui.add(egui::Slider::new(&mut ed.shape_sides, 3..=16)); }
            }
            Tool::Picker => { ui.checkbox(&mut cx.sh.prefs.pick_layer, "Только текущий слой"); }
            Tool::Text => { ui.label(egui::RichText::new("Щёлкните по холсту, чтобы добавить текст; по тексту на текстовом слое — чтобы изменить").weak()); }
            Tool::Hand | Tool::Zoom => {
                for (k, n) in [("fit", "Вписать"), ("actual", "100%"), ("reset_rotation", "Сбросить поворот"), ("flip_view", "Зеркальный вид")] { if ui.button(n).clicked() { cx.acts.push(k); } }
            }
        }
    });
}

fn vector_erase_modes(ui: &mut Ui, mode: &mut String) {
    for (m, tip) in [(EraseMode::Touch, "Стирает только там, где прошёл ластик"), (EraseMode::Whole, "Удаляет всю задетую линию"), (EraseMode::Intersect, "Стирает часть линии до ближайших пересечений с другими линиями")] {
        if ui.selectable_label(*mode == m.id(), m.name()).on_hover_text(tip).clicked() { *mode = m.id().into(); }
    }
}

/* ---------- панель инструментов ---------- */

const TOOLS: [(Tool, Option<SelShape>, &str, &str); 19] = [
    (Tool::Transform, None, "move", "Перемещение и трансформация (V)"), (Tool::Select, Some(SelShape::Rect), "rect", "Прямоугольное выделение (M)"),
    (Tool::Select, Some(SelShape::Ellipse), "ellipse", "Овальное выделение (M)"), (Tool::Select, Some(SelShape::Lasso), "lasso", "Лассо (L)"),
    (Tool::Select, Some(SelShape::Wand), "wand", "Волшебная палочка (W)"), (Tool::Brush, None, "brush", "Кисть (B)"), (Tool::Eraser, None, "eraser", "Ластик (E)"),
    (Tool::Smudge, None, "smudge", "Палец (R)"), (Tool::Clone, None, "clone", "Штамп (S)"), (Tool::Fill, None, "fill", "Заливка (G)"),
    (Tool::Gradient, None, "gradient", "Градиент (Shift+G)"), (Tool::Text, None, "text", "Текст (T)"), (Tool::Shapes, None, "shapes", "Фигуры (U)"),
    (Tool::Picker, None, "picker", "Пипетка (I, Alt, правая кнопка, удержание)"), (Tool::Hand, None, "hand", "Рука (H, пробел)"), (Tool::Zoom, None, "zoom", "Масштаб (Z)"),
    (Tool::Brush, None, "", ""), (Tool::Brush, None, "", ""), (Tool::Brush, None, "", ""),
];

fn tools_column(ui: &mut Ui, cx: &mut Cx, size: f32) {
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
        for (i, (t, s, ic, tip)) in TOOLS.iter().enumerate() {
            if ic.is_empty() { continue; }
            if i == 5 || i == 11 || i == 14 { ui.separator(); }
            let sel = cx.ed.tool == *t && s.map_or(true, |s| cx.ed.sel_shape == s);
            if icon_btn(ui, ic, sel, tip, Vec2::new(size, size * 0.86)).clicked() {
                if let Some(s) = s { cx.ed.sel_shape = *s; }
                if cx.ed.tool == *t && t.is_paint() { cx.st.dock_tab[1] = 0; cx.st.win.brushes = !cx.st.win.brushes; }
                cx.ed.set_tool(*t);
            }
        }
        ui.add_space(8.0);
        let (r, resp) = ui.allocate_exact_size(Vec2::new(size, size * 1.1), Sense::click());
        let fg = cx.sh.colors.fg;
        let bg = cx.sh.colors.bg;
        let s = size * 0.55;
        let bgr = Rect::from_min_size(r.min + Vec2::new(size * 0.38, size * 0.45), Vec2::splat(s));
        let fgr = Rect::from_min_size(r.min + Vec2::new(size * 0.08, size * 0.12), Vec2::splat(s));
        ui.painter().rect_filled(bgr, 2.0, Color32::from_rgb(bg[0], bg[1], bg[2]));
        ui.painter().rect_stroke(bgr, 2.0, EStroke::new(1.0, Color32::GRAY), egui::StrokeKind::Outside);
        ui.painter().rect_filled(fgr, 2.0, Color32::from_rgb(fg[0], fg[1], fg[2]));
        ui.painter().rect_stroke(fgr, 2.0, EStroke::new(1.0, Color32::GRAY), egui::StrokeKind::Outside);
        if resp.clicked() {
            let p = resp.interact_pointer_pos().unwrap_or(r.center());
            if bgr.contains(p) && !fgr.contains(p) { cx.sh.colors.swap(); } else { cx.st.dock_tab[0] = 0; cx.st.win.color = !cx.st.win.color; }
        }
        resp.on_hover_text("Основной и фоновый цвета (X — поменять, D — чёрный/белый)");
    });
}

/* ---------- панели дока ---------- */

fn color_panel(ui: &mut Ui, cx: &mut Cx) {
    let fg = cx.sh.colors.fg;
    let mut hsva = match cx.st.hsva {
        Some((h, c)) if c == fg => h,
        _ => egui::ecolor::Hsva::from(Color32::from_rgb(fg[0], fg[1], fg[2])),
    };
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::new(40.0, 26.0), Sense::hover());
        ui.painter().rect_filled(r, 4.0, Color32::from_rgb(fg[0], fg[1], fg[2]));
        let bg = cx.sh.colors.bg;
        if ui.add(egui::Button::new("").fill(Color32::from_rgb(bg[0], bg[1], bg[2])).min_size(Vec2::new(40.0, 26.0))).on_hover_text("Фоновый цвет — щёлкните, чтобы поменять местами").clicked() { cx.sh.colors.swap(); }
        if cx.st.hex.is_empty() || !ui.memory(|m| m.has_focus(egui::Id::new("hexedit"))) { cx.st.hex = util::hex(fg); }
        let r = ui.add(egui::TextEdit::singleline(&mut cx.st.hex).id(egui::Id::new("hexedit")).desired_width(80.0));
        if r.lost_focus() { if let Some(c) = util::parse_hex(&cx.st.hex) { cx.sh.colors.set(c); } }
    });
    if egui::color_picker::color_picker_hsva_2d(ui, &mut hsva, egui::color_picker::Alpha::Opaque) {
        let c = Color32::from(hsva);
        let rgb = [c.r(), c.g(), c.b()];
        cx.sh.colors.set(rgb);
        cx.st.hsva = Some((hsva, rgb));
    }
    if !cx.sh.colors.recent.is_empty() {
        ui.label(egui::RichText::new("Недавние").weak());
        let recent = cx.sh.colors.recent.clone();
        swatch_row(ui, &recent, |c| cx.sh.colors.set(c));
    }
    if !cx.sh.palettes.is_empty() {
        let pi = cx.sh.palette.min(cx.sh.palettes.len() - 1);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Палитра").weak());
            egui::ComboBox::from_id_salt("cpal").selected_text(cx.sh.palettes[pi].name.clone()).width(160.0).show_ui(ui, |ui| {
                for i in 0..cx.sh.palettes.len() { let n = cx.sh.palettes[i].name.clone(); ui.selectable_value(&mut cx.sh.palette, i, n); }
            });
            if ui.small_button("+").on_hover_text("Добавить текущий цвет в палитру").clicked() { let c = cx.sh.colors.fg; let p = &mut cx.sh.palettes[pi]; p.colors.push(c); p.names.push(String::new()); }
        });
        let p = cx.sh.palettes[pi].clone();
        palette_grid(ui, cx, pi, &p);
    }
}

fn swatch_row(ui: &mut Ui, cols: &[[u8; 3]], mut pick: impl FnMut([u8; 3])) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(3.0);
        for c in cols {
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
            ui.painter().rect_filled(r, 3.0, Color32::from_rgb(c[0], c[1], c[2]));
            if resp.clicked() { pick(*c); }
        }
    });
}

/// Сетка образцов палитры: щелчок — выбрать цвет, правая кнопка — меню образца.
fn palette_grid(ui: &mut Ui, cx: &mut Cx, pi: usize, p: &crate::state::Palette) {
    let mut remove = None;
    let mut replace = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(3.0);
        for (i, c) in p.colors.iter().enumerate() {
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
            ui.painter().rect_filled(r, 3.0, Color32::from_rgb(c[0], c[1], c[2]));
            if *c == cx.sh.colors.fg { ui.painter().rect_stroke(r, 3.0, EStroke::new(2.0, ACCENT), egui::StrokeKind::Outside); }
            let name = p.names.get(i).filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| util::hex(*c));
            let resp = resp.on_hover_text(name);
            if resp.clicked() { cx.sh.colors.set(*c); }
            resp.context_menu(|ui| {
                if ui.button("Заменить текущим цветом").clicked() { replace = Some(i); ui.close_menu(); }
                if ui.button("Удалить образец").clicked() { remove = Some(i); ui.close_menu(); }
            });
        }
        if p.colors.is_empty() { ui.label(egui::RichText::new("Палитра пуста — нажмите «+», чтобы добавить текущий цвет").weak()); }
    });
    let pal = &mut cx.sh.palettes[pi];
    if let Some(i) = replace { pal.colors[i] = cx.sh.colors.fg; }
    if let Some(i) = remove { pal.colors.remove(i); if i < pal.names.len() { pal.names.remove(i); } }
}

fn swatches_panel(ui: &mut Ui, cx: &mut Cx) {
    if cx.sh.palettes.is_empty() { cx.sh.palettes = crate::state::default_palettes(); }
    let pi = cx.sh.palette.min(cx.sh.palettes.len() - 1);
    cx.sh.palette = pi;
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("spal").selected_text(cx.sh.palettes[pi].name.clone()).width(170.0).show_ui(ui, |ui| {
            for i in 0..cx.sh.palettes.len() { let n = format!("{}  ({})", cx.sh.palettes[i].name, cx.sh.palettes[i].colors.len()); ui.selectable_value(&mut cx.sh.palette, i, n); }
        });
        if ui.small_button("+").on_hover_text("Добавить текущий цвет").clicked() { let c = cx.sh.colors.fg; let p = &mut cx.sh.palettes[pi]; p.colors.push(c); p.names.push(String::new()); }
    });
    let pi = cx.sh.palette.min(cx.sh.palettes.len() - 1);
    ui.horizontal(|ui| {
        ui.label("Название");
        let mut n = cx.sh.palettes[pi].name.clone();
        if ui.add(egui::TextEdit::singleline(&mut n).desired_width(170.0)).changed() { cx.sh.palettes[pi].name = n; }
    });
    let p = cx.sh.palettes[pi].clone();
    palette_grid(ui, cx, pi, &p);
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if ui.button("Новая").clicked() {
            let k = cx.sh.palettes.len() + 1;
            cx.sh.palettes.push(crate::state::Palette { name: format!("Палитра {k}"), ..Default::default() });
            cx.sh.palette = cx.sh.palettes.len() - 1;
        }
        if ui.button("Импорт…").on_hover_text("Photoshop .aco/.ase, GIMP/Krita .gpl, .pal, Procreate .swatches, список HEX").clicked() { cx.acts.push("import_palette"); }
        ui.menu_button("Экспорт", |ui| {
            for (k, n) in [("aco", "Photoshop (.aco)"), ("ase", "Adobe Swatch Exchange (.ase)"), ("gpl", "GIMP / Krita (.gpl)"), ("swatches", "Procreate (.swatches)"), ("hex", "Список HEX (.hex)")] {
                if ui.button(n).clicked() { cx.st.palette_export = Some((pi, k)); ui.close_menu(); }
            }
        });
        if ui.button("Из холста").on_hover_text("Собрать основные цвета рисунка").clicked() {
            let cols = palette_from(&crate::doc::flatten(&cx.ed.doc, true));
            let n = cols.len();
            cx.sh.palettes.push(crate::state::Palette { name: format!("Из «{}»", cx.ed.doc.name), colors: cols, names: vec![String::new(); n] });
            cx.sh.palette = cx.sh.palettes.len() - 1;
        }
        if ui.button("Недавние в палитру").clicked() {
            let cols = cx.sh.colors.recent.clone();
            let n = cols.len();
            cx.sh.palettes.push(crate::state::Palette { name: "Недавние цвета".into(), colors: cols, names: vec![String::new(); n] });
            cx.sh.palette = cx.sh.palettes.len() - 1;
        }
        if ui.button("Дублировать").clicked() {
            let mut p = cx.sh.palettes[pi].clone();
            p.name = format!("{} (копия)", p.name);
            cx.sh.palettes.push(p);
            cx.sh.palette = cx.sh.palettes.len() - 1;
        }
        if ui.button("Удалить").clicked() {
            cx.sh.palettes.remove(pi);
            if cx.sh.palettes.is_empty() { cx.sh.palettes = crate::state::default_palettes(); }
            cx.sh.palette = pi.min(cx.sh.palettes.len() - 1);
        }
    });
}
fn palette_from(img: &Img) -> Vec<[u8; 3]> {
    let small = img.resize(72, 72);
    let px: Vec<[f32; 3]> = small.px.chunks_exact(4).filter(|p| p[3] > 128).map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
    if px.is_empty() { return vec![]; }
    let dist = |a: &[f32; 3], b: &[f32; 3]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);
    let mut cen = vec![px[px.len() / 2]];
    let mut md: Vec<f32> = px.iter().map(|p| dist(p, &cen[0])).collect();
    while cen.len() < 15 {
        let (bi, bv) = md.iter().enumerate().fold((0, 0.0), |(bi, bv), (i, v)| if *v > bv { (i, *v) } else { (bi, bv) });
        if bv < 120.0 { break; }
        cen.push(px[bi]);
        for (i, p) in px.iter().enumerate() { md[i] = md[i].min(dist(p, &px[bi])); }
    }
    for _ in 0..8 {
        let mut sum = vec![[0f32; 4]; cen.len()];
        for p in &px {
            let j = (0..cen.len()).min_by(|&a, &b| dist(p, &cen[a]).partial_cmp(&dist(p, &cen[b])).unwrap()).unwrap();
            for k in 0..3 { sum[j][k] += p[k]; }
            sum[j][3] += 1.0;
        }
        for (j, s) in sum.iter().enumerate() { if s[3] > 0.0 { cen[j] = [s[0] / s[3], s[1] / s[3], s[2] / s[3]]; } }
    }
    let mut out: Vec<[u8; 3]> = cen.iter().map(|c| [c[0] as u8, c[1] as u8, c[2] as u8]).collect();
    out.sort_by(|a, b| util::luma(*a).partial_cmp(&util::luma(*b)).unwrap());
    out.dedup();
    out
}

fn brushes_panel(ui: &mut Ui, cx: &mut Cx) {
    let tool = cx.ed.tool;
    let cur = editor::tool_state(cx.sh, tool).brush_id.clone();
    if cx.st.brush_cat.is_empty() { cx.st.brush_cat = cx.sh.brushes.get(&cur).cat.clone(); }
    ui.horizontal(|ui| {
        if ui.small_button("+ Новая").clicked() { let id = cx.sh.brushes.create(); cx.st.brush_cat = "custom".into(); pick_brush(cx, &id); cx.st.dock_tab[1] = 1; }
        if ui.small_button("Дублировать").clicked() { let id = cx.sh.brushes.duplicate(&cur); cx.st.brush_cat = "custom".into(); pick_brush(cx, &id); }
        if ui.small_button("Импорт…").on_hover_text("Кисти Photoshop (.abr) и Clip Studio Paint (.sut). Можно просто перетащить файл в окно").clicked() { cx.acts.push("import_brushes"); }
    });
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(86.0);
            for (id, n) in CATS { if ui.selectable_label(cx.st.brush_cat == id, n).clicked() { cx.st.brush_cat = id.to_string(); } }
        });
        ui.separator();
        ui.vertical(|ui| {
            let list: Vec<Brush> = cx.sh.brushes.list(&cx.st.brush_cat).into_iter().cloned().collect();
            if list.is_empty() { ui.label(egui::RichText::new(if cx.st.brush_cat == "imported" { "Импортируйте кисти Photoshop (.abr) или Clip Studio (.sut) кнопкой «Импорт…»" } else { "Здесь появятся ваши кисти" }).weak()); }
            let col = ui.visuals().text_color();
            let tcol = [col.r(), col.g(), col.b()];
            let ver = cx.sh.brushes.version;
            let ctx = ui.ctx().clone();
            for b in list {
                let selected = b.id == cur;
                let t = texture(&ctx, cx.tex, format!("b:{}", b.id), ver ^ (tcol[0] as u64) << 40, || brush_sample(&b, 200, 36, tcol));
                let resp = ui.add(egui::Button::new(egui::RichText::new(format!("{}{}", b.name, if cx.sh.brushes.is_modified(&b.id) { " *" } else { "" })).size(12.0)).selected(selected).min_size(Vec2::new(ui.available_width(), 18.0)));
                let img_resp = ui.add(egui::Image::new(&t).fit_to_exact_size(Vec2::new(ui.available_width().min(200.0), 36.0)).sense(Sense::click()));
                if resp.clicked() || img_resp.clicked() { pick_brush(cx, &b.id); }
            }
        });
    });
}

fn pick_brush(cx: &mut Cx, id: &str) {
    let b = cx.sh.brushes.get(id).clone();
    let ts = cx.ed.tool_state_mut(cx.sh);
    ts.brush_id = b.id.clone();
    ts.size = b.size;
    if cx.ed.tool == Tool::Brush { ts.opacity = b.opacity; }
}

const BSET: [&str; 9] = ["Форма отпечатка", "Динамика формы", "Рассеивание", "Текстура", "Передача", "Динамика цвета", "Сглаживание и сужение", "Перо и нажим", "Свойства"];

fn brushset_panel(ui: &mut Ui, cx: &mut Cx) {
    let id = editor::tool_state(cx.sh, cx.ed.tool).brush_id.clone();
    let mut b = cx.sh.brushes.get(&id).clone();
    let orig = b.clone();
    ui.horizontal(|ui| {
        ui.strong(&b.name);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if b.preset && cx.sh.brushes.is_modified(&id) && ui.small_button("Сбросить").clicked() { cx.sh.brushes.reset(&id); }
            if !b.preset && ui.small_button("Удалить").clicked() { cx.sh.brushes.remove(&id); let ts = cx.ed.tool_state_mut(cx.sh); ts.brush_id = "pencil_hb".into(); }
        });
    });
    let fg = cx.sh.colors.fg;
    let ver = cx.sh.brushes.version;
    let t = texture(ui.ctx(), cx.tex, format!("bs:{id}"), ver ^ ((fg[0] as u64) << 32 | (fg[1] as u64) << 40 | (fg[2] as u64) << 48), || {
        let mut s = brush_sample(&b, 260, 56, fg);
        let mut bg = Img::filled(s.w, s.h, [255, 255, 255, 255]);
        bg.draw_over(0, 0, &s, 1.0);
        s = bg;
        s
    });
    ui.add(egui::Image::new(&t).fit_to_exact_size(Vec2::new(ui.available_width().min(280.0), 56.0)));
    for (si, title) in BSET.iter().enumerate() {
        egui::CollapsingHeader::new(egui::RichText::new(*title).strong()).id_salt(("bset", si)).default_open(cx.st.bset_open[si]).show(ui, |ui| {
            match si {
                0 => {
                    let custom = b.shape.starts_with("img:");
                    let orig_shape = orig.shape.clone();
                    egui::ComboBox::from_id_salt("shape").selected_text(if custom { "Своя форма" } else { SHAPES.iter().find(|s| s.0 == b.shape).map_or("Круглая", |s| s.1) }).show_ui(ui, |ui| {
                        if orig_shape.starts_with("img:") { ui.selectable_value(&mut b.shape, orig_shape.clone(), "Своя форма"); }
                        for (sid, n) in SHAPES { ui.selectable_value(&mut b.shape, sid.to_string(), n); }
                    });
                    if ui.button("Форма из картинки…").on_hover_text("Чёрное на белом или фигура на прозрачном фоне").clicked() { cx.acts.push("tip_image"); }
                    pslider(ui, "Жёсткость", &mut b.hardness, 0.0, 1.0, true);
                    pslider(ui, "Интервал", &mut b.spacing, 0.01, 2.5, true);
                    pslider(ui, "Округлость", &mut b.roundness, 0.02, 1.0, true);
                    pslider(ui, "Угол, °", &mut b.angle, -180.0, 180.0, false);
                    ui.checkbox(&mut b.follow, "Поворачивать по направлению штриха");
                }
                1 => {
                    pslider(ui, "Колебание размера", &mut b.j_size, 0.0, 1.0, true);
                    pslider(ui, "Нажим: размер", &mut b.p_size, 0.0, 1.0, true);
                    pslider(ui, "Минимальный диаметр", &mut b.min_size, 0.0, 1.0, true);
                    pslider(ui, "Наклон: размер", &mut b.tilt_size, 0.0, 4.0, false);
                    pslider(ui, "Колебание угла", &mut b.j_angle, 0.0, 1.0, true);
                }
                2 => {
                    pslider(ui, "Рассеивание", &mut b.scatter, 0.0, 3.0, true);
                    pslider(ui, "Количество", &mut b.count, 1.0, 8.0, false);
                }
                3 => {
                    let mut g = b.grain.clone().unwrap_or_default();
                    let og = orig.grain.clone().unwrap_or_default();
                    egui::ComboBox::from_id_salt("grain").selected_text(if g.starts_with("img:") { "Своя текстура" } else { GRAINS.iter().find(|x| x.0 == g).map_or("Без текстуры", |x| x.1) }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut g, String::new(), "Без текстуры");
                        if og.starts_with("img:") { ui.selectable_value(&mut g, og.clone(), "Своя текстура"); }
                        for (gid, n) in GRAINS { ui.selectable_value(&mut g, gid.to_string(), n); }
                    });
                    if ui.button("Текстура из картинки…").clicked() { cx.acts.push("grain_image"); }
                    b.grain = if g.is_empty() { None } else { Some(g) };
                    pslider(ui, "Масштаб", &mut b.grain_scale, 0.2, 4.0, false);
                    pslider(ui, "Глубина", &mut b.grain_strength, 0.0, 1.0, true);
                }
                4 => {
                    pslider(ui, "Нажим (поток)", &mut b.flow, 0.01, 1.0, true);
                    pslider(ui, "Нажим пера: плотность", &mut b.p_flow, 0.0, 1.0, true);
                    pslider(ui, "Колебание непрозрачности", &mut b.j_opacity, 0.0, 1.0, true);
                    pslider(ui, "Непрозрачность по умолчанию", &mut b.opacity, 0.01, 1.0, true);
                }
                5 => {
                    pslider(ui, "Колебание тона", &mut b.j_hue, 0.0, 1.0, true);
                    pslider(ui, "Колебание насыщенности", &mut b.j_sat, 0.0, 1.0, true);
                    pslider(ui, "Колебание яркости", &mut b.j_bri, 0.0, 1.0, true);
                }
                6 => {
                    pslider(ui, "Сглаживание линии", &mut b.streamline, 0.0, 1.0, true);
                    pslider(ui, "Сужение в начале", &mut b.taper_start, 0.0, 30.0, false);
                    pslider(ui, "Сужение в конце", &mut b.taper_end, 0.0, 30.0, false);
                    pslider(ui, "Толщина на концах", &mut b.taper_min, 0.0, 1.0, true);
                    pslider(ui, "Стабилизатор (общий)", &mut cx.sh.prefs.stabilizer, 0.0, 1.0, true);
                    pslider(ui, "Коррекция после штриха", &mut cx.sh.prefs.post_smooth, 0.0, 1.0, true);
                }
                7 => {
                    pslider(ui, "Нажим: размер", &mut b.p_size, 0.0, 1.0, true);
                    pslider(ui, "Нажим: плотность", &mut b.p_flow, 0.0, 1.0, true);
                    pslider(ui, "Кривая нажима (общая)", &mut cx.sh.prefs.pressure_gamma, 0.4, 2.5, false);
                }
                _ => {
                    ui.horizontal(|ui| { ui.label("Название"); ui.text_edit_singleline(&mut b.name); });
                    pslider(ui, "Мин. размер, px", &mut b.size_min, 0.5, 50.0, false);
                    pslider(ui, "Макс. размер, px", &mut b.size_max, 1.0, 1000.0, false);
                    pslider(ui, "Сила размазывания", &mut b.smudge, 0.0, 1.0, true);
                    let mut glow = b.blend.as_deref() == Some("lighter");
                    if ui.checkbox(&mut glow, "Режим «Добавление» (свечение)").changed() { b.blend = if glow { Some("lighter".into()) } else { None }; }
                    ui.checkbox(&mut b.pixel, "Пиксельная кисть");
                }
            }
        });
    }
    if b != orig { cx.sh.brushes.update(b); }
}

fn layers_panel(ui: &mut Ui, cx: &mut Cx) {
    let ed = &mut *cx.ed;
    let li = ed.doc.active;
    let l = ed.doc.layer().clone();
    ui.horizontal(|ui| {
        let mut b = l.blend;
        egui::ComboBox::from_id_salt("lblend").selected_text(b.name()).width(150.0).show_ui(ui, |ui| {
            for (i, (bl, _, n, _)) in BLENDS.iter().enumerate() { if GROUP_STARTS.contains(&i) { ui.separator(); } ui.selectable_value(&mut b, *bl, *n); }
        });
        if b != l.blend { ed.layer_props(li, |x| x.blend = b, "Режим наложения"); }
    });
    ui.horizontal(|ui| {
        ui.label("Непрозр.");
        let mut op = l.opacity;
        let r = ui.add(egui::Slider::new(&mut op, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
        if r.drag_started() || (r.changed() && cx.st.op_before.is_none()) { cx.st.op_before = Some((li, l.props())); }
        if r.changed() { ed.layer_props_live(li, |x| x.opacity = op); }
        if (r.drag_stopped() || (r.changed() && !r.dragged())) && cx.st.op_before.is_some() {
            if let Some((i, before)) = cx.st.op_before.take() { ed.layer_props_commit(i, before, "Непрозрачность слоя"); }
        }
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Закрепить:").weak());
        if icon_btn(ui, "alphalock", l.alpha_lock, "Защита прозрачности", Vec2::new(26.0, 22.0)).clicked() { ed.layer_props(li, |x| x.alpha_lock = !l.alpha_lock, "Защита прозрачности"); }
        if icon_btn(ui, "lock", l.locked, "Блокировка слоя", Vec2::new(26.0, 22.0)).clicked() { ed.layer_props(li, |x| x.locked = !l.locked, "Блокировка слоя"); }
        if icon_btn(ui, "clip", l.clip, "Обтравочная маска", Vec2::new(26.0, 22.0)).clicked() { ed.layer_props(li, |x| x.clip = !l.clip, "Обтравочная маска"); }
        ui.label(egui::RichText::new(format!("{}/{}", ed.doc.layers.len(), ed.doc.max_layers())).weak());
    });
    ui.separator();
    let n = ed.doc.layers.len();
    let ctx = ui.ctx().clone();
    let mut select = None;
    let mut toggle_vis = None;
    let mut mv = None;
    for p in 0..n {
        let i = n - 1 - p;
        let layer = &ed.doc.layers[i];
        let t = texture(&ctx, cx.tex, format!("l:{}", layer.id), layer.ver, || {
            let th = layer.img.thumbnail(56);
            let mut bg = Img::filled(th.w, th.h, [255, 255, 255, 255]);
            for y in 0..th.h { for x in 0..th.w { if (x / 5 + y / 5) % 2 == 1 { let k = bg.idx(x, y); bg.px[k..k + 3].copy_from_slice(&[214, 214, 214]); } } }
            bg.draw_over(0, 0, &th, 1.0);
            bg
        });
        let active = i == li;
        let frame = egui::Frame::default().inner_margin(3.0).corner_radius(4.0).fill(if active { ACCENT.gamma_multiply(0.35) } else { Color32::TRANSPARENT });
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                if layer.clip && i > 0 {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(18.0, 18.0), Sense::hover());
                    paint_icon(ui.painter(), r, "clip", ui.visuals().weak_text_color());
                }
                let vis = layer.visible;
                if icon_btn(ui, if vis { "eye" } else { "more" }, false, "Видимость", Vec2::new(22.0, 22.0)).clicked() { toggle_vis = Some(i); }
                let ir = ui.add(egui::Image::new(&t).fit_to_exact_size(Vec2::new(44.0, 34.0)).sense(Sense::click()));
                let mut sub = vec![];
                if layer.blend != Blend::Normal { sub.push(layer.blend.name().to_string()); }
                if layer.opacity < 1.0 { sub.push(format!("{:.0}%", layer.opacity * 100.0)); }
                if layer.text.is_some() { sub.push("текст".into()); }
                if let Some(v) = &layer.vector { sub.push(format!("вектор · {} линий", v.len())); }
                if layer.locked { sub.push("закреплён".into()); }
                let lr = ui.add(egui::Label::new(egui::RichText::new(format!("{}{}", layer.name, if sub.is_empty() { String::new() } else { format!("\n{}", sub.join(" · ")) })).size(12.5)).sense(Sense::click()).truncate());
                if ir.clicked() || lr.clicked() { select = Some(i); }
                if lr.double_clicked() { cx.st.layer_rename = Some((i, layer.name.clone())); }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if i > 0 && icon_btn(ui, "down", false, "Ниже", Vec2::new(20.0, 20.0)).clicked() { mv = Some((i, i - 1)); }
                    if i + 1 < n && icon_btn(ui, "up", false, "Выше", Vec2::new(20.0, 20.0)).clicked() { mv = Some((i, i + 1)); }
                });
            });
        });
    }
    if let Some(i) = select { ed.layer_select(i); }
    if let Some(i) = toggle_vis { let v = ed.doc.layers[i].visible; ed.layer_props(i, |x| x.visible = !v, "Видимость слоя"); }
    if let Some((a, b)) = mv { ed.layer_move(a, b); }
    ui.horizontal(|ui| {
        let mut c = ed.doc.bg;
        let mut vis = ed.doc.bg_visible;
        if ui.color_edit_button_srgb(&mut c).changed() { ed.set_bg(c, vis); }
        ui.label("Цвет фона");
        if ui.checkbox(&mut vis, "").on_hover_text("Показывать фон").changed() { let c = ed.doc.bg; ed.set_bg(c, vis); }
    });
    ui.separator();
    ui.horizontal(|ui| {
        let s = Vec2::new(26.0, 24.0);
        if icon_btn(ui, "plus", false, "Новый слой", s).clicked() { ed.layer_add(); }
        if icon_btn(ui, "vector", false, "Новый векторный слой", s).clicked() { ed.layer_add_vector(); }
        if icon_btn(ui, "copy", false, "Дублировать", s).clicked() { ed.layer_duplicate(); }
        if icon_btn(ui, "merge", false, "Объединить с нижним", s).clicked() { ed.merge_down(); }
        if icon_btn(ui, "trash", false, "Удалить слой", s).clicked() { ed.layer_delete(); }
        ui.menu_button("Ещё", |ui| {
            if ui.button("Переименовать…").clicked() { cx.st.layer_rename = Some((li, ed.doc.layer().name.clone())); ui.close_menu(); }
            if ui.button("Очистить").clicked() { ed.layer_clear(); ui.close_menu(); }
            if ui.button("Залить основным цветом").clicked() { let c = cx.sh.colors.fg; ed.layer_fill(c); ui.close_menu(); }
            if ui.button("Выделить по содержимому").clicked() { ed.select_from_layer(); ui.close_menu(); }
            if ui.button("Отразить по горизонтали").clicked() { ed.layer_flip(true); ui.close_menu(); }
            if ui.button("Отразить по вертикали").clicked() { ed.layer_flip(false); ui.close_menu(); }
            if ed.doc.layer().is_vector() && ui.button("Растеризовать").clicked() { ed.layer_rasterize(); ui.close_menu(); }
            let r = ed.doc.layer().reference;
            if ui.selectable_label(r, "Референс для заливки").clicked() { ed.layer_props(li, |x| x.reference = !r, "Референс"); ui.close_menu(); }
            if ui.button("Свести изображение").clicked() { ed.flatten_all(); ui.close_menu(); }
        });
    });
}

fn history_panel(ui: &mut Ui, cx: &mut Cx) {
    let ed = &mut *cx.ed;
    let mut goto = None;
    if ui.selectable_label(ed.hist.pos == 0, "Исходное состояние").clicked() { goto = Some(0); }
    for (i, e) in ed.hist.list.iter().enumerate() {
        let txt = egui::RichText::new(&e.name);
        let txt = if i + 1 > ed.hist.pos { txt.weak() } else { txt };
        if ui.selectable_label(ed.hist.pos == i + 1, txt).clicked() { goto = Some(i + 1); }
    }
    if let Some(i) = goto { ed.history_goto(i); }
}

fn panel_by(ui: &mut Ui, cx: &mut Cx, group: usize, tab: usize) {
    match (group, tab) {
        (0, 0) => color_panel(ui, cx),
        (0, _) => swatches_panel(ui, cx),
        (1, 0) => brushes_panel(ui, cx),
        (1, _) => brushset_panel(ui, cx),
        (2, 0) => layers_panel(ui, cx),
        _ => history_panel(ui, cx),
    }
}

fn dock(ui: &mut Ui, cx: &mut Cx) {
    let total = ui.available_height();
    let tabs = [["Цвет", "Образцы"], ["Кисти", "Настройки кисти"], ["Слои", "История"]];
    let heights = [total * 0.36, total * 0.30, total * 0.34];
    for g in 0..3 {
        ui.horizontal(|ui| {
            for (t, n) in tabs[g].iter().enumerate() {
                if ui.selectable_label(cx.st.dock_tab[g] == t, egui::RichText::new(*n).strong()).clicked() { cx.st.dock_tab[g] = t; }
            }
        });
        ui.separator();
        let h = heights[g] - 34.0;
        egui::ScrollArea::vertical().id_salt(("dock", g)).max_height(h).min_scrolled_height(h).auto_shrink([false, false]).show(ui, |ui| {
            let t = cx.st.dock_tab[g];
            panel_by(ui, cx, g, t);
        });
        ui.separator();
    }
}

fn status_bar(ui: &mut Ui, cx: &mut Cx) {
    let ed = &mut *cx.ed;
    ui.horizontal(|ui| {
        ui.label(format!("{:.1}%", ed.view.s * 100.0));
        ui.separator();
        ui.label(format!("{} × {} px", ed.doc.w, ed.doc.h));
        ui.separator();
        ui.label(format!("Слой: {}", ed.doc.layer().name));
        ui.separator();
        let hint = match ed.tool {
            Tool::Brush => "Кисть: удержание — пипетка, задержка в конце штриха — быстрая фигура",
            Tool::Fill => "Заливка: тяните вбок, чтобы менять допуск",
            Tool::Select => "Выделение: Shift — добавить, Alt — вычесть",
            Tool::Transform => "Тяните маркеры; Shift — пропорционально; Enter — применить, Esc — отмена",
            Tool::Gradient => "Протяните от начала к концу (Shift — шаг 45°)",
            Tool::Shapes => "Протяните (Shift — пропорционально, Alt — от центра)",
            Tool::Clone => "Alt+щелчок — источник",
            _ => "",
        };
        ui.label(egui::RichText::new(hint).weak());
        if let Some(h) = ed.hover {
            let (x, y) = ed.view.to_doc(h);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.label(format!("X: {:.0}  Y: {:.0}", x, y)); });
        }
    });
}

/* ---------- раскладки ---------- */

impl App {
    fn compact(&self, ctx: &egui::Context) -> bool {
        match self.sh.prefs.ui_mode.as_str() {
            "desk" => false,
            "touch" => true,
            _ => ctx.screen_rect().width() < 900.0,
        }
    }

    pub fn editor_ui(&mut self, ctx: &egui::Context) {
        let compact = self.compact(ctx);
        let stage = if self.sh.prefs.light { Color32::from_gray(185) } else { Color32::from_gray(38) };
        let mut acts: Vec<&'static str> = vec![];
        let App { ed, sh, ui: st, tex, .. } = self;
        let Some(ed) = ed.as_mut() else { return };
        let hidden = ed.ui_hidden;
        {
            let mut cx = Cx { ed, sh, st, tex, acts: &mut acts };
            if !hidden {
                if !compact {
                    egui::TopBottomPanel::top("menubar").show(ctx, |ui| menu_bar(ui, &mut cx));
                    egui::TopBottomPanel::top("options").exact_height(34.0).show(ctx, |ui| { ui.add_space(3.0); egui::ScrollArea::horizontal().show(ui, |ui| options_bar(ui, &mut cx)); });
                    egui::TopBottomPanel::bottom("status").exact_height(22.0).show(ctx, |ui| status_bar(ui, &mut cx));
                    egui::SidePanel::left("tools").exact_width(52.0).resizable(false).show(ctx, |ui| { ui.add_space(4.0); tools_column(ui, &mut cx, 34.0); });
                    egui::SidePanel::right("dock").default_width(310.0).min_width(240.0).max_width(520.0).show(ctx, |ui| dock(ui, &mut cx));
                } else {
                    egui::TopBottomPanel::top("ctop").show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            let s = Vec2::new(38.0, 34.0);
                            if icon_btn(ui, "gallery", false, "Галерея", s).clicked() { cx.acts.push("gallery"); }
                            if icon_btn(ui, "menu", cx.st.win.menu, "Меню", s).clicked() { cx.st.win.menu = !cx.st.win.menu; }
                            if icon_btn(ui, "undo", false, "Отменить (2 пальца)", s).clicked() { cx.ed.undo(); }
                            if icon_btn(ui, "redo", false, "Повторить (3 пальца)", s).clicked() { cx.ed.redo(); }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let fg = cx.sh.colors.fg;
                                let (r, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
                                ui.painter().circle_filled(r.center(), 13.0, Color32::from_rgb(fg[0], fg[1], fg[2]));
                                ui.painter().circle_stroke(r.center(), 14.0, EStroke::new(2.0, ui.visuals().text_color()));
                                if resp.clicked() { cx.st.win.color = !cx.st.win.color; }
                                if icon_btn(ui, "layers", cx.st.win.layers, "Слои", s).clicked() { cx.st.win.layers = !cx.st.win.layers; }
                                if icon_btn(ui, "settings", cx.st.win.brushset, "Настройки кисти", s).clicked() { cx.st.win.brushset = !cx.st.win.brushset; }
                                if icon_btn(ui, "brush", cx.st.win.brushes, "Кисти", s).clicked() { cx.st.win.brushes = !cx.st.win.brushes; }
                            });
                        });
                    });
                    let needs_opts = !matches!(cx.ed.tool, Tool::Brush | Tool::Eraser | Tool::Smudge | Tool::Picker | Tool::Hand | Tool::Zoom | Tool::Text) || (cx.ed.tool == Tool::Eraser && cx.ed.doc.layer().is_vector());
                    if needs_opts {
                        egui::TopBottomPanel::bottom("copts").show(ctx, |ui| { egui::ScrollArea::horizontal().show(ui, |ui| options_bar(ui, &mut cx)); });
                    }
                    egui::SidePanel::left("ctools").exact_width(60.0).resizable(false).show(ctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| tools_column(ui, &mut cx, 38.0));
                    });
                    egui::SidePanel::right("csliders").exact_width(48.0).resizable(false).show(ctx, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(20.0);
                            let h = (ui.available_height() / 2.0 - 40.0).clamp(80.0, 220.0);
                            ui.spacing_mut().slider_width = h;
                            let ts = cx.ed.tool_state_mut(cx.sh);
                            ui.label(egui::RichText::new("Размер").size(10.0));
                            ui.add(egui::Slider::new(&mut ts.size, 0.0..=1.0).vertical().show_value(false));
                            ui.add_space(14.0);
                            ui.label(egui::RichText::new("Непр.").size(10.0));
                            ui.add(egui::Slider::new(&mut ts.opacity, 0.01..=1.0).vertical().show_value(false));
                        });
                    });
                    let ctx2 = ctx.clone();
                    let wins: [(&str, usize, usize); 5] = [("Цвет", 0, 0), ("Кисти", 1, 0), ("Настройки кисти", 1, 1), ("Слои", 2, 0), ("История", 2, 1)];
                    for (title, g, t) in wins {
                        let mut open = match (g, t) { (0, _) => cx.st.win.color, (1, 0) => cx.st.win.brushes, (1, _) => cx.st.win.brushset, (2, 0) => cx.st.win.layers, _ => cx.st.win.history };
                        if !open { continue; }
                        egui::Window::new(title).open(&mut open).default_width(320.0).max_height(ctx2.screen_rect().height() * 0.7).vscroll(true).show(&ctx2, |ui| panel_by(ui, &mut cx, g, t));
                        match (g, t) { (0, _) => cx.st.win.color = open, (1, 0) => cx.st.win.brushes = open, (1, _) => cx.st.win.brushset = open, (2, 0) => cx.st.win.layers = open, _ => cx.st.win.history = open }
                    }
                    if cx.st.win.menu {
                        let mut open = true;
                        egui::Window::new("Меню").open(&mut open).default_width(300.0).vscroll(true).show(&ctx2, |ui| {
                            egui::CollapsingHeader::new("Файл").show(ui, |ui| {
                                for (k, n) in [("new", "Новый холст"), ("open", "Открыть / импорт"), ("save", "Сохранить"), ("save_as", "Сохранить как…"), ("quick_png", "Быстрый экспорт PNG"), ("export_png", "Экспорт PNG"), ("export_jpeg", "Экспорт JPEG"), ("export_psd", "Экспорт PSD"), ("export_mazok", "Экспорт .mazok"), ("export_gif", "Экспорт GIF"), ("pick_export_dir", "Папка для сохранения…"), ("import_brushes", "Импорт кистей (.abr, .sut)"), ("import_palette", "Импорт палитры"), ("insert_image", "Вставить изображение…"), ("gallery", "В галерею")] {
                                    if ui.button(n).clicked() { cx.acts.push(k); }
                                }
                            });
                            egui::CollapsingHeader::new("Редактирование").show(ui, |ui| edit_menu(ui, &mut cx));
                            egui::CollapsingHeader::new("Изображение").show(ui, |ui| image_menu(ui, &mut cx));
                            egui::CollapsingHeader::new("Слои").show(ui, |ui| layer_menu(ui, &mut cx));
                            egui::CollapsingHeader::new("Выделение").show(ui, |ui| select_menu(ui, &mut cx));
                            egui::CollapsingHeader::new("Фильтр").show(ui, |ui| { for f in FILTERS { if ui.button(f.name).clicked() { cx.ed.filter_open(f.id); } } });
                            egui::CollapsingHeader::new("Просмотр").show(ui, |ui| view_menu(ui, &mut cx));
                            if ui.button("История").clicked() { cx.st.win.history = true; }
                            if ui.button("Настройки").clicked() { cx.st.settings = true; }
                            if ui.button("Справка").clicked() { cx.st.help = true; }
                        });
                        cx.st.win.menu = open;
                    }
                }
            }
            egui::CentralPanel::default().frame(egui::Frame::default().fill(stage)).show(ctx, |ui| cx.ed.canvas_ui(ui, cx.sh, stage));
            if let Some(editor::Request::Text { spec, layer }) = cx.ed.request.take() {
                cx.st.text = Some(TextDlg { spec, layer, font: 0 });
            }
        }
        for a in acts { self.dispatch(a); }
    }

    pub fn dispatch(&mut self, a: &str) {
        if let Some(k) = a.strip_prefix("export_") { self.export(k); } else { self.act(a); }
    }

    /* ---------- галерея ---------- */

    pub fn gallery_ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("ghead").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading("Мазок");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(egui::RichText::new("+ Холст").color(Color32::WHITE)).fill(ACCENT)).clicked() { self.ui.new_canvas = Some(NewCanvas::default()); }
                    if ui.button("Импорт").on_hover_text("PNG, JPEG, PSD или .mazok — или перетащите файлы в окно").clicked() { self.import_files(); }
                    if ui.button("Вставить").on_hover_text("Новый холст из картинки в буфере обмена (Ctrl+V)").clicked() { self.clipboard_paste(None); }
                    if ui.button("Настройки").clicked() { self.ui.settings = true; }
                });
            });
            ui.add_space(4.0);
        });
        let mut open = None;
        let mut del = None;
        let mut dup = None;
        let mut exp = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.items.is_empty() {
                ui.vertical_centered(|ui| { ui.add_space(80.0); ui.heading("Здесь будут ваши рисунки"); ui.label("Нажмите «+ Холст», чтобы начать, вставьте картинку (Ctrl+V) или перетащите файлы в окно."); });
                return;
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(16.0);
                    let items = self.items.clone();
                    for it in items {
                        let key = format!("g:{}", it.id);
                        let path = self.docs_dir.join(format!("{}.png", it.id));
                        let t = texture(ctx, &mut self.tex, key, it.modified as u64, || std::fs::read(&path).ok().and_then(|b| crate::formats::decode_image(&b).ok()).unwrap_or_else(|| Img::filled(8, 8, [200, 200, 200, 255])));
                        ui.vertical(|ui| {
                            ui.set_width(190.0);
                            ui.spacing_mut().item_spacing = Vec2::new(4.0, 3.0);
                            let sz = t.size_vec2();
                            let k = (190.0 / sz.x).min(142.0 / sz.y);
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(190.0, 142.0), Sense::click());
                            ui.painter().rect_filled(r, 8.0, ui.visuals().faint_bg_color);
                            let ir = Rect::from_center_size(r.center(), sz * k);
                            ui.painter().image(t.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                            if resp.hovered() { ui.painter().rect_stroke(r, 8.0, EStroke::new(2.0, ACCENT), egui::StrokeKind::Inside); }
                            if resp.clicked() { open = Some(it.id.clone()); }
                            resp.context_menu(|ui| {
                                if ui.button("Открыть").clicked() { open = Some(it.id.clone()); ui.close_menu(); }
                                if ui.button("Переименовать").clicked() { self.ui.rename = Some((it.id.clone(), it.name.clone())); ui.close_menu(); }
                                if ui.button("Дублировать").clicked() { dup = Some(it.id.clone()); ui.close_menu(); }
                                if ui.button("Экспорт .mazok").clicked() { exp = Some(it.id.clone()); ui.close_menu(); }
                                if ui.button("Удалить").clicked() { del = Some(it.id.clone()); ui.close_menu(); }
                            });
                            ui.label(egui::RichText::new(&it.name).strong());
                            ui.label(egui::RichText::new(format!("{}×{} · правый щелчок — меню", it.w, it.h)).weak().size(11.0));
                        });
                    }
                });
            });
        });
        if let Some(id) = open { self.open_doc(&id); }
        if let Some(id) = del { self.ui.confirm_delete = Some(id); }
        if let Some(id) = dup { self.duplicate_doc(&id); }
        if let Some(id) = exp { self.export_doc_from_gallery(&id); }
    }

    /* ---------- диалоги ---------- */

    pub fn dialogs_ui(&mut self, ctx: &egui::Context) {
        let win = |t: &str| egui::Window::new(t.to_string()).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);
        if let Some(nc) = &mut self.ui.new_canvas {
            let mut create = false;
            let mut close = false;
            win("Новый холст").show(ctx, |ui| {
                ui.horizontal(|ui| { ui.label("Название"); ui.text_edit_singleline(&mut nc.name); });
                let presets: [(&str, usize, usize); 11] = [("Квадрат", 2048, 2048), ("A4, 300 dpi", 2480, 3508), ("A5, 300 dpi", 1748, 2480), ("Full HD", 1920, 1080), ("4K", 3840, 2160), ("Сторис 9:16", 1080, 1920), ("Пост 4:5", 1080, 1350), ("Экран", 2560, 1600), ("Пиксель-арт", 128, 128), ("Иконка", 512, 512), ("Скетч", 1500, 1500)];
                egui::Grid::new("presets").num_columns(3).spacing([6.0, 6.0]).show(ui, |ui| {
                    for (i, (n, w, h)) in presets.iter().enumerate() {
                        if ui.selectable_label(nc.w == *w && nc.h == *h, format!("{n}\n{w} × {h}")).clicked() { nc.w = *w; nc.h = *h; }
                        if i % 3 == 2 { ui.end_row(); }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Ширина"); ui.add(egui::DragValue::new(&mut nc.w).range(16..=8192).suffix(" px"));
                    ui.label("Высота"); ui.add(egui::DragValue::new(&mut nc.h).range(16..=8192).suffix(" px"));
                });
                ui.horizontal(|ui| { ui.label("Фон"); ui.color_edit_button_srgb(&mut nc.bg); ui.checkbox(&mut nc.transparent, "Прозрачный"); });
                let budget: f64 = if cfg!(target_os = "android") { 700e6 } else { 2.5e9 };
                ui.label(egui::RichText::new(format!("До {} слоёв на этом устройстве", ((budget / (nc.w * nc.h * 4) as f64) as usize).saturating_sub(4).clamp(3, 200))).weak());
                ui.horizontal(|ui| {
                    if ui.button("Отмена").clicked() { close = true; }
                    if ui.add(egui::Button::new(egui::RichText::new("Создать").color(Color32::WHITE)).fill(ACCENT)).clicked() { create = true; }
                });
            });
            if create {
                let nc = self.ui.new_canvas.take().unwrap();
                if nc.w * nc.h > 50_000_000 { self.toast("Слишком большой холст"); } else { self.create_doc(nc.name.trim(), nc.w, nc.h, nc.bg, !nc.transparent); }
            } else if close { self.ui.new_canvas = None; }
        }
        if let Some((id, name)) = &mut self.ui.rename {
            let (mut ok, mut close) = (false, false);
            win("Переименовать").show(ctx, |ui| {
                let r = ui.text_edit_singleline(name);
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { ok = true; }
                ui.horizontal(|ui| { if ui.button("Отмена").clicked() { close = true; } if ui.button("Готово").clicked() { ok = true; } });
            });
            if ok { let (id, name) = (id.clone(), name.clone()); self.ui.rename = None; if !name.trim().is_empty() { self.rename_doc(&id, name.trim()); } } else if close { self.ui.rename = None; }
        }
        if let Some((li, name)) = &mut self.ui.layer_rename {
            let (mut ok, mut close) = (false, false);
            win("Имя слоя").show(ctx, |ui| {
                ui.text_edit_singleline(name);
                ui.horizontal(|ui| { if ui.button("Отмена").clicked() { close = true; } if ui.button("Готово").clicked() { ok = true; } });
            });
            if ok { let (li, n) = (*li, name.clone()); self.ui.layer_rename = None; if let Some(ed) = &mut self.ed { ed.layer_props(li, |l| l.name = n, "Переименовать слой"); } } else if close { self.ui.layer_rename = None; }
        }
        if let Some(id) = self.ui.confirm_delete.clone() {
            let (mut ok, mut close) = (false, false);
            win("Удалить рисунок?").show(ctx, |ui| {
                ui.label("Рисунок будет удалён безвозвратно.");
                ui.horizontal(|ui| { if ui.button("Отмена").clicked() { close = true; } if ui.add(egui::Button::new(egui::RichText::new("Удалить").color(Color32::WHITE)).fill(Color32::from_rgb(200, 60, 60))).clicked() { ok = true; } });
            });
            if ok { self.ui.confirm_delete = None; self.delete_doc(&id); } else if close { self.ui.confirm_delete = None; }
        }
        if let Some(px) = &mut self.ui.feather {
            let (mut ok, mut close) = (false, false);
            win("Растушёвка").show(ctx, |ui| {
                ui.add(egui::Slider::new(px, 1.0..=200.0).suffix(" px"));
                ui.horizontal(|ui| { if ui.button("Отмена").clicked() { close = true; } if ui.button("Применить").clicked() { ok = true; } });
            });
            if ok { let v = *px; self.ui.feather = None; if let Some(ed) = &mut self.ed { ed.select_feather(v); } } else if close { self.ui.feather = None; }
        }
        if let Some(rd) = &mut self.ui.resize {
            let (mut ok, mut close) = (false, false);
            let (ow, oh) = self.ed.as_ref().map_or((1, 1), |e| (e.doc.w, e.doc.h));
            win("Размер изображения и холста").show(ctx, |ui| {
                ui.horizontal(|ui| { ui.selectable_value(&mut rd.scale, false, "Размер холста"); ui.selectable_value(&mut rd.scale, true, "Масштабировать изображение"); });
                ui.horizontal(|ui| {
                    ui.label("Ширина");
                    if ui.add(egui::DragValue::new(&mut rd.w).range(1..=8192)).changed() && rd.keep { rd.h = ((rd.w as f32 * oh as f32 / ow as f32).round() as usize).max(1); }
                    ui.label("Высота");
                    if ui.add(egui::DragValue::new(&mut rd.h).range(1..=8192)).changed() && rd.keep { rd.w = ((rd.h as f32 * ow as f32 / oh as f32).round() as usize).max(1); }
                });
                ui.checkbox(&mut rd.keep, "Сохранять пропорции");
                if !rd.scale {
                    ui.label("Привязка");
                    egui::Grid::new("anchor").show(ui, |ui| {
                        for y in 0..3 { for x in 0..3 { if ui.selectable_label(rd.anchor == (x, y), " o ").clicked() { rd.anchor = (x, y); } } ui.end_row(); }
                    });
                }
                ui.horizontal(|ui| { if ui.button("Отмена").clicked() { close = true; } if ui.button("Применить").clicked() { ok = true; } });
            });
            if ok {
                let rd = self.ui.resize.take().unwrap();
                if let Some(ed) = &mut self.ed {
                    if (rd.w, rd.h) != (ow, oh) && rd.w * rd.h <= 50_000_000 {
                        let ox = (rd.w as i32 - ow as i32) * rd.anchor.0 / 2;
                        let oy = (rd.h as i32 - oh as i32) * rd.anchor.1 / 2;
                        ed.resize_canvas(rd.w, rd.h, ox, oy, rd.scale);
                    }
                }
            } else if close { self.ui.resize = None; }
        }
        if let Some(td) = &mut self.ui.text {
            let (mut ok, mut close) = (false, false);
            let fonts = self.fonts.clone();
            win(if td.layer.is_some() { "Изменить текст" } else { "Новый текст" }).show(ctx, |ui| {
                ui.add(egui::TextEdit::multiline(&mut td.spec.str).desired_rows(4).desired_width(360.0).hint_text("Введите текст…"));
                egui::ComboBox::from_label("Шрифт").selected_text(fonts.get(td.font).map_or("Встроенный".to_string(), |f| f.0.clone())).show_ui(ui, |ui| {
                    for (i, (n, _)) in fonts.iter().enumerate() { ui.selectable_value(&mut td.font, i, n); }
                });
                ui.add(egui::Slider::new(&mut td.spec.size, 4.0..=600.0).text("Размер, px"));
                ui.add(egui::Slider::new(&mut td.spec.lh, 0.6..=3.0).text("Межстрочный"));
                ui.add(egui::Slider::new(&mut td.spec.spacing, -20.0..=100.0).text("Трекинг"));
                ui.horizontal(|ui| {
                    let mut c = util::parse_hex(&td.spec.color).unwrap_or([0, 0, 0]);
                    if ui.color_edit_button_srgb(&mut c).changed() { td.spec.color = util::hex(c); }
                    ui.label("Цвет");
                    ui.checkbox(&mut td.spec.bold, "Жирный");
                    for (a, n) in [("left", "Влево"), ("center", "Центр"), ("right", "Вправо")] { ui.selectable_value(&mut td.spec.align, a.to_string(), n); }
                });
                ui.horizontal(|ui| { if ui.button("Отмена").clicked() { close = true; } if ui.button("Готово").clicked() { ok = true; } });
            });
            if ok {
                let td = self.ui.text.take().unwrap();
                let bytes = self.fonts.get(td.font).and_then(|f| f.1.clone()).and_then(|p: PathBuf| {
                    let key = p.to_string_lossy().to_string();
                    if let Some(b) = self.ui.font_cache.get(&key) { return Some(b.clone()); }
                    let b = std::fs::read(&p).ok()?;
                    self.ui.font_cache.insert(key, b.clone());
                    Some(b)
                });
                let mut spec = td.spec;
                spec.font = self.fonts.get(td.font).map_or("Встроенный".into(), |f| f.0.clone());
                if !spec.str.trim().is_empty() { if let Some(ed) = &mut self.ed { ed.apply_text(spec, td.layer, bytes.as_deref()); } }
            } else if close { self.ui.text = None; }
        }
        if let Some(ed) = &mut self.ed {
            if let Some(f) = &mut ed.filter {
                let def = filters::find(f.id);
                let (mut apply, mut cancel, mut reset) = (false, false, false);
                egui::Window::new(def.name).collapsible(false).resizable(false).anchor(egui::Align2::RIGHT_TOP, [-330.0, 80.0]).show(ctx, |ui| {
                    for (i, p) in def.params.iter().enumerate() {
                        if !p.choices.is_empty() {
                            let mut v = f.params[i].round() as usize;
                            egui::ComboBox::from_label(p.name).selected_text(p.choices[v.min(p.choices.len() - 1)]).show_ui(ui, |ui| { for (k, n) in p.choices.iter().enumerate() { ui.selectable_value(&mut v, k, *n); } });
                            if v as f32 != f.params[i] { f.params[i] = v as f32; f.dirty = true; }
                        } else if ui.add(egui::Slider::new(&mut f.params[i], p.min..=p.max).text(p.name)).changed() { f.dirty = true; }
                    }
                    if def.params.is_empty() { ui.label("Без параметров — нажмите «Применить»."); }
                    ui.horizontal(|ui| { if ui.button("Сброс").clicked() { reset = true; } if ui.button("Отмена").clicked() { cancel = true; } if ui.add(egui::Button::new(egui::RichText::new("Применить").color(Color32::WHITE)).fill(ACCENT)).clicked() { apply = true; } });
                });
                if reset { f.params = filters::defaults(def); f.dirty = true; }
                if apply { ed.filter_apply(); } else if cancel { ed.filter_cancel(); }
            }
        }
        if let Some((i, k)) = self.ui.palette_export.take() { self.export_palette(i, k); }
        if self.drop_hover {
            egui::Area::new(egui::Id::new("drop")).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).order(egui::Order::Foreground).show(ctx, |ui| {
                egui::Frame::popup(ui.style()).inner_margin(24.0).show(ui, |ui| {
                    ui.heading("Отпустите файлы");
                    ui.label(if self.ed.is_some() { "Картинки — новыми слоями, кисти .abr/.sut — в набор «Импорт», палитры — в образцы, проекты — откроются" } else { "Картинки и проекты станут холстами, кисти и палитры добавятся в библиотеку" });
                });
            });
        }
        let mut pick_dir = false;
        if self.ui.settings {
            let mut open = true;
            let p = &mut self.sh.prefs;
            egui::Window::new("Настройки").open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Интерфейс");
                    for (k, n) in [("auto", "Автоматически"), ("desk", "Настольный"), ("touch", "Сенсорный")] { ui.selectable_value(&mut p.ui_mode, k.to_string(), n); }
                });
                ui.checkbox(&mut p.light, "Светлая тема");
                ui.add(egui::Slider::new(&mut p.ui_scale, 0.7..=1.8).text("Масштаб интерфейса"));
                ui.separator();
                ui.add(egui::Slider::new(&mut p.pressure_gamma, 0.4..=2.5).text("Кривая нажима (меньше — мягче)"));
                ui.add(egui::Slider::new(&mut p.streamline_mul, 0.0..=2.0).text("Сглаживание штриха (общее)"));
                ui.add(egui::Slider::new(&mut p.stabilizer, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)).text("Стабилизатор «ленивая кисть»")).on_hover_text("Кисть тянется за пером на нити: ровные линии без дрожания руки");
                ui.add(egui::Slider::new(&mut p.post_smooth, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)).text("Коррекция линии после штриха"));
                ui.horizontal(|ui| { ui.label("Векторный ластик"); vector_erase_modes(ui, &mut p.vector_erase); });
                ui.checkbox(&mut p.touch_draw, "Рисование пальцем (иначе палец двигает холст)");
                ui.checkbox(&mut p.hold_pick, "Пипетка при удержании");
                ui.add(egui::Slider::new(&mut p.hold_delay, 0.2..=1.2).text("Задержка пипетки, с"));
                ui.checkbox(&mut p.quick_shape, "Быстрая фигура (задержка в конце штриха)");
                ui.add(egui::Slider::new(&mut p.quick_shape_delay, 0.3..=1.5).text("Задержка фигуры, с"));
                ui.checkbox(&mut p.cursor, "Контур кисти под курсором");
                ui.checkbox(&mut p.pick_layer, "Пипетка берёт цвет только с текущего слоя");
                ui.separator();
                ui.label(egui::RichText::new("Сохранение и экспорт").strong());
                ui.horizontal(|ui| {
                    ui.label("Папка:");
                    ui.label(egui::RichText::new(if p.export_dir.is_empty() { "не выбрана — каждый раз спрашивать".to_string() } else { crate::platform::folder_label(&p.export_dir) }).weak());
                });
                ui.horizontal(|ui| {
                    if ui.button("Выбрать папку…").clicked() { pick_dir = true; }
                    #[cfg(not(target_os = "android"))]
                    if !p.export_dir.is_empty() && ui.button("Открыть в проводнике").clicked() { crate::platform::reveal(&p.export_dir); }
                    if !p.export_dir.is_empty() && ui.button("Сбросить").clicked() { p.export_dir.clear(); p.quick_export = false; }
                });
                ui.add_enabled(!p.export_dir.is_empty(), egui::Checkbox::new(&mut p.quick_export, "Экспорт сразу в эту папку, без диалога"));
                ui.label(egui::RichText::new("«Сохранить как…» всегда спрашивает место и имя файла").weak());
            });
            self.ui.settings = open;
        }
        if pick_dir { self.pick_export_dir(); }
        if self.ui.help {
            let mut open = true;
            egui::Window::new("Справка").open(&mut open).collapsible(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).vscroll(true).show(ctx, |ui| {
                egui::Grid::new("help").striped(true).show(ui, |ui| {
                    for (a, b) in HELP { ui.label(egui::RichText::new(*a).weak()); ui.label(*b); ui.end_row(); }
                });
            });
            self.ui.help = open;
        }
    }

    pub fn toast_ui(&mut self, ctx: &egui::Context) {
        if let Some((m, t)) = &self.toast {
            if self.now - t > 2.6 || m.is_empty() { self.toast = None; return; }
            let m = m.clone();
            egui::Area::new(egui::Id::new("toast")).anchor(egui::Align2::CENTER_BOTTOM, [0.0, -48.0]).order(egui::Order::Tooltip).show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| { ui.label(m); });
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
    }
}

const HELP: &[(&str, &str)] = &[
    ("B / E / R / S", "Кисть / Ластик / Палец / Штамп"), ("G / Shift+G", "Заливка / Градиент"), ("I / M / L / W", "Пипетка / Выделение / Лассо / Волшебная палочка"),
    ("V / T / U / H / Z", "Трансформация / Текст / Фигуры / Рука / Масштаб"), ("X / D", "Поменять цвета / Чёрный и белый"), ("[ и ]", "Размер кисти"),
    ("Ctrl+Z / Ctrl+Shift+Z", "Отменить / Повторить"), ("Ctrl+N / Ctrl+O / Ctrl+S", "Создать / Открыть / Сохранить"), ("Ctrl+T", "Свободное трансформирование"),
    ("Ctrl+J / Ctrl+E", "Дублировать слой / Объединить с нижним"), ("Ctrl+A / Ctrl+D / Ctrl+Shift+I", "Выделить всё / Снять / Инвертировать"),
    ("Пробел + перетаскивание", "Сдвиг холста"), ("Колесо мыши", "Масштаб"), ("Alt или правая кнопка", "Пипетка"), ("Ctrl+0 / Ctrl+1 / F", "Вписать / 100% / Зеркальный вид"),
    ("Ctrl+C / Ctrl+X / Ctrl+V", "Копировать / вырезать / вставить (в том числе картинки из других программ)"), ("Ctrl+Shift+S", "Сохранить как… (выбор папки)"),
    ("Перетаскивание файлов в окно", "Картинки, проекты, кисти .abr/.sut, палитры"), ("Alt + колесо мыши", "Размер кисти"), ("1 … 9, 0", "Непрозрачность 10…100 %"),
    ("Tab", "Скрыть панели"), ("Удержание пера", "Пипетка"), ("Задержка в конце штриха", "Быстрая фигура: линия, круг, многоугольник"),
    ("2 пальца — касание", "Отменить"), ("3 пальца — касание", "Повторить"), ("2 пальца — движение", "Сдвиг, масштаб, поворот"), ("4 пальца — касание", "Скрыть панели"),
];

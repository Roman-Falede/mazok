//! Приложение: галерея, хранение рисунков, автосохранение, экспорт и команды.

use crate::doc::{self, Document};
use crate::editor::{Editor, SelShape, Tool};
use crate::formats;
use crate::platform;
use crate::raster::Img;
use crate::state::Shared;
use crate::ui::UiState;
use crate::util;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GalleryItem {
    pub id: String,
    pub name: String,
    pub w: usize,
    pub h: usize,
    pub modified: i64,
}

pub struct App {
    pub sh: Shared,
    pub ed: Option<Editor>,
    pub items: Vec<GalleryItem>,
    pub tex: HashMap<String, (u64, egui::TextureHandle)>,
    pub ui: UiState,
    saver: Option<std::thread::JoinHandle<()>>,
    pub fonts: Vec<(String, Option<PathBuf>)>,
    pub toast: Option<(String, f64)>,
    pub now: f64,
    pub docs_dir: PathBuf,
    visuals: Option<(bool, f32)>,
    last_settings_save: f64,
    pub test: Option<crate::autotest::AutoTest>,
    focused: bool,
    force_save: bool,
}

pub fn snapshot(d: &Document) -> Document {
    Document {
        id: d.id.clone(), name: d.name.clone(), w: d.w, h: d.h, bg: d.bg, bg_visible: d.bg_visible, layers: d.layers.clone(), active: d.active,
        layer_seq: d.layer_seq, created: d.created, modified: d.modified, strokes: d.strokes, time: d.time, guide: d.guide.clone(),
    }
}

pub fn write_doc(dir: &Path, d: &Document) -> Result<(), String> {
    let _ = std::fs::create_dir_all(dir);
    let bytes = formats::save_mazok(d);
    let tmp = dir.join(format!("{}.tmp", d.id));
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(format!("{}.mazok", d.id))).map_err(|e| e.to_string())?;
    let item = GalleryItem { id: d.id.clone(), name: d.name.clone(), w: d.w, h: d.h, modified: d.modified };
    let _ = std::fs::write(dir.join(format!("{}.json", d.id)), serde_json::to_vec(&item).unwrap_or_default());
    let thumb = doc::flatten(d, true);
    let mut t = thumb.thumbnail(360);
    if !d.bg_visible {
        let mut bg = Img::filled(t.w, t.h, [255, 255, 255, 255]);
        bg.draw_over(0, 0, &t, 1.0);
        t = bg;
    }
    let _ = std::fs::write(dir.join(format!("{}.png", d.id)), formats::png(&t));
    Ok(())
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, data_dir: PathBuf) -> App {
        let sh = Shared::load(data_dir.clone());
        let docs_dir = data_dir.join("docs");
        let _ = std::fs::create_dir_all(&docs_dir);
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let mut app = App {
            sh, ed: None, items: vec![], tex: HashMap::new(), ui: UiState::default(), saver: None, fonts: platform::fonts(), toast: None,
            now: 0.0, docs_dir, visuals: None, last_settings_save: 0.0, test: crate::autotest::AutoTest::from_env(),
            focused: true, force_save: false,
        };
        app.refresh_gallery();
        app
    }

    pub fn toast(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), self.now));
    }

    /* ---------- галерея и хранение ---------- */

    pub fn refresh_gallery(&mut self) {
        let mut items = vec![];
        if let Ok(rd) = std::fs::read_dir(&self.docs_dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map_or(false, |x| x == "json") {
                    if let Some(it) = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<GalleryItem>(&b).ok()) {
                        if self.docs_dir.join(format!("{}.mazok", it.id)).exists() {
                            items.push(it);
                        }
                    }
                }
            }
        }
        items.sort_by(|a, b| b.modified.cmp(&a.modified));
        self.items = items;
    }

    fn wait_saver(&mut self) {
        if let Some(h) = self.saver.take() {
            let _ = h.join();
        }
    }

    pub fn save_sync(&mut self) {
        self.wait_saver();
        if let Some(ed) = &mut self.ed {
            ed.commit_pending();
            ed.cancel();
            if let Err(e) = write_doc(&self.docs_dir, &ed.doc) {
                self.toast = Some((format!("Ошибка сохранения: {e}"), self.now));
            }
            ed.changed = false;
        }
        self.sh.save();
    }

    fn autosave(&mut self) {
        let Some(ed) = &mut self.ed else { return };
        // При сворачивании окна (на Android — приложения) сохраняем сразу, не дожидаясь паузы.
        let urgent = self.force_save;
        if !ed.changed || ed.busy() || ed.xform.is_some() || ed.filter.is_some() || (!urgent && self.now - ed.change_time < 2.0) {
            return;
        }
        if self.saver.as_ref().map_or(false, |h| !h.is_finished()) {
            return;
        }
        self.force_save = false;
        self.saver = None;
        ed.changed = false;
        let snap = snapshot(&ed.doc);
        let dir = self.docs_dir.clone();
        self.saver = Some(std::thread::spawn(move || { let _ = write_doc(&dir, &snap); }));
        if self.now - self.last_settings_save > 10.0 {
            self.last_settings_save = self.now;
            self.sh.save();
        }
    }

    pub fn open_doc(&mut self, id: &str) {
        self.wait_saver();
        let path = self.docs_dir.join(format!("{id}.mazok"));
        match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| formats::load_mazok(&b)) {
            Ok(d) => self.open_document(d),
            Err(e) => self.toast(format!("Не удалось открыть: {e}")),
        }
    }

    pub fn open_document(&mut self, d: Document) {
        if self.ed.is_some() {
            self.save_sync();
        }
        let mut ed = Editor::new(d);
        ed.tool = Tool::Brush;
        self.ed = Some(ed);
        self.ui.reset_for_doc();
    }

    pub fn create_doc(&mut self, name: &str, w: usize, h: usize, bg: [u8; 3], bg_visible: bool) {
        let d = Document::new(name, w, h, bg, bg_visible);
        if let Err(e) = write_doc(&self.docs_dir, &d) {
            self.toast(format!("Ошибка: {e}"));
        }
        self.open_document(d);
    }

    pub fn to_gallery(&mut self) {
        self.save_sync();
        self.ed = None;
        self.tex.retain(|k, _| k.starts_with("b:"));
        self.refresh_gallery();
    }

    pub fn delete_doc(&mut self, id: &str) {
        for ext in ["mazok", "json", "png"] {
            let _ = std::fs::remove_file(self.docs_dir.join(format!("{id}.{ext}")));
        }
        self.tex.remove(&format!("g:{id}"));
        self.refresh_gallery();
    }

    pub fn duplicate_doc(&mut self, id: &str) {
        let path = self.docs_dir.join(format!("{id}.mazok"));
        if let Ok(mut d) = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| formats::load_mazok(&b)) {
            d.id = util::uid();
            d.name = format!("{} (копия)", d.name);
            d.modified = util::now_ms();
            let _ = write_doc(&self.docs_dir, &d);
            self.refresh_gallery();
        }
    }

    pub fn rename_doc(&mut self, id: &str, name: &str) {
        if let Some(ed) = &mut self.ed {
            if ed.doc.id == id {
                ed.doc.name = name.to_string();
                ed.mark_changed();
                return;
            }
        }
        let path = self.docs_dir.join(format!("{id}.mazok"));
        if let Ok(mut d) = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| formats::load_mazok(&b)) {
            d.name = name.to_string();
            let _ = write_doc(&self.docs_dir, &d);
            self.refresh_gallery();
        }
    }

    pub fn import_files(&mut self) {
        if !platform::can_pick_files() {
            self.toast("Открытие файлов на Android пока недоступно");
            return;
        }
        let files = platform::pick_files("Открыть", "Изображения и проекты", &["png", "jpg", "jpeg", "webp", "psd", "mazok", "zip"], true);
        let mut last = None;
        for (name, bytes) in files {
            match formats::import(&bytes, &name) {
                Ok(d) => {
                    let _ = write_doc(&self.docs_dir, &d);
                    last = Some(d);
                }
                Err(e) => self.toast(format!("{name}: {e}")),
            }
        }
        self.refresh_gallery();
        if let Some(d) = last {
            self.open_document(d);
        }
    }

    pub fn export_doc_from_gallery(&mut self, id: &str) {
        if let Ok(b) = std::fs::read(self.docs_dir.join(format!("{id}.mazok"))) {
            let name = self.items.iter().find(|i| i.id == id).map(|i| i.name.clone()).unwrap_or_else(|| "Рисунок".into());
            self.save_result(platform::save_bytes(&format!("{}.mazok", safe_name(&name)), "application/zip", &b));
        }
    }

    pub fn save_result(&mut self, r: Result<String, String>) {
        match r {
            Ok(m) => self.toast(m),
            Err(e) if e.is_empty() => {}
            Err(e) => self.toast(e),
        }
    }

    /* ---------- экспорт ---------- */

    pub fn export(&mut self, kind: &str) {
        let Some(ed) = &mut self.ed else { return };
        ed.commit_pending();
        let d = &ed.doc;
        let base = safe_name(&d.name);
        let res = match kind {
            "png" => platform::save_bytes(&format!("{base}.png"), "image/png", &formats::png(&doc::flatten(d, true))),
            "jpeg" => platform::save_bytes(&format!("{base}.jpg"), "image/jpeg", &formats::jpeg(&doc::flatten(d, true), [255, 255, 255])),
            "psd" => platform::save_bytes(&format!("{base}.psd"), "image/vnd.adobe.photoshop", &formats::psd_write(d)),
            "mazok" => platform::save_bytes(&format!("{base}.mazok"), "application/zip", &formats::save_mazok(d)),
            "layers" => {
                let files: Vec<(String, Vec<u8>)> = d.layers.iter().enumerate().map(|(i, l)| (format!("{:02} {}.png", i + 1, safe_name(&l.name)), formats::png(&l.img))).collect();
                platform::save_bytes(&format!("{base} — слои.zip"), "application/zip", &formats::zip_write(&files))
            }
            "gif" => {
                let k = (720.0 / d.w.max(d.h) as f32).min(1.0);
                let (w, h) = (((d.w as f32 * k) as usize).max(1), ((d.h as f32 * k) as usize).max(1));
                let frames: Vec<Img> = d.layers.iter().map(|l| {
                    let mut f = if d.bg_visible { Img::filled(d.w, d.h, [d.bg[0], d.bg[1], d.bg[2], 255]) } else { Img::new(d.w, d.h) };
                    if l.visible { f.draw_over(0, 0, &l.img, l.opacity); }
                    f.resize(w, h)
                }).collect();
                match formats::gif(&frames, 12) {
                    Ok(b) => platform::save_bytes(&format!("{base}.gif"), "image/gif", &b),
                    Err(e) => Err(e),
                }
            }
            _ => Err(String::new()),
        };
        self.save_result(res);
    }

    pub fn insert_image(&mut self) {
        let files = platform::pick_files("Вставить изображение", "Изображения", &["png", "jpg", "jpeg", "webp"], false);
        if !platform::can_pick_files() {
            self.toast("Открытие файлов на Android пока недоступно");
        }
        for (name, bytes) in files {
            match formats::decode_image(&bytes) {
                Ok(img) => {
                    if let Some(ed) = &mut self.ed {
                        let stem = Path::new(&name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        ed.layer_add_image(img, &stem);
                        ed.set_tool(Tool::Transform);
                    }
                }
                Err(e) => self.toast(e),
            }
        }
    }

    /* ---------- команды ---------- */

    pub fn act(&mut self, a: &str) {
        match a {
            "new" => self.ui.new_canvas = Some(crate::ui::NewCanvas::default()),
            "open" => self.import_files(),
            "save" => { self.save_sync(); self.toast("Сохранено"); }
            "gallery" => self.to_gallery(),
            "insert_image" => self.insert_image(),
            _ => {}
        }
        let sh = &mut self.sh;
        let Some(ed) = &mut self.ed else { return };
        match a {
            "undo" => ed.undo(),
            "redo" => ed.redo(),
            "layer_new" => ed.layer_add(),
            "sel_all" => ed.select_all(),
            "sel_none" => ed.select_none(),
            "sel_invert" => ed.select_invert(),
            "copy" => ed.copy(false),
            "copy_merged" => ed.copy(true),
            "cut" => ed.cut(),
            "paste" => ed.paste(),
            "dup_or_copy" => { if ed.sel.is_some() { ed.copy_to_layer(false) } else { ed.layer_duplicate() } }
            "cut_layer" => ed.copy_to_layer(true),
            "merge" => ed.merge_down(),
            "flatten" => ed.flatten_all(),
            "transform" => ed.set_tool(Tool::Transform),
            "fit" => ed.fit(),
            "actual" => { let c = ed.canvas_rect.center(); let k = 1.0 / ed.view.s; ed.view.zoom_at(c, k); }
            "zoom_in" => { let c = ed.canvas_rect.center(); ed.view.zoom_at(c, 1.25); }
            "zoom_out" => { let c = ed.canvas_rect.center(); ed.view.zoom_at(c, 0.8); }
            "flip_view" => { let c = ed.canvas_rect.center(); let (x, y) = ed.view.to_doc(c); ed.view.flip *= -1.0; let q = ed.view.to_screen(x, y); ed.view.ox += c.x - q.x; ed.view.oy += c.y - q.y; }
            "reset_rotation" => { let c = ed.canvas_rect.center(); let r = ed.view.r; ed.view.rotate_at(c, -r); }
            "t_brush" => ed.set_tool(Tool::Brush),
            "t_eraser" => ed.set_tool(Tool::Eraser),
            "t_smudge" => ed.set_tool(Tool::Smudge),
            "t_clone" => ed.set_tool(Tool::Clone),
            "t_picker" => ed.set_tool(Tool::Picker),
            "t_transform" => ed.set_tool(Tool::Transform),
            "t_text" => ed.set_tool(Tool::Text),
            "t_shapes" => ed.set_tool(Tool::Shapes),
            "t_hand" => ed.set_tool(Tool::Hand),
            "t_zoom" => ed.set_tool(Tool::Zoom),
            "t_fill" => ed.set_tool(Tool::Fill),
            "t_gradient" => ed.set_tool(Tool::Gradient),
            "t_select_rect" => { ed.sel_shape = if ed.tool == Tool::Select && ed.sel_shape == SelShape::Rect { SelShape::Ellipse } else { SelShape::Rect }; ed.set_tool(Tool::Select); }
            "t_select_lasso" => { ed.sel_shape = SelShape::Lasso; ed.set_tool(Tool::Select); }
            "t_select_wand" => { ed.sel_shape = SelShape::Wand; ed.set_tool(Tool::Select); }
            "swap" => sh.colors.swap(),
            "default_colors" => { sh.colors.fg = [0, 0, 0]; sh.colors.bg = [255, 255, 255]; }
            "size_down" | "size_up" => {
                let ts = ed.tool_state_mut(sh);
                ts.size = (ts.size + if a == "size_up" { 0.03 } else { -0.03 }).clamp(0.0, 1.0);
            }
            "clear" => { if ed.xform.is_none() { ed.layer_clear(); } }
            "enter" => { if ed.xform.is_some() { ed.xform_commit(); let p = ed.prev_tool; ed.set_tool(if p == Tool::Transform { Tool::Brush } else { p }); } else if ed.filter.is_some() { ed.filter_apply(); } }
            "escape" => {
                if ed.xform.is_some() { ed.xform_cancel(); let p = ed.prev_tool; ed.tool = if p == Tool::Transform { Tool::Brush } else { p }; }
                else if ed.filter.is_some() { ed.filter_cancel(); }
                else if ed.ui_hidden { ed.ui_hidden = false; }
            }
            "toggle_ui" => ed.ui_hidden = !ed.ui_hidden,
            _ => {}
        }
    }

    fn apply_visuals(&mut self, ctx: &egui::Context) {
        let key = (self.sh.prefs.light, self.sh.prefs.ui_scale);
        if self.visuals == Some(key) {
            return;
        }
        self.visuals = Some(key);
        let mut v = if key.0 { egui::Visuals::light() } else { egui::Visuals::dark() };
        let accent = egui::Color32::from_rgb(61, 143, 230);
        v.selection.bg_fill = accent;
        if !key.0 {
            v.panel_fill = egui::Color32::from_gray(50);
            v.window_fill = egui::Color32::from_gray(46);
            v.extreme_bg_color = egui::Color32::from_gray(34);
            v.faint_bg_color = egui::Color32::from_gray(56);
        } else {
            v.panel_fill = egui::Color32::from_gray(236);
            v.window_fill = egui::Color32::from_gray(244);
        }
        ctx.set_visuals(v);
        ctx.style_mut(|s| {
            s.spacing.item_spacing = egui::vec2(6.0, 5.0);
            s.spacing.button_padding = egui::vec2(7.0, 4.0);
            s.spacing.slider_width = 110.0;
            s.interaction.tooltip_delay = 0.4;
        });
        if (key.1 - 1.0).abs() > 0.01 {
            ctx.set_zoom_factor(key.1.clamp(0.6, 2.0));
        }
    }
}

pub fn safe_name(s: &str) -> String {
    let t: String = s.chars().map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
    let t = t.trim().to_string();
    if t.is_empty() { "Без названия".into() } else { t }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.now = ctx.input(|i| i.time);
        self.apply_visuals(ctx);
        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_sync();
        }
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
        if self.focused && !focused {
            self.force_save = true;
            self.sh.save();
        }
        self.focused = focused;
        let mut action: Option<String> = None;
        if let Some(ed) = &mut self.ed {
            action = ed.keys(ctx, &mut self.sh).map(|a| a.to_string());
            if let Some((m, t)) = ed.toast.take() {
                self.toast = Some((m, t));
            }
        }
        if let Some(a) = action {
            self.dispatch(&a);
        }
        if self.ed.is_some() {
            self.editor_ui(ctx);
        } else {
            self.gallery_ui(ctx);
        }
        self.dialogs_ui(ctx);
        self.toast_ui(ctx);
        self.autosave();
        if let Some(ed) = &mut self.ed {
            ed.doc.time += ctx.input(|i| i.stable_dt).min(0.5) as f64;
        }
        crate::autotest::AutoTest::after(self, ctx);
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        crate::autotest::AutoTest::input(self, raw);
    }
}

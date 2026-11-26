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

/// Зачем открываются файлы.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Purpose { Import, InsertImage, Brushes, Palette, TipImage, GrainImage }

impl Purpose {
    fn spec(self) -> (&'static str, &'static str, &'static [&'static str], bool) {
        match self {
            Purpose::Import => ("Открыть", "Изображения и проекты", &["png", "jpg", "jpeg", "jfif", "jpe", "webp", "bmp", "gif", "tif", "tiff", "tga", "psd", "mazok", "zip"], true),
            Purpose::InsertImage => ("Добавить фото или картинку", "Фото и картинки", &["png", "jpg", "jpeg", "jfif", "jpe", "webp", "bmp", "gif", "tif", "tiff", "tga"], true),
            Purpose::Brushes => ("Импорт кистей", "Кисти Photoshop и Clip Studio", &["abr", "sut"], true),
            Purpose::Palette => ("Импорт палитры", "Палитры", &crate::palette::EXTS, true),
            Purpose::TipImage => ("Форма отпечатка из картинки", "Изображения", &["png", "jpg", "jpeg", "webp", "bmp"], false),
            Purpose::GrainImage => ("Текстура из картинки", "Изображения", &["png", "jpg", "jpeg", "webp", "bmp"], false),
        }
    }
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    fn mimes(self) -> &'static str {
        match self {
            Purpose::InsertImage | Purpose::TipImage | Purpose::GrainImage => "image/*",
            _ => "*/*",
        }
    }
}

/// Ожидание результата системного диалога (Android).
pub enum Pending {
    Open(Purpose),
    Save { bytes: Vec<u8>, name: String, mime: String },
    Folder,
}

pub struct App {
    pub sh: Shared,
    pub pending: Option<Pending>,
    /// Наведены ли файлы на окно (перетаскивание).
    pub drop_hover: bool,
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
        layer_seq: d.layer_seq, created: d.created, modified: d.modified, strokes: d.strokes, time: d.time, guide: d.guide.clone(), sheet: d.sheet,
    }
}

pub fn write_doc(dir: &Path, d: &Document) -> Result<(), String> {
    let _ = std::fs::create_dir_all(dir);
    let bytes = formats::save_mazok(d);
    let tmp = dir.join(format!("{}.tmp", d.id));
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(format!("{}.mazok", d.id))).map_err(|e| e.to_string())?;
    let sr = d.sheet_rect();
    let item = GalleryItem { id: d.id.clone(), name: d.name.clone(), w: sr.w as usize, h: sr.h as usize, modified: d.modified };
    let _ = std::fs::write(dir.join(format!("{}.json", d.id)), serde_json::to_vec(&item).unwrap_or_default());
    let thumb = doc::flatten(d, true).crop(d.sheet_rect());
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
        crate::platform::install_paste_hook();
        setup_fonts(&cc.egui_ctx);
        let mut app = App {
            sh, pending: None, drop_hover: false, ed: None, items: vec![], tex: HashMap::new(), ui: UiState::default(), saver: None, fonts: platform::fonts(), toast: None,
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
        self.create_doc_margin(name, w, h, bg, bg_visible, 0.0);
    }

    /// Новый холст: лист w×h и поле для рисования вокруг него (margin — доля листа с каждой стороны).
    pub fn create_doc_margin(&mut self, name: &str, w: usize, h: usize, bg: [u8; 3], bg_visible: bool, margin: f32) {
        let (mx, my) = ((w as f32 * margin).round() as usize, (h as f32 * margin).round() as usize);
        let mut d = Document::new(name, w + 2 * mx, h + 2 * my, bg, bg_visible);
        if mx > 0 || my > 0 {
            d.sheet = Some(util::Rect::new(mx as i32, my as i32, w as i32, h as i32));
        }
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

    /* ---------- открытие файлов ---------- */

    pub fn import_files(&mut self) {
        self.request_open(Purpose::Import);
    }

    pub fn insert_image(&mut self) {
        self.request_open(Purpose::InsertImage);
    }

    /// Открыть системный диалог выбора файлов для цели purpose.
    pub fn request_open(&mut self, purpose: Purpose) {
        let (title, name, exts, multiple) = purpose.spec();
        #[cfg(not(target_os = "android"))]
        {
            let files = platform::pick_files(title, name, exts, multiple, &mut self.sh.prefs.last_dir);
            self.handle_open(purpose, files);
        }
        #[cfg(target_os = "android")]
        {
            let _ = (title, name, exts);
            // для картинок — системный выбор фото (галерея), для остального — выбор файлов
            let r = if purpose.mimes() == "image/*" { platform::android::open_images(multiple) } else { platform::android::open(purpose.mimes(), multiple) };
            match r {
                Ok(()) => self.pending = Some(Pending::Open(purpose)),
                Err(e) => self.toast(format!("Не удалось открыть выбор файлов: {e}")),
            }
        }
    }

    /// Обработать выбранные или перетащенные файлы.
    pub fn handle_open(&mut self, purpose: Purpose, files: Vec<(String, Vec<u8>)>) {
        if files.is_empty() { return; }
        match purpose {
            Purpose::Import => {
                let mut last = None;
                for (name, bytes) in files {
                    match formats::import(&bytes, &name) {
                        Ok(d) => { let _ = write_doc(&self.docs_dir, &d); last = Some(d); }
                        Err(e) => self.toast(format!("{name}: {e}")),
                    }
                }
                self.refresh_gallery();
                if let Some(d) = last { self.open_document(d); }
            }
            Purpose::InsertImage => {
                for (name, bytes) in files {
                    match formats::decode_image(&bytes) {
                        Ok(img) => {
                            let stem = Path::new(&name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                            self.add_image(img, &stem);
                        }
                        Err(e) => self.toast(e),
                    }
                }
            }
            Purpose::Brushes => {
                let mut total = 0;
                let mut first = None;
                for (name, bytes) in files {
                    match crate::brushio::import(&bytes, &name) {
                        Ok(list) => { total += list.len(); let f = self.sh.brushes.add_imported(list); first = first.or(f); }
                        Err(e) => self.toast(format!("{name}: {e}")),
                    }
                }
                if total > 0 {
                    self.sh.brushes.save();
                    self.ui.brush_cat = "imported".into();
                    if let Some(id) = first {
                        // новая кисть — сразу в руку (для инструмента «Кисть»)
                        self.sh.tools.brush.brush_id = id;
                        if let Some(ed) = &mut self.ed { if ed.tool != Tool::Brush { ed.set_tool(Tool::Brush); } }
                    }
                    self.toast(format!("Импортировано кистей: {total} (набор «Импорт»)"));
                }
            }
            Purpose::Palette => {
                let mut n = 0;
                for (name, bytes) in files {
                    match crate::palette::import(&bytes, &name) {
                        Ok(list) => { for p in list { self.sh.palettes.push(p); n += 1; } }
                        Err(e) => self.toast(format!("{name}: {e}")),
                    }
                }
                if n > 0 {
                    self.sh.palette = self.sh.palettes.len() - 1;
                    self.sh.save();
                    self.toast(format!("Добавлено палитр: {n}"));
                }
            }
            Purpose::TipImage | Purpose::GrainImage => {
                let Some((_, bytes)) = files.into_iter().next() else { return };
                let Ok(img) = formats::decode_image(&bytes) else { self.toast("Не удалось открыть картинку"); return };
                let Some(ed) = &self.ed else { return };
                let id = crate::editor::tool_state(&self.sh, ed.tool).brush_id.clone();
                let mut b = self.sh.brushes.get(&id).clone();
                if purpose == Purpose::TipImage {
                    b.shape = crate::brush::add_tip_image(crate::brush::tip_mask_from_image(&img));
                } else {
                    b.grain = Some(crate::brush::add_grain_image(crate::brush::grain_from_image(&img)));
                    if b.grain_strength <= 0.0 { b.grain_strength = 0.6; }
                }
                self.sh.brushes.update(b);
                self.sh.brushes.save();
                self.toast(if purpose == Purpose::TipImage { "Форма отпечатка обновлена" } else { "Текстура кисти обновлена" });
            }
        }
    }

    /// Картинка в новый слой (в редакторе) или новый холст (в галерее).
    pub fn add_image(&mut self, img: Img, name: &str) {
        if let Some(ed) = &mut self.ed {
            ed.layer_add_image(img, if name.is_empty() { "Вставка" } else { name });
            ed.set_tool(Tool::Transform);
            self.toast("Добавлено новым слоем — подвиньте и примените: Enter или ещё раз «Трансформация»");
        } else {
            // большое фото уменьшается с сохранением пропорций (не больше 8192 px по стороне и 36 Мп)
            let k = (8192.0 / img.w.max(img.h).max(1) as f32).min((36e6 / (img.w * img.h).max(1) as f32).sqrt()).min(1.0);
            let img = if k < 1.0 { img.resize(((img.w as f32 * k) as usize).max(1), ((img.h as f32 * k) as usize).max(1)) } else { img };
            let (w, h) = (img.w, img.h);
            let mut d = Document::new(if name.is_empty() { "Вставка" } else { name }, w, h, [255, 255, 255], true);
            d.layers[0].img = std::sync::Arc::new(img);
            let _ = write_doc(&self.docs_dir, &d);
            self.refresh_gallery();
            self.open_document(d);
        }
    }

    /// Файлы по расширению: кисти, палитры, проекты или картинки.
    pub fn open_by_ext(&mut self, files: Vec<(String, Vec<u8>)>) {
        let mut groups: Vec<(Purpose, Vec<(String, Vec<u8>)>)> = vec![];
        for (name, bytes) in files {
            let ext = Path::new(&name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let p = match ext.as_str() {
                "abr" | "sut" => Purpose::Brushes,
                e if crate::palette::EXTS.contains(&e) => Purpose::Palette,
                "psd" | "mazok" | "zip" => Purpose::Import,
                _ => if self.ed.is_some() { Purpose::InsertImage } else { Purpose::Import },
            };
            match groups.iter_mut().find(|g| g.0 == p) { Some(g) => g.1.push((name, bytes)), None => groups.push((p, vec![(name, bytes)])) }
        }
        for (p, f) in groups { self.handle_open(p, f); }
    }

    /* ---------- буфер обмена ---------- */

    pub fn clipboard_copy(&mut self, merged: bool) {
        let Some(ed) = &mut self.ed else { return };
        ed.copy(merged);
        if let Some((img, _, _)) = &ed.clipboard {
            platform::set_clipboard_image(img);
        }
    }

    pub fn clipboard_cut(&mut self) {
        let Some(ed) = &mut self.ed else { return };
        ed.cut();
        if let Some((img, _, _)) = &ed.clipboard {
            platform::set_clipboard_image(img);
        }
    }

    /// Вставка: картинка из системного буфера, скопированные файлы, путь к файлу или внутренний буфер.
    pub fn clipboard_paste(&mut self, text: Option<String>) {
        let ours = platform::clipboard_is_ours();
        if let Some(ed) = &mut self.ed {
            if ours && ed.clipboard.is_some() { ed.paste(); return; }
        }
        if let Some(img) = platform::clipboard_image() {
            self.add_image(img, "Вставка");
            return;
        }
        let files = platform::clipboard_files();
        if !files.is_empty() {
            let f = platform::read_paths(&files);
            self.open_by_ext(f);
            return;
        }
        if let Some(t) = text {
            let p = PathBuf::from(t.trim().trim_matches('"').trim_start_matches("file:///"));
            if p.is_file() {
                let f = platform::read_paths(&[p]);
                self.open_by_ext(f);
                return;
            }
        }
        match &mut self.ed {
            Some(ed) if ed.clipboard.is_some() => ed.paste(),
            Some(_) => self.toast("В буфере обмена нет изображения"),
            None => self.toast("Скопируйте картинку и нажмите Ctrl+V, чтобы создать из неё холст"),
        }
    }

    /* ---------- сохранение ---------- */

    pub fn save_result(&mut self, r: Result<String, String>) {
        match r {
            Ok(m) => self.toast(m),
            Err(e) if e.is_empty() => {}
            Err(e) => self.toast(e),
        }
    }

    /// Сохранить файл: в папку для экспорта (если включено) или через диалог «Сохранить как».
    pub fn save_file(&mut self, name: &str, mime: &str, bytes: Vec<u8>, ask: bool) {
        let dir = self.sh.prefs.export_dir.clone();
        if !ask && self.sh.prefs.quick_export && !dir.is_empty() {
            let r = platform::write_in_folder(&dir, name, mime, &bytes);
            if r.is_ok() { self.save_result(r); return; }
            if let Err(e) = r { self.toast(e); }
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = mime;
            let r = platform::save_dialog(name, &bytes, &mut self.sh.prefs.last_dir);
            self.save_result(r);
        }
        #[cfg(target_os = "android")]
        {
            match platform::android::create(name, mime) {
                Ok(()) => self.pending = Some(Pending::Save { bytes, name: name.to_string(), mime: mime.to_string() }),
                Err(_) => { let r = platform::save_bytes(name, mime, &bytes); self.save_result(r); }
            }
        }
    }

    /// Выбрать папку для быстрого экспорта.
    pub fn pick_export_dir(&mut self) {
        #[cfg(not(target_os = "android"))]
        {
            if let Some(d) = platform::pick_folder(&self.sh.prefs.export_dir) {
                self.sh.prefs.export_dir = d;
                self.sh.prefs.quick_export = true;
                self.sh.save();
            }
        }
        #[cfg(target_os = "android")]
        {
            match platform::android::tree() {
                Ok(()) => self.pending = Some(Pending::Folder),
                Err(e) => self.toast(format!("Не удалось открыть выбор папки: {e}")),
            }
        }
    }

    /// Результат системного диалога (Android).
    fn poll_pending(&mut self, ctx: &egui::Context) {
        if self.pending.is_none() { return; }
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        #[cfg(target_os = "android")]
        {
            let Some(r) = platform::android::poll() else { return };
            let Some(p) = self.pending.take() else { return };
            if !r.ok || r.uris.is_empty() { return; }
            match p {
                Pending::Open(purpose) => {
                    // фото: HEIC и очень большие снимки декодирует сама система (с уменьшением и поворотом по EXIF)
                    let limit = if purpose == Purpose::Import { 8192 } else { 4096 };
                    let images = matches!(purpose, Purpose::Import | Purpose::InsertImage | Purpose::TipImage | Purpose::GrainImage);
                    let files: Vec<(String, Vec<u8>)> = r.uris.iter().filter_map(|u| {
                        platform::android::read(u).map(|f| if images { platform::android::prepare_image(u, f, limit) } else { f })
                    }).collect();
                    if files.is_empty() { self.toast("Не удалось прочитать файл"); }
                    self.handle_open(purpose, files);
                }
                Pending::Save { bytes, name, .. } => {
                    let res = platform::android::write(&r.uris[0], &bytes).map(|_| format!("Сохранено: {name}"));
                    self.save_result(res);
                }
                Pending::Folder => {
                    self.sh.prefs.export_dir = r.uris[0].clone();
                    self.sh.prefs.quick_export = true;
                    self.sh.save();
                    self.toast(format!("Папка для сохранения: {}", platform::folder_label(&r.uris[0])));
                }
            }
        }
    }

    pub fn export_doc_from_gallery(&mut self, id: &str) {
        if let Ok(b) = std::fs::read(self.docs_dir.join(format!("{id}.mazok"))) {
            let name = self.items.iter().find(|i| i.id == id).map(|i| i.name.clone()).unwrap_or_else(|| "Рисунок".into());
            self.save_file(&format!("{}.mazok", safe_name(&name)), "application/zip", b, true);
        }
    }

    /* ---------- экспорт ---------- */

    /// Экспорт с настройками по умолчанию; ask — всегда спрашивать место («Сохранить как»).
    pub fn export_as(&mut self, kind: &str, ask: bool) {
        let o = ExportOpts { kind: EXPORT_KINDS.iter().find(|k| k.0 == kind).map_or("png", |k| k.0), ..ExportOpts::default() };
        self.export_with(&o, ask);
    }

    /// Экспорт листа (или всего холста с полем) в выбранный формат.
    pub fn export_with(&mut self, o: &ExportOpts, ask: bool) {
        let Some(ed) = &mut self.ed else { return };
        ed.commit_pending();
        let full = &ed.doc;
        // по умолчанию — только лист: нарисованное на поле вокруг него в файл не попадает
        let d = if o.sheet_only { full.cropped_to_sheet() } else { snapshot(full) };
        let base = safe_name(&d.name);
        let k = (o.scale / 100.0).clamp(0.05, 8.0);
        let scaled = |img: Img| -> Img {
            if (k - 1.0).abs() < 1e-3 { img } else { img.resize(((img.w as f32 * k).round() as usize).max(1), ((img.h as f32 * k).round() as usize).max(1)) }
        };
        let flat = || scaled(doc::flatten_export(&d, o.bg));
        let paper = if d.bg_visible && o.bg { d.bg } else { [255, 255, 255] };
        let (ext, mime) = EXPORT_KINDS.iter().find(|x| x.0 == o.kind).map_or(("png", "image/png"), |x| (x.2, x.3));
        let name = format!("{base}.{ext}");
        let out: Result<Vec<u8>, String> = match o.kind {
            "png" | "webp" | "bmp" | "tiff" | "tga" => formats::encode(&flat(), o.kind),
            "jpeg" => Ok(formats::jpeg_q(&flat(), paper, o.quality)),
            "pdf" => Ok(formats::pdf(&flat(), paper, o.quality, o.dpi)),
            "psd" => Ok(formats::psd_write(&d)),
            "mazok" => Ok(formats::save_mazok(full)),
            "layers" => {
                let files: Vec<(String, Vec<u8>)> = d.layers.iter().filter(|l| !l.group).enumerate().map(|(i, l)| (format!("{:02} {}.png", i + 1, safe_name(&l.name)), formats::png(&scaled(l.baked())))).collect();
                Ok(formats::zip_write(&files))
            }
            "gif" => {
                // 100 % — как раньше, не больше 720 точек; иначе — выбранный масштаб
                let kk = if (k - 1.0).abs() < 1e-3 { (720.0 / d.w.max(d.h) as f32).min(1.0) } else { k };
                let (w, h) = (((d.w as f32 * kk) as usize).clamp(1, 4096), ((d.h as f32 * kk) as usize).clamp(1, 4096));
                let frames: Vec<Img> = d.layers.iter().filter(|l| !l.group && !l.draft).map(|l| {
                    let mut f = if d.bg_visible && o.bg { Img::filled(d.w, d.h, [d.bg[0], d.bg[1], d.bg[2], 255]) } else { Img::new(d.w, d.h) };
                    if l.visible { f.draw_over(0, 0, &l.baked(), l.opacity); }
                    f.resize(w, h)
                }).collect();
                if frames.is_empty() { Err("Нет слоёв для кадров".into()) } else { formats::gif(&frames, 12) }
            }
            _ => Err(String::new()),
        };
        let name = if o.kind == "layers" { format!("{base} — слои.zip") } else { name };
        match out {
            Ok(bytes) => self.save_file(&name, mime, bytes, ask),
            Err(e) => self.save_result(Err(e)),
        }
    }

    pub fn export(&mut self, kind: &str) {
        self.export_as(kind, false);
    }

    /// Экспорт палитры.
    pub fn export_palette(&mut self, i: usize, kind: &str) {
        let Some(p) = self.sh.palettes.get(i).cloned() else { return };
        let (name, mime, bytes) = crate::palette::export(&p, kind);
        self.save_file(&name, mime, bytes, true);
    }
    /* ---------- команды ---------- */

    pub fn act(&mut self, a: &str) {
        match a {
            "new" => self.ui.new_canvas = Some(crate::ui::NewCanvas::default()),
            "open" => self.import_files(),
            "save" => { self.save_sync(); self.toast("Сохранено"); }
            "gallery" => self.to_gallery(),
            "insert_image" => self.insert_image(),
            "save_as" => self.export_as("mazok", true),
            "export_dialog" => { let o = self.ui.last_export.clone().unwrap_or_default(); self.ui.export = Some(o); }
            "quick_png" => self.export_as("png", false),
            "import_brushes" => self.request_open(Purpose::Brushes),
            "import_palette" => self.request_open(Purpose::Palette),
            "tip_image" => self.request_open(Purpose::TipImage),
            "grain_image" => self.request_open(Purpose::GrainImage),
            "pick_export_dir" => self.pick_export_dir(),
            "paste" => self.clipboard_paste(None),
            "copy" => self.clipboard_copy(false),
            "copy_merged" => self.clipboard_copy(true),
            "cut" => self.clipboard_cut(),
            _ => {}
        }
        let sh = &mut self.sh;
        let Some(ed) = &mut self.ed else { return };
        match a {
            "undo" => ed.undo(),
            "redo" => ed.redo(),
            "layer_new" => ed.layer_add(),
            "layer_new_vector" => ed.layer_add_vector(),
            "layer_folder" => ed.layer_add_folder(),
            "group" => ed.group_selected(),
            "ungroup" => ed.ungroup(),
            "layer_up" => ed.layer_step(true),
            "layer_down" => ed.layer_step(false),
            "layer_sel_up" => ed.layer_select_step(true),
            "layer_sel_down" => ed.layer_select_step(false),
            "rasterize" => ed.layer_rasterize(),
            "sel_all" => ed.select_all(),
            "sel_none" => ed.select_none(),
            "sel_invert" => ed.select_invert(),
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
            o if o.starts_with("opacity_") => {
                let v: f32 = o[8..].parse().unwrap_or(10.0);
                let ts = ed.tool_state_mut(sh);
                ts.opacity = (v / 10.0).clamp(0.01, 1.0);
                ed.toast(&format!("Непрозрачность: {:.0}%", v * 10.0));
            }
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
            v.panel_fill = egui::Color32::from_gray(30);
            v.window_fill = egui::Color32::from_gray(38);
            v.extreme_bg_color = egui::Color32::from_gray(22);
            v.faint_bg_color = egui::Color32::from_gray(48);
            v.widgets.noninteractive.fg_stroke.color = egui::Color32::from_gray(205);
            v.widgets.inactive.fg_stroke.color = egui::Color32::from_gray(225);
            v.widgets.inactive.weak_bg_fill = egui::Color32::from_gray(52);
            v.widgets.inactive.bg_fill = egui::Color32::from_gray(52);
            v.window_corner_radius = egui::CornerRadius::same(12);
            v.menu_corner_radius = egui::CornerRadius::same(10);
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
            // крупнее и чётче: на телефоне мелкий текст сливается
            let k = if cfg!(target_os = "android") { 1.08 } else { 1.0 };
            use egui::{FontFamily, FontId, TextStyle};
            s.text_styles = [
                (TextStyle::Small, FontId::new(11.5 * k, FontFamily::Proportional)),
                (TextStyle::Body, FontId::new(14.0 * k, FontFamily::Proportional)),
                (TextStyle::Button, FontId::new(14.0 * k, FontFamily::Proportional)),
                (TextStyle::Heading, FontId::new(20.0 * k, FontFamily::Proportional)),
                (TextStyle::Monospace, FontId::new(13.0 * k, FontFamily::Monospace)),
            ].into();
            if cfg!(target_os = "android") {
                s.spacing.interact_size.y = 30.0;
                s.spacing.button_padding = egui::vec2(9.0, 6.0);
            }
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

impl App {
    /// Ctrl+C / Ctrl+X / Ctrl+V (egui сообщает их событиями, а не клавишами).
    fn clipboard_events(&mut self, ctx: &egui::Context) {
        let (mut copy, mut cut, mut paste) = (false, false, None);
        let shift = ctx.input(|i| i.modifiers.shift);
        ctx.input(|i| for e in &i.events {
            match e {
                egui::Event::Copy => copy = true,
                egui::Event::Cut => cut = true,
                egui::Event::Paste(t) => paste = Some(t.clone()),
                _ => {}
            }
        });
        let key = platform::take_paste_request();
        if ctx.wants_keyboard_input() || self.ui.text.is_some() { return; }
        if copy { self.clipboard_copy(shift); }
        if cut { self.clipboard_cut(); }
        if key || paste.is_some() { self.clipboard_paste(paste); }
    }

    /// Файлы, перетащенные на окно.
    fn dropped_files(&mut self, ctx: &egui::Context) {
        let (hover, dropped) = ctx.input(|i| (!i.raw.hovered_files.is_empty(), i.raw.dropped_files.clone()));
        self.drop_hover = hover;
        if dropped.is_empty() { return; }
        let files: Vec<(String, Vec<u8>)> = dropped.into_iter().filter_map(|f| {
            let name = f.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or(f.name.clone());
            let bytes = match (&f.bytes, &f.path) { (Some(b), _) => b.to_vec(), (None, Some(p)) => std::fs::read(p).ok()?, _ => return None };
            Some((name, bytes))
        }).collect();
        self.open_by_ext(files);
    }
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
        self.poll_pending(ctx);
        #[cfg(target_os = "android")]
        if self.now - self.ui.safe_t > 1.0 {
            self.ui.safe_t = self.now;
            if let Some(px) = platform::android::insets() {
                let k = ctx.pixels_per_point().max(0.5);
                self.ui.safe = px.map(|v| (v.max(0) as f32 / k).min(120.0));
            }
        }
        self.clipboard_events(ctx);
        self.dropped_files(ctx);
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
        // на Android атлас шрифта не шире 4096: меньше данных при догрузке символов
        if cfg!(target_os = "android") { raw.max_texture_side = Some(raw.max_texture_side.unwrap_or(4096).min(4096)); }
        crate::autotest::AutoTest::input(self, raw);
    }
}

/// Системный шрифт интерфейса: Roboto на Android, Segoe UI на Windows (чётче встроенного Ubuntu Light).
fn setup_fonts(ctx: &egui::Context) {
    let candidates: &[&str] = if cfg!(target_os = "android") {
        &["/system/fonts/RobotoStatic-Regular.ttf", "/system/fonts/Roboto-Regular.ttf", "/system/fonts/NotoSans-Regular.ttf", "/system/fonts/DroidSans.ttf"]
    } else {
        &[r"C:\Windows\Fonts\segoeui.ttf", r"C:\Windows\Fonts\arial.ttf"]
    };
    let Some(bytes) = candidates.iter().find_map(|p| std::fs::read(p).ok()) else { return };
    // шрифт должен открываться и содержать кириллицу
    let ok = ab_glyph::FontRef::try_from_slice(&bytes).map_or(false, |f| { use ab_glyph::Font; f.glyph_id('Ж').0 != 0 && f.glyph_id('a').0 != 0 });
    if !ok { return; }
    let mut defs = egui::FontDefinitions::default();
    defs.font_data.insert("system-ui".into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
    if let Some(f) = defs.families.get_mut(&egui::FontFamily::Proportional) { f.insert(0, "system-ui".into()); }
    ctx.set_fonts(defs);
}

/// Форматы экспорта: (id, название, расширение, MIME).
pub const EXPORT_KINDS: [(&str, &str, &str, &str); 11] = [
    ("png", "PNG", "png", "image/png"),
    ("jpeg", "JPEG", "jpg", "image/jpeg"),
    ("webp", "WebP", "webp", "image/webp"),
    ("pdf", "PDF", "pdf", "application/pdf"),
    ("psd", "PSD (слои)", "psd", "image/vnd.adobe.photoshop"),
    ("tiff", "TIFF", "tif", "image/tiff"),
    ("bmp", "BMP", "bmp", "image/bmp"),
    ("tga", "TGA", "tga", "image/x-tga"),
    ("gif", "GIF (кадры)", "gif", "image/gif"),
    ("layers", "Слои PNG (ZIP)", "zip", "application/zip"),
    ("mazok", "Проект .mazok", "mazok", "application/zip"),
];

/// Настройки экспорта.
#[derive(Clone, Debug)]
pub struct ExportOpts {
    pub kind: &'static str,
    /// Только лист (иначе весь холст вместе с полем вокруг листа).
    pub sheet_only: bool,
    /// Масштаб в процентах.
    pub scale: f32,
    /// С фоном (иначе прозрачный, если формат умеет).
    pub bg: bool,
    pub quality: u8,
    pub dpi: f32,
}

impl Default for ExportOpts {
    fn default() -> Self {
        ExportOpts { kind: "png", sheet_only: true, scale: 100.0, bg: true, quality: 92, dpi: 300.0 }
    }
}

//! Встроенный автотест интерфейса: запускается, если задана переменная окружения MAZOK_AUTOTEST=<папка>.
//! Рисует штрихи настоящими событиями мыши, применяет инструменты и фильтры, проверяет форматы
//! и сохраняет скриншоты окна.

use crate::app::App;
use crate::doc::{self, TextSpec};
use crate::editor::{ShapeKind, Tool};
use crate::formats;
use crate::raster::Img;
use egui::{Event, Pos2, RawInput};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

type Action = Box<dyn FnOnce(&mut App, &mut AutoLog)>;

enum Step {
    Run(Action),
    Stroke(Vec<(f32, f32)>),
    Click(f32, f32),
    Wait(u32),
    Shot(&'static str),
    Quit,
}

#[derive(Default)]
pub struct AutoLog {
    pub lines: Vec<String>,
    pub fails: usize,
}
impl AutoLog {
    pub fn check(&mut self, ok: bool, what: impl Into<String>) {
        let w = what.into();
        if !ok { self.fails += 1; }
        self.lines.push(format!("{} {}", if ok { "OK  " } else { "FAIL" }, w));
    }
    pub fn info(&mut self, s: impl Into<String>) { self.lines.push(format!("     {}", s.into())); }
}

pub struct AutoTest {
    pub dir: PathBuf,
    steps: VecDeque<Step>,
    stroke: Option<(Vec<Pos2>, usize)>,
    wait: u32,
    shots: usize,
    pub log: AutoLog,
    stroke_frames: Vec<f64>,
    last: Option<Instant>,
    quit: bool,
    done: bool,
    start: Instant,
}

fn wave(y: f32, amp: f32, x0: f32, x1: f32, n: usize) -> Vec<(f32, f32)> {
    (0..=n).map(|i| { let t = i as f32 / n as f32; (x0 + (x1 - x0) * t, y + (t * std::f32::consts::TAU * 1.5).sin() * amp) }).collect()
}
fn circle(cx: f32, cy: f32, r: f32, n: usize) -> Vec<(f32, f32)> {
    (0..=n + 2).map(|i| { let a = i as f32 / n as f32 * std::f32::consts::TAU; (cx + a.cos() * r * 0.75, cy + a.sin() * r) }).collect()
}

impl AutoTest {
    pub fn from_env() -> Option<AutoTest> {
        let dir = PathBuf::from(std::env::var_os("MAZOK_AUTOTEST")?);
        let _ = std::fs::create_dir_all(&dir);
        let mut s: VecDeque<Step> = VecDeque::new();
        let run = |f: Action| Step::Run(f);
        s.push_back(Step::Wait(8));
        s.push_back(Step::Shot("01-gallery-empty.png"));
        s.push_back(run(Box::new(|a, l| { a.create_doc("Автотест", 1600, 1200, [255, 255, 255], true); l.check(a.ed.is_some(), "создан холст 1600×1200"); })));
        s.push_back(Step::Wait(6));
        s.push_back(run(Box::new(|a, _| brush(a, "oil", 0.55, [30, 60, 150]))));
        s.push_back(Step::Stroke(wave(0.3, 0.08, 0.1, 0.9, 60)));
        s.push_back(run(Box::new(|a, _| brush(a, "watercolor", 0.7, [220, 50, 60]))));
        s.push_back(Step::Stroke((0..=40).map(|i| { let t = i as f32 / 40.0; (0.15 + 0.7 * t, 0.85 - 0.6 * t) }).collect()));
        s.push_back(run(Box::new(|a, l| {
            let ed = a.ed.as_mut().unwrap();
            let n = ed.doc.layers[ed.doc.active].img.content_bounds();
            l.check(n.is_some(), "штрихи кистью нанесены на слой");
            l.check(ed.hist.pos == 2, format!("история: 2 штриха (pos = {})", ed.hist.pos));
            ed.layer_add();
            brush(a, "pencil_hb", 0.35, [20, 20, 20]);
        })));
        s.push_back(Step::Stroke(circle(0.5, 0.55, 0.18, 70)));
        s.push_back(run(Box::new(|a, _| { let ed = a.ed.as_mut().unwrap(); ed.set_tool(Tool::Fill); a.sh.colors.set([250, 200, 40]); })));
        s.push_back(Step::Click(0.5, 0.55));
        s.push_back(Step::Wait(4));
        s.push_back(run(Box::new(|a, l| {
            let ed = a.ed.as_mut().unwrap();
            let img = &ed.doc.layers[ed.doc.active].img;
            let (x, y) = (ed.doc.w / 2, (ed.doc.h as f32 * 0.55) as usize);
            let i = img.idx(x, y);
            l.check(img.px[i + 3] > 200 && img.px[i] > 200 && img.px[i + 2] < 120, format!("заливка внутри контура (пиксель {:?})", &img.px[i..i + 4]));
            ed.layer_add();
            ed.set_tool(Tool::Shapes);
            ed.shape_kind = ShapeKind::Star;
            ed.shape_fill = true;
            a.sh.colors.set([60, 170, 90]);
        })));
        s.push_back(Step::Stroke(vec![(0.72, 0.12), (0.76, 0.18), (0.82, 0.26), (0.88, 0.34)]));
        s.push_back(run(Box::new(|a, l| {
            let ed = a.ed.as_mut().unwrap();
            l.check(ed.doc.layer().img.content_bounds().is_some(), "фигура «звезда» нарисована");
            ed.set_tool(Tool::Smudge);
            let ts = ed.tool_state_mut(&mut a.sh);
            ts.size = 0.6;
            ed.layer_select(0);
        })));
        s.push_back(Step::Stroke(wave(0.3, 0.02, 0.2, 0.6, 30)));
        s.push_back(run(Box::new(|a, l| {
            let ed = a.ed.as_mut().unwrap();
            let before = ed.hist.pos;
            ed.filter_open("hsb");
            if let Some(f) = &mut ed.filter { f.params[0] = 40.0; f.dirty = true; }
            ed.filter_apply();
            l.check(ed.hist.pos == before + 1, "фильтр «Цветовой тон/насыщенность» применён");
            let t0 = Instant::now();
            ed.filter_open("blur");
            ed.filter_apply();
            l.info(format!("размытие слоя 1600×1200: {:.0} мс", t0.elapsed().as_secs_f64() * 1000.0));
            ed.undo();
            let font = std::fs::read("C:\\Windows\\Fonts\\arialbd.ttf").ok();
            let spec = TextSpec { str: "Мазок на Rust".into(), size: 110.0, color: "#1a3a8a".into(), x: 180.0, y: 960.0, bold: true, ..Default::default() };
            ed.apply_text(spec, None, font.as_deref());
            l.check(ed.doc.layer().text.is_some(), "текстовый слой создан");
            ed.set_tool(Tool::Brush);
        })));
        s.push_back(Step::Wait(10));
        s.push_back(Step::Shot("02-editor.png"));
        s.push_back(run(Box::new(|a, l| roundtrip(a, l))));
        s.push_back(run(Box::new(|a, l| {
            let ed = a.ed.as_mut().unwrap();
            let p = ed.hist.pos;
            ed.undo(); ed.undo(); ed.redo();
            l.check(ed.hist.pos == p - 1, format!("отмена/повтор ({} → {})", p, ed.hist.pos));
            ed.redo();
            a.ui.dock_tab[1] = 1;
            a.ui.dock_tab[2] = 0;
        })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("03-brush-settings.png"));
        s.push_back(run(Box::new(|a, _| { a.sh.prefs.ui_mode = "touch".into(); a.ui.win.layers = true; })));
        s.push_back(Step::Wait(8));
        s.push_back(Step::Shot("04-compact-layers.png"));
        s.push_back(run(Box::new(|a, _| { a.ui.win.layers = false; a.ui.win.brushes = true; })));
        s.push_back(Step::Wait(8));
        s.push_back(Step::Shot("05-compact-brushes.png"));
        s.push_back(run(Box::new(|a, l| {
            a.sh.prefs.ui_mode = "auto".into();
            a.ui.win.brushes = false;
            a.to_gallery();
            l.check(a.items.iter().any(|i| i.name == "Автотест"), "рисунок сохранён и есть в галерее");
        })));
        s.push_back(Step::Wait(8));
        s.push_back(Step::Shot("06-gallery.png"));
        s.push_back(run(Box::new(|a, l| {
            let id = a.items.first().map(|i| i.id.clone()).unwrap_or_default();
            a.open_doc(&id);
            let ok = a.ed.as_ref().map_or(false, |e| e.doc.layers.len() >= 4 && e.doc.layers.iter().any(|l| l.text.is_some()));
            l.check(ok, "рисунок снова открыт из галереи со всеми слоями");
            // векторный слой: три линии, потом ластик «до пересечения»
            a.sh.prefs.stabilizer = 0.0;
            a.ed.as_mut().unwrap().layer_add_vector();
            brush(a, "pencil_hb", 0.3, [20, 20, 30]);
        })));
        s.push_back(Step::Stroke((0..=30).map(|i| (0.15 + 0.7 * i as f32 / 30.0, 0.5)).collect()));
        s.push_back(Step::Stroke((0..=30).map(|i| (0.4, 0.2 + 0.6 * i as f32 / 30.0)).collect()));
        s.push_back(Step::Stroke((0..=30).map(|i| (0.6, 0.2 + 0.6 * i as f32 / 30.0)).collect()));
        s.push_back(run(Box::new(|a, l| {
            let ed = a.ed.as_mut().unwrap();
            let n = ed.doc.layer().vector.as_ref().map_or(0, |v| v.len());
            l.check(n == 3, format!("векторный слой: нарисовано линий {n} из 3"));
            l.check(alpha_at(ed, 0.3, 0.5) > 150, "векторная линия видна на холсте");
            ed.set_tool(Tool::Eraser);
            a.sh.prefs.vector_erase = "intersect".into();
            let ts = ed.tool_state_mut(&mut a.sh);
            ts.size = 0.12;
        })));
        s.push_back(Step::Click(0.5, 0.5));
        s.push_back(Step::Wait(3));
        s.push_back(run(Box::new(|a, l| {
            let ed = a.ed.as_mut().unwrap();
            let n = ed.doc.layer().vector.as_ref().map_or(0, |v| v.len());
            l.check(n == 4, format!("ластик «до пересечения»: горизонталь разрезана на 2 части (линий {n})"));
            l.check(alpha_at(ed, 0.5, 0.5) < 10 && alpha_at(ed, 0.3, 0.5) > 150 && alpha_at(ed, 0.7, 0.5) > 150, format!("стёрт только участок между пересечениями ({} / {} / {})", alpha_at(ed, 0.3, 0.5), alpha_at(ed, 0.5, 0.5), alpha_at(ed, 0.7, 0.5)));
            ed.undo();
            let n1 = ed.doc.layer().vector.as_ref().map_or(0, |v| v.len());
            let a1 = alpha_at(ed, 0.5, 0.5);
            ed.redo();
            let n2 = ed.doc.layer().vector.as_ref().map_or(0, |v| v.len());
            l.check(n1 == 3 && a1 > 150 && n2 == 4, format!("отмена и повтор векторного ластика ({n1} → {n2})"));
            let mz = formats::save_mazok(&ed.doc);
            let ok = formats::load_mazok(&mz).map_or(false, |d| d.layers.iter().any(|x| x.vector.as_ref().map_or(false, |v| v.len() == 4)));
            l.check(ok, ".mazok сохраняет векторные линии");
            let before = ed.doc.layer().img.content_bounds();
            ed.resize_canvas(3200, 2400, 0, 0, true);
            let v = ed.doc.layer().vector.clone().unwrap_or_default();
            let wide = v.iter().any(|s| s.size > 3.0 * 1.5);
            l.check(before.is_some() && ed.doc.w == 3200 && wide, "увеличение холста перерисовало векторный слой без потери резкости");
            ed.undo();
            ed.fit();
        })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("07-vector-eraser.png"));
        s.push_back(run(Box::new(|a, l| clipboard_test(a, l))));
        s.push_back(Step::Wait(8));
        s.push_back(run(Box::new(|a, l| clipboard_finish(a, l))));
        s.push_back(Step::Wait(4));
        s.push_back(run(Box::new(|a, l| import_test(a, l))));
        s.push_back(run(Box::new(|a, _| { a.ui.dock_tab[0] = 1; a.ui.dock_tab[1] = 0; a.ui.brush_cat = "imported".into(); })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("08-palettes-imported-brushes.png"));
        s.push_back(run(Box::new(|a, _| {
            // стабилизатор: дрожащая линия
            a.sh.prefs.stabilizer = 0.5;
            brush(a, "pencil_hb", 0.3, [200, 40, 40]);
            a.ed.as_mut().unwrap().layer_add();
        })));
        s.push_back(Step::Stroke((0..=80).map(|i| { let t = i as f32 / 80.0; (0.1 + 0.8 * t, 0.85 + if i % 2 == 0 { 0.012 } else { -0.012 }) }).collect()));
        s.push_back(run(Box::new(|a, l| {
            let ed = a.ed.as_mut().unwrap();
            let b = ed.doc.layer().img.content_bounds();
            let h = b.map_or(0, |b| b.h);
            l.check(b.is_some() && h < 40, format!("стабилизатор сгладил дрожание (высота линии {h} px)"));
            a.sh.prefs.stabilizer = 0.0;
            a.ui.new_canvas = Some(crate::ui::NewCanvas::default());
        })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("09-new-canvas.png"));
        s.push_back(run(Box::new(|a, _| { a.ui.new_canvas = None; a.ui.settings = true; })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("10-settings.png"));
        s.push_back(run(Box::new(|a, _| { a.ui.settings = false; a.sh.prefs.light = true; })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("11-light.png"));
        s.push_back(run(Box::new(|a, _| { a.sh.prefs.light = false; })));
        s.push_back(Step::Quit);
        Some(AutoTest { dir, steps: s, stroke: None, wait: 0, shots: 0, log: AutoLog::default(), stroke_frames: vec![], last: None, quit: false, done: false, start: Instant::now() })
    }

    /// Вызывается перед кадром: подмешивает события мыши.
    pub fn input(app: &mut App, raw: &mut RawInput) {
        let Some(t) = app.test.as_mut() else { return };
        let rect = app.ed.as_ref().map(|e| e.canvas_rect);
        let to = |p: (f32, f32)| rect.map_or(Pos2::ZERO, |r| Pos2::new(r.left() + r.width() * p.0, r.top() + r.height() * p.1));
        if let Some((pts, i)) = &mut t.stroke {
            let now = Instant::now();
            if let Some(l) = t.last { t.stroke_frames.push(now.duration_since(l).as_secs_f64() * 1000.0); }
            t.last = Some(now);
            let mods = egui::Modifiers::NONE;
            if *i == 0 {
                raw.events.push(Event::PointerMoved(pts[0]));
                raw.events.push(Event::PointerButton { pos: pts[0], button: egui::PointerButton::Primary, pressed: true, modifiers: mods });
                *i = 1;
            } else if *i < pts.len() {
                let end = (*i + 3).min(pts.len());
                for p in &pts[*i..end] { raw.events.push(Event::PointerMoved(*p)); }
                *i = end;
            } else {
                let p = *pts.last().unwrap();
                raw.events.push(Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: mods });
                t.stroke = None;
                t.last = None;
            }
            return;
        }
        if t.wait > 0 { return; }
        match t.steps.front() {
            Some(Step::Stroke(_)) => {
                if let Some(Step::Stroke(p)) = t.steps.pop_front() {
                    t.stroke = Some((p.into_iter().map(to).collect(), 0));
                }
            }
            Some(Step::Click(x, y)) => {
                let p = to((*x, *y));
                raw.events.push(Event::PointerMoved(p));
                raw.events.push(Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
                raw.events.push(Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
                t.steps.pop_front();
            }
            _ => {}
        }
    }

    /// Вызывается после кадра: выполняет шаги, сохраняет скриншоты.
    pub fn after(app: &mut App, ctx: &egui::Context) {
        let Some(mut t) = app.test.take() else { return };
        ctx.input(|i| {
            for e in &i.raw.events {
                if let Event::Screenshot { image, user_data, .. } = e {
                    if let Some(name) = user_data.data.as_ref().and_then(|d| d.downcast_ref::<&'static str>()) {
                        let mut img = Img::new(image.size[0], image.size[1]);
                        for (k, c) in image.pixels.iter().enumerate() { img.px[k * 4..k * 4 + 4].copy_from_slice(&[c.r(), c.g(), c.b(), 255]); }
                        let _ = std::fs::write(t.dir.join(name), formats::png(&img));
                        t.shots = t.shots.saturating_sub(1);
                    }
                }
            }
        });
        ctx.request_repaint();
        if t.stroke.is_some() {
            app.test = Some(t);
            return;
        }
        if t.wait > 0 {
            t.wait -= 1;
            app.test = Some(t);
            return;
        }
        if t.quit {
            if !t.done && (t.shots == 0 || t.start.elapsed().as_secs() > 120) {
                t.done = true;
                finish(&mut t);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            app.test = Some(t);
            return;
        }
        match t.steps.pop_front() {
            Some(Step::Run(f)) => { let mut log = std::mem::take(&mut t.log); f(app, &mut log); t.log = log; t.wait = 2; }
            Some(Step::Wait(n)) => t.wait = n,
            Some(Step::Shot(name)) => { t.shots += 1; ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(name))); t.wait = 3; }
            Some(Step::Quit) | None => { t.quit = true; t.wait = 4; }
            Some(s) => t.steps.push_front(s),
        }
        app.test = Some(t);
    }
}

fn finish(t: &mut AutoTest) {
    if !t.stroke_frames.is_empty() {
        let mut v = t.stroke_frames.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let avg = v.iter().sum::<f64>() / v.len() as f64;
        t.log.info(format!("кадр во время рисования: среднее {:.1} мс, медиана {:.1} мс, 95% {:.1} мс ({} кадров)", avg, v[v.len() / 2], v[(v.len() * 95 / 100).min(v.len() - 1)], v.len()));
    }
    let fails = t.log.fails;
    t.log.lines.push(if fails == 0 { "ИТОГ: все проверки пройдены".into() } else { format!("ИТОГ: ошибок — {fails}") });
    let _ = std::fs::write(t.dir.join("log.txt"), t.log.lines.join("\n"));
}

fn brush(a: &mut App, id: &str, size: f32, col: [u8; 3]) {
    let ed = a.ed.as_mut().unwrap();
    ed.set_tool(Tool::Brush);
    let ts = ed.tool_state_mut(&mut a.sh);
    ts.brush_id = id.into();
    ts.size = size;
    ts.opacity = 1.0;
    a.sh.colors.set(col);
}

fn hash(img: &Img) -> u64 {
    img.px.iter().fold(1469598103934665603u64, |h, b| (h ^ *b as u64).wrapping_mul(1099511628211))
}

fn roundtrip(a: &mut App, l: &mut AutoLog) {
    let ed = a.ed.as_mut().unwrap();
    let d = &ed.doc;
    let t0 = Instant::now();
    let flat = doc::flatten(d, true);
    l.info(format!("сведение 4 слоёв 1600×1200: {:.0} мс", t0.elapsed().as_secs_f64() * 1000.0));
    let png = formats::png(&flat);
    if let Some(dir) = std::env::var_os("MAZOK_AUTOTEST") {
        let _ = std::fs::write(PathBuf::from(dir).join("export.png"), &png);
    }
    let back = formats::decode_image(&png);
    l.check(back.as_ref().map_or(false, |b| b.w == d.w && b.h == d.h), "экспорт PNG читается обратно");
    let mz = formats::save_mazok(d);
    match formats::load_mazok(&mz) {
        Ok(d2) => {
            let same = d2.layers.len() == d.layers.len() && d2.layers.iter().zip(&d.layers).all(|(a, b)| hash(&a.img) == hash(&b.img) && a.blend == b.blend && a.name == b.name);
            l.check(same, format!(".mazok: {} слоя совпадают пиксель в пиксель ({} КБ)", d2.layers.len(), mz.len() / 1024));
            l.check(d2.layers.iter().any(|x| x.text.is_some()), ".mazok: текстовый слой сохранил текст");
        }
        Err(e) => l.check(false, format!(".mazok не читается: {e}")),
    }
    let psd = formats::psd_write(d);
    match formats::import(&psd, "test.psd") {
        Ok(d3) => {
            let fl3 = doc::flatten(&d3, true);
            let diff = fl3.px.iter().zip(&flat.px).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap_or(255);
            l.check(d3.layers.len() == d.layers.len() && diff <= 3, format!("PSD: {} слоя, макс. расхождение {} ({} КБ)", d3.layers.len(), diff, psd.len() / 1024));
        }
        Err(e) => l.check(false, format!("PSD не читается: {e}")),
    }
    let jpg = formats::jpeg(&flat, [255, 255, 255]);
    l.check(formats::decode_image(&jpg).is_ok(), format!("экспорт JPEG ({} КБ)", jpg.len() / 1024));
    let frames: Vec<Img> = d.layers.iter().map(|x| x.img.resize(320, 240)).collect();
    l.check(formats::gif(&frames, 12).is_ok(), "экспорт GIF");
}

/// Прозрачность пикселя активного слоя в точке холста (доли области просмотра).
fn alpha_at(ed: &crate::editor::Editor, x: f32, y: f32) -> u8 {
    let r = ed.canvas_rect;
    let (dx, dy) = ed.view.to_doc(Pos2::new(r.left() + r.width() * x, r.top() + r.height() * y));
    let img = &ed.doc.layer().img;
    // максимум в окрестности 7×7 — у карандаша зернистая текстура
    let mut m = 0;
    for oy in -3..=3 {
        for ox in -3..=3 {
            let (x, y) = (dx as i32 + ox, dy as i32 + oy);
            if x < 0 || y < 0 || x as usize >= img.w || y as usize >= img.h { continue; }
            m = m.max(img.px[img.idx(x as usize, y as usize) + 3]);
        }
    }
    m
}

/// Вставка картинки из системного буфера обмена и копирование в него.
/// Содержимое буфера пользователя сохраняется и восстанавливается.
fn clipboard_test(a: &mut App, l: &mut AutoLog) {
    #[cfg(not(target_os = "android"))]
    {
        let mut cb = match arboard::Clipboard::new() { Ok(c) => c, Err(e) => { l.check(false, format!("буфер обмена недоступен: {e}")); return; } };
        let saved_text = cb.get_text().ok();
        let saved_img = cb.get_image().ok().map(|i| (i.width, i.height, i.bytes.into_owned()));
        let (w, h) = (120usize, 80usize);
        let mut px = vec![0u8; w * h * 4];
        for p in px.chunks_exact_mut(4) { p.copy_from_slice(&[230, 40, 40, 255]); }
        let set = cb.set_image(arboard::ImageData { width: w, height: h, bytes: std::borrow::Cow::Owned(px) }).is_ok();
        l.check(set, "в буфер обмена положена картинка другой программы");
        let n0 = a.ed.as_ref().map_or(0, |e| e.doc.layers.len());
        a.clipboard_paste(None);
        let ed = a.ed.as_mut().unwrap();
        let ok = ed.doc.layers.len() == n0 + 1 && ed.tool == Tool::Transform;
        a.act("enter");
        let ed = a.ed.as_mut().unwrap();
        let b = ed.doc.layer().img.content_bounds();
        l.check(ok && b.map_or(false, |b| b.w == w as i32 && b.h == h as i32), format!("Ctrl+V: картинка вставлена новым слоем {:?}", b.map(|b| (b.w, b.h))));
        a.clipboard_copy(false);
        let back = cb.get_image().ok().map(|i| (i.width, i.height));
        l.check(back == Some((w, h)) && crate::platform::clipboard_is_ours(), format!("Ctrl+C: слой скопирован в системный буфер {back:?}"));
        let n1 = a.ed.as_ref().map_or(0, |e| e.doc.layers.len());
        a.clipboard_paste(None);
        let ed = a.ed.as_mut().unwrap();
        l.check(ed.doc.layers.len() == n1 + 1 && ed.doc.layer().name == "Вставка", "своя копия вставляется на то же место");
        ed.layer_delete();
        ed.layer_delete();
        *CLIP_BACKUP.lock().unwrap() = Some((saved_img, saved_text));
        // настоящее нажатие Ctrl+V: картинка 90×50 из «другой программы»
        let mut px = vec![0u8; 90 * 50 * 4];
        for p in px.chunks_exact_mut(4) { p.copy_from_slice(&[40, 160, 60, 255]); }
        let _ = cb.set_image(arboard::ImageData { width: 90, height: 50, bytes: std::borrow::Cow::Owned(px) });
        let n = a.ed.as_ref().map_or(0, |e| e.doc.layers.len());
        N_BEFORE.store(n, std::sync::atomic::Ordering::Relaxed);
        #[cfg(windows)]
        l.check(winkeys::press_ctrl_v(), "окну отправлено нажатие Ctrl+V");
    }
    let _ = (a, l);
}

static CLIP_BACKUP: std::sync::Mutex<Option<(Option<(usize, usize, Vec<u8>)>, Option<String>)>> = std::sync::Mutex::new(None);
static N_BEFORE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn clipboard_finish(a: &mut App, l: &mut AutoLog) {
    #[cfg(windows)]
    winkeys::release_ctrl();
    #[cfg(windows)]
    {
        let ed = a.ed.as_mut().unwrap();
        let n0 = N_BEFORE.load(std::sync::atomic::Ordering::Relaxed);
        let b = ed.doc.layer().img.content_bounds();
        let pasted = ed.doc.layers.len() == n0 + 1;
        l.check(pasted && b.map_or(false, |b| b.w == 90 && b.h == 50), format!("клавиши Ctrl+V вставили картинку из буфера {:?}", b.map(|b| (b.w, b.h))));
        if pasted { a.act("enter"); let ed = a.ed.as_mut().unwrap(); ed.layer_delete(); }
    }
    #[cfg(not(target_os = "android"))]
    if let (Ok(mut cb), Some((img, text))) = (arboard::Clipboard::new(), CLIP_BACKUP.lock().unwrap().take()) {
        match (img, text) {
            (Some((w, h, bytes)), _) => { let _ = cb.set_image(arboard::ImageData { width: w, height: h, bytes: std::borrow::Cow::Owned(bytes) }); }
            (None, Some(t)) => { let _ = cb.set_text(t); }
            _ => { let _ = cb.clear(); }
        }
    }
    let _ = (a, l);
}

/// Нажатие Ctrl+V только для своего окна: состояние Ctrl — в таблице клавиш своего потока,
/// сообщение WM_KEYDOWN — в свою очередь сообщений. Другие программы не затрагиваются.
#[cfg(windows)]
mod winkeys {
    #[link(name = "user32")]
    extern "system" {
        fn GetKeyboardState(s: *mut u8) -> i32;
        fn SetKeyboardState(s: *const u8) -> i32;
        fn PostMessageW(h: isize, msg: u32, w: usize, l: isize) -> i32;
        fn EnumThreadWindows(tid: u32, f: unsafe extern "system" fn(isize, isize) -> i32, l: isize) -> i32;
        fn IsWindowVisible(h: isize) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
    unsafe extern "system" fn first_visible(h: isize, l: isize) -> i32 {
        if IsWindowVisible(h) != 0 { *(l as *mut isize) = h; return 0; }
        1
    }
    fn set_ctrl(down: bool) {
        let mut st = [0u8; 256];
        unsafe {
            GetKeyboardState(st.as_mut_ptr());
            for k in [0x11usize, 0xA2] { if down { st[k] |= 0x80; } else { st[k] &= 0x7f; } }
            SetKeyboardState(st.as_ptr());
        }
    }
    pub fn press_ctrl_v() -> bool {
        let mut hwnd: isize = 0;
        unsafe { EnumThreadWindows(GetCurrentThreadId(), first_visible, &mut hwnd as *mut isize as isize); }
        if hwnd == 0 { return false; }
        set_ctrl(true);
        unsafe {
            PostMessageW(hwnd, 0x0100, 0x56, 0x002F_0001);
            PostMessageW(hwnd, 0x0101, 0x56, 0xC02F_0001u32 as i32 as isize);
        }
        true
    }
    pub fn release_ctrl() {
        set_ctrl(false);
    }
}

/// Импорт кисти Photoshop (.abr) и палитр (.gpl, .ase) так, как при выборе файлов.
fn import_test(a: &mut App, l: &mut AutoLog) {
    // ABR версии 2: одна кисть-отпечаток «кольцо» 48×48
    let (w, h) = (48i32, 48i32);
    let mut body = vec![];
    body.extend(0u32.to_be_bytes());
    body.extend(30u16.to_be_bytes());
    let name: Vec<u16> = "Кольцо из ABR".encode_utf16().chain([0]).collect();
    body.extend((name.len() as u32).to_be_bytes());
    for c in name { body.extend(c.to_be_bytes()); }
    body.push(1);
    body.extend([0u8; 8]);
    for v in [0, 0, h, w] { body.extend(v.to_be_bytes()); }
    body.extend(8u16.to_be_bytes());
    body.push(0);
    for y in 0..h { for x in 0..w { let d = ((x as f32 - 23.5).powi(2) + (y as f32 - 23.5).powi(2)).sqrt(); body.push(if d > 14.0 && d < 22.0 { 255 } else { 0 }); } }
    let mut abr = vec![0, 2, 0, 1, 0, 2];
    abr.extend((body.len() as u32).to_be_bytes());
    abr.extend(body);
    let before = a.sh.brushes.custom.len();
    a.handle_open(crate::app::Purpose::Brushes, vec![("Тест.abr".into(), abr)]);
    let nb = a.sh.brushes.custom.last().cloned();
    l.check(a.sh.brushes.custom.len() == before + 1 && nb.as_ref().map_or(false, |b| b.shape.starts_with("img:") && b.name == "Кольцо из ABR"), "импорт кисти Photoshop (.abr)");
    if let Some(b) = nb {
        let ed = a.ed.as_ref().unwrap();
        let id = crate::editor::tool_state(&a.sh, ed.tool).brush_id.clone();
        l.check(id == b.id, "импортированная кисть сразу выбрана");
        let m = crate::brush::tip_mask(&b.shape);
        l.check(m[256 * 512 + 256] < 0.05 && m.iter().any(|v| *v > 0.9), "форма отпечатка — кольцо с пустым центром");
    }
    let gpl = "GIMP Palette\nName: Море\n#\n10 40 90 Глубина\n40 120 200 Волна\n230 240 250 Пена\n".as_bytes().to_vec();
    let ase = crate::palette::ase_write(&crate::state::Palette { name: "Закат".into(), colors: vec![[250, 120, 60], [200, 60, 90]], names: vec![] });
    let np = a.sh.palettes.len();
    a.handle_open(crate::app::Purpose::Palette, vec![("Море.gpl".into(), gpl), ("Закат.ase".into(), ase)]);
    l.check(a.sh.palettes.len() == np + 2 && a.sh.palettes.iter().any(|p| p.name == "Море" && p.colors.len() == 3), "импорт палитр GIMP (.gpl) и Adobe (.ase)");
}
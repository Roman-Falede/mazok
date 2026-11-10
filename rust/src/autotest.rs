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
            let ok = a.ed.as_ref().map_or(false, |e| e.doc.layers.len() == 4 && e.doc.layers.iter().any(|l| l.text.is_some()));
            l.check(ok, "рисунок снова открыт из галереи со всеми слоями");
            a.ui.new_canvas = Some(crate::ui::NewCanvas::default());
        })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("07-new-canvas.png"));
        s.push_back(run(Box::new(|a, _| { a.ui.new_canvas = None; a.ui.settings = true; })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("08-settings.png"));
        s.push_back(run(Box::new(|a, _| { a.ui.settings = false; a.sh.prefs.light = true; })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("09-light.png"));
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

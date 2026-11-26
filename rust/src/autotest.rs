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
/// Точки жеста в экранных координатах — считаются в момент начала жеста (по текущему положению панелей).
type PathFn = Box<dyn FnOnce(&App) -> Vec<Pos2>>;

/// Жест пальцем или мышью, который сейчас подаётся в программу.
struct Gesture {
    pts: Vec<Pos2>,
    i: usize,
    t0: Instant,
    hold: f64,
    touch: bool,
}

enum Step {
    Run(Action),
    Stroke(Vec<(f32, f32)>),
    Click(f32, f32),
    Wait(u32),
    Shot(&'static str),
    Size(f32, f32),
    /// Штрих пером: (x, y, нажим) — события касания с силой нажима.
    Pen(Vec<(f32, f32, f32)>),
    /// Жест в экранных координатах: удержание на месте hold секунд, потом движение по точкам.
    /// touch = палец/перо так, как их передаёт egui-winit: касание + указатель, при отрыве — PointerGone.
    Gesture(PathFn, f64, bool),
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
    pen: Option<(Vec<(Pos2, f32)>, usize)>,
    gesture: Option<Gesture>,
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
            pop(a, crate::ui::Pop::Studio);
        })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("03-brush-settings.png"));
        s.push_back(run(Box::new(|a, _| pop(a, crate::ui::Pop::Layers))));
        s.push_back(Step::Wait(8));
        s.push_back(Step::Shot("04-layers-popover.png"));
        s.push_back(run(Box::new(|a, _| pop(a, crate::ui::Pop::Brushes))));
        s.push_back(Step::Wait(8));
        s.push_back(Step::Shot("05-brush-library.png"));
        s.push_back(run(Box::new(|a, l| {
            a.ui.pop = None;
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
        s.push_back(run(Box::new(|a, _| { a.ui.color_tab = 1; pop(a, crate::ui::Pop::Color); })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("08-palettes.png"));
        s.push_back(run(Box::new(|a, _| { a.ui.brush_cat = "imported".into(); pop(a, crate::ui::Pop::Brushes); })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("08b-imported-brushes.png"));
        s.push_back(run(Box::new(|a, _| a.ui.pop = None)));
        s.push_back(run(Box::new(|a, l| layers_test(a, l))));
        s.push_back(run(Box::new(|a, _| { a.sh.prefs.ui_mode = "desk".into(); a.ui.dock_tab[2] = 0; a.ui.dock_collapsed = [false, true, false]; a.sh.prefs.layer_thumb = 48.0; })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("09-classic-layers-panel.png"));
        s.push_back(run(Box::new(|a, l| { a.ui.dock_collapsed = [false; 3]; a.sh.prefs.ui_mode = "studio".into(); layers_test2(a, l) })));
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
            // нажим пера: кисть с сужением конца — но автосужение выключено
            let id = a.sh.brushes.create();
            let mut b = a.sh.brushes.get(&id).clone();
            b.p_size = 1.0; b.min_size = 0.05; b.taper_end = 8.0; b.taper_start = 0.0; b.j_size = 0.0; b.streamline = 0.0; b.hardness = 1.0;
            a.sh.brushes.update(b);
            brush(a, &id, 0.6, [20, 20, 120]);
            *LT_IDS.lock().unwrap() = vec![id];
            a.ed.as_mut().unwrap().layer_add();
        })));
        s.push_back(Step::Pen((0..=90).map(|i| { let t = i as f32 / 90.0; (0.15 + 0.7 * t, 0.3, if t < 0.5 { 0.08 + 1.84 * t } else { 1.0 }) }).collect()));
        s.push_back(Step::Wait(3));
        s.push_back(run(Box::new(|a, l| {
            let ed = a.ed.as_mut().unwrap();
            let avg = |ed: &crate::editor::Editor, x: f32| (0..7).map(|k| thick_at(ed, x + (k as f32 - 3.0) * 0.002, 0.3)).sum::<i32>() / 7;
            let (t0, t1, t2) = (avg(ed, 0.18), avg(ed, 0.6), avg(ed, 0.82));
            l.check(t0 > 0 && t1 > t0 * 2, format!("толщина зависит от нажима пера: слабый {t0} px, сильный {t1} px"));
            l.check(t2 * 10 >= t1 * 8, format!("без автосужения конец штриха не заостряется: середина {t1} px, конец {t2} px"));
            let pen = crate::editor::LAST_PEN.load(std::sync::atomic::Ordering::Relaxed);
            l.check(pen > 900, format!("нажим пера дошёл до программы ({:.2})", pen as f32 / 1000.0));
            if let Some(id) = LT_IDS.lock().unwrap().first().cloned() { a.sh.brushes.remove(&id); }
            brush(a, "pencil_hb", 0.3, [20, 20, 30]);
            a.ui.new_canvas = Some(crate::ui::NewCanvas::default());
        })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("10-new-canvas.png"));
        s.push_back(run(Box::new(|a, _| { a.ui.new_canvas = None; a.ui.settings = true; })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("11-settings.png"));
        s.push_back(run(Box::new(|a, _| { a.ui.settings = false; a.sh.prefs.light = true; })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("12-light.png"));
        s.push_back(run(Box::new(|a, _| { a.sh.prefs.light = false; })));
        // лист с полем вокруг: рисунок за краем листа не экспортируется
        s.push_back(run(Box::new(|a, l| sheet_test(a, l))));
        s.push_back(Step::Wait(8));
        s.push_back(Step::Shot("19-sheet-margin.png"));
        s.push_back(run(Box::new(|a, _| { a.ui.export = Some(crate::app::ExportOpts { kind: "pdf", ..Default::default() }); })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("20-export-dialog.png"));
        s.push_back(run(Box::new(|a, _| { a.ui.export = None; })));
        // перетаскивание слоёв пальцем, пером и мышью (слои A, B, C, D снизу вверх = 0, 1, 2, 3)
        s.push_back(run(Box::new(|a, l| drag_setup(a, l))));
        s.push_back(Step::Wait(8));
        s.push_back(Step::Shot("21-layers-grip.png"));
        s.push_back(Step::Gesture(Box::new(|a| row_drag(a, 0, 3, true, 0.2)), 0.0, true));
        s.push_back(Step::Wait(4));
        s.push_back(run(Box::new(|a, l| order_check(a, l, &[1, 2, 3, 0], "пальцем за ручку ≡: нижний слой перетащен на самый верх"))));
        s.push_back(Step::Gesture(Box::new(|a| row_drag(a, 2, 0, false, 0.2)), 0.0, true));
        s.push_back(Step::Wait(4));
        s.push_back(run(Box::new(|a, l| order_check(a, l, &[1, 2, 3, 0], "быстрый взмах пальцем по строке прокручивает список, а не двигает слой"))));
        s.push_back(Step::Gesture(Box::new(|a| row_drag(a, 1, 0, false, 0.2)), 0.45, true));
        s.push_back(Step::Wait(4));
        s.push_back(run(Box::new(|a, l| order_check(a, l, &[2, 3, 0, 1], "палец: короткое удержание строки, потом перетаскивание"))));
        s.push_back(Step::Gesture(Box::new(|a| row_drag(a, 2, 1, false, 0.2)), 0.0, false));
        s.push_back(Step::Wait(4));
        s.push_back(run(Box::new(|a, l| order_check(a, l, &[3, 0, 1, 2], "мышью за строку: слой перетащен наверх"))));
        s.push_back(Step::Gesture(Box::new(|a| row_drag(a, 2, 3, true, 0.8)), 0.0, true));
        s.push_back(Step::Wait(4));
        s.push_back(run(Box::new(|a, l| order_check(a, l, &[2, 3, 0, 1], "пальцем за ручку ≡: верхний слой перетащен под нижний"))));
        // цвет: удержание кисти больше не включает пипетку; инструмент «Пипетка» берёт цвет и возвращает кисть
        s.push_back(run(Box::new(|a, l| picker_setup(a, l))));
        s.push_back(Step::Wait(4));
        s.push_back(Step::Gesture(Box::new(|a| canvas_pts(a, &[(0.5, 0.5), (0.5, 0.5)])), 0.9, false));
        s.push_back(Step::Wait(4));
        s.push_back(run(Box::new(|a, l| {
            let ed = a.ed.as_mut().unwrap();
            l.check(a.sh.colors.fg == [20, 20, 20] && ed.tool == Tool::Brush, format!("удержание кисти на месте не включает пипетку (цвет {:?}, инструмент {})", a.sh.colors.fg, ed.tool.name()));
            ed.set_tool(Tool::Picker);
        })));
        s.push_back(Step::Wait(4));
        s.push_back(Step::Shot("22-picker-tool.png"));
        s.push_back(Step::Click(0.38, 0.38));
        s.push_back(Step::Wait(4));
        s.push_back(run(Box::new(|a, l| {
            let ed = a.ed.as_ref().unwrap();
            l.check(a.sh.colors.fg == [200, 120, 40] && ed.tool == Tool::Brush, format!("пипетка взяла цвет с холста и вернула кисть (цвет {:?}, инструмент {})", a.sh.colors.fg, ed.tool.name()));
        })));
        // фото: поворот по EXIF, HEIC, вставка новым слоем в лист
        s.push_back(run(Box::new(|a, l| photo_test(a, l))));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("23-photo-layer.png"));
        // ширина телефона: интерфейс как в Procreate не должен наезжать сам на себя
        s.push_back(Step::Size(412.0, 800.0));
        s.push_back(Step::Wait(12));
        s.push_back(run(Box::new(|a, _| { if let Some(ed) = a.ed.as_mut() { ed.fit(); } })));
        s.push_back(Step::Wait(4));
        s.push_back(Step::Shot("13-phone-editor.png"));
        s.push_back(run(Box::new(|a, _| pop(a, crate::ui::Pop::Brushes))));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("14-phone-brushes.png"));
        s.push_back(run(Box::new(|a, _| pop(a, crate::ui::Pop::Layers))));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("15-phone-layers.png"));
        s.push_back(run(Box::new(|a, _| { a.ui.actions_tab = 8; pop(a, crate::ui::Pop::Actions); })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("16-phone-settings.png"));
        s.push_back(run(Box::new(|a, _| pop(a, crate::ui::Pop::Studio))));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("17-phone-brush-studio.png"));
        s.push_back(run(Box::new(|a, _| { a.ui.pop = None; if let Some(ed) = a.ed.as_mut() { ed.set_tool(Tool::Select); } })));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Shot("18-phone-selection.png"));
        s.push_back(run(Box::new(|a, _| { if let Some(ed) = a.ed.as_mut() { ed.set_tool(Tool::Brush); } a.to_gallery(); })));
        s.push_back(Step::Wait(8));
        s.push_back(Step::Shot("24-phone-gallery.png"));
        s.push_back(Step::Size(1400.0, 900.0));
        s.push_back(Step::Wait(6));
        s.push_back(Step::Quit);
        Some(AutoTest { dir, steps: s, stroke: None, pen: None, gesture: None, wait: 0, shots: 0, log: AutoLog::default(), stroke_frames: vec![], last: None, quit: false, done: false, start: Instant::now() })
    }

    /// Вызывается перед кадром: подмешивает события мыши.
    pub fn input(app: &mut App, raw: &mut RawInput) {
        // новый жест: точки считаются сейчас, по текущему положению панелей
        let start = match app.test.as_mut() {
            Some(t) if t.wait == 0 && t.stroke.is_none() && t.pen.is_none() && t.gesture.is_none() && matches!(t.steps.front(), Some(Step::Gesture(..))) => t.steps.pop_front(),
            _ => None,
        };
        if let Some(Step::Gesture(f, hold, touch)) = start {
            let pts = f(app);
            if let Some(t) = app.test.as_mut() {
                if pts.len() >= 2 { t.gesture = Some(Gesture { pts, i: 0, t0: Instant::now(), hold, touch }); } else { t.log.check(false, "жест: не найдено, куда нажимать"); }
            }
        }
        let Some(t) = app.test.as_mut() else { return };
        // настоящая мышь не должна вмешиваться в программные штрихи (если курсор оказался над окном)
        raw.events.retain(|e| !matches!(e, Event::PointerMoved(_) | Event::MouseMoved(_) | Event::PointerButton { .. } | Event::Touch { .. } | Event::PointerGone | Event::MouseWheel { .. }));
        if let Some(g) = &mut t.gesture {
            let mods = egui::Modifiers::NONE;
            let touch = g.touch;
            let tev = |pos: Pos2, phase: egui::TouchPhase| Event::Touch { device_id: egui::TouchDeviceId(2), id: egui::TouchId(9), phase, pos, force: None };
            if g.i == 0 {
                let p = g.pts[0];
                if touch { raw.events.push(tev(p, egui::TouchPhase::Start)); }
                raw.events.push(Event::PointerMoved(p));
                raw.events.push(Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: mods });
                g.i = 1;
                g.t0 = Instant::now();
            } else if g.t0.elapsed().as_secs_f64() < g.hold {
                // палец (или кнопка мыши) лежит на месте
            } else if g.i < g.pts.len() {
                let end = (g.i + 2).min(g.pts.len());
                for p in &g.pts[g.i..end] {
                    if touch { raw.events.push(tev(*p, egui::TouchPhase::Move)); }
                    raw.events.push(Event::PointerMoved(*p));
                }
                g.i = end;
            } else {
                let p = *g.pts.last().unwrap();
                if touch { raw.events.push(tev(p, egui::TouchPhase::End)); }
                raw.events.push(Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: mods });
                // так делает egui-winit, когда палец отрывают: указатель исчезает
                if touch { raw.events.push(Event::PointerGone); }
                t.gesture = None;
            }
            return;
        }
        let rect = app.ed.as_ref().map(|e| e.canvas_rect);
        let to = |p: (f32, f32)| rect.map_or(Pos2::ZERO, |r| Pos2::new(r.left() + r.width() * p.0, r.top() + r.height() * p.1));
        if let Some((pts, i)) = &mut t.pen {
            let ev = |pos: Pos2, f: f32, phase: egui::TouchPhase| Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(7), phase, pos, force: Some(f) };
            if *i == 0 {
                raw.events.push(ev(pts[0].0, pts[0].1, egui::TouchPhase::Start));
                *i = 1;
            } else if *i < pts.len() {
                let end = (*i + 3).min(pts.len());
                for p in &pts[*i..end] { raw.events.push(ev(p.0, p.1, egui::TouchPhase::Move)); }
                *i = end;
            } else {
                let p = *pts.last().unwrap();
                raw.events.push(ev(p.0, p.1, egui::TouchPhase::End));
                t.pen = None;
            }
            return;
        }
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
            Some(Step::Pen(_)) => {
                if let Some(Step::Pen(p)) = t.steps.pop_front() {
                    t.pen = Some((p.into_iter().map(|(x, y, f)| (to((x, y)), f)).collect(), 0));
                }
            }
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
        if t.stroke.is_some() || t.pen.is_some() || t.gesture.is_some() {
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
            Some(Step::Size(w, h)) => { ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h))); t.wait = 4; }
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

#[cfg_attr(target_os = "android", allow(dead_code))]
static CLIP_BACKUP: std::sync::Mutex<Option<(Option<(usize, usize, Vec<u8>)>, Option<String>)>> = std::sync::Mutex::new(None);
#[cfg_attr(target_os = "android", allow(dead_code))]
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

fn blue(p: [u8; 4]) -> bool {
    p[2] > 180 && p[0] < 100 && p[1] < 120
}

fn flat_px(d: &crate::doc::Document, x: usize, y: usize, export: bool) -> [u8; 4] {
    let f = if export { doc::flatten_export(d, true) } else { doc::flatten(d, true) };
    let i = f.idx(x, y);
    [f.px[i], f.px[i + 1], f.px[i + 2], f.px[i + 3]]
}

fn fill_rect(ed: &mut crate::editor::Editor, x0: usize, y0: usize, x1: usize, y1: usize, c: [u8; 3]) {
    let li = ed.doc.active;
    let img = ed.doc.layers[li].img_mut();
    for y in y0..y1 { for x in x0..x1 { let i = img.idx(x, y); img.px[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]); } }
    ed.invalidate_all();
}

static LT_IDS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Папки, выбор нескольких слоёв, маска, перенос, сохранение.
fn layers_test(a: &mut App, l: &mut AutoLog) {
    use crate::editor::Drop;
    let ed = a.ed.as_mut().unwrap();
    ed.set_tool(Tool::Brush);
    ed.layer_add();
    fill_rect(ed, 0, 0, 200, 150, [30, 60, 220]);
    let l1 = ed.doc.layer().id.clone();
    ed.layer_add();
    fill_rect(ed, 1400, 0, 1600, 150, [40, 200, 60]);
    let l2 = ed.doc.layer().id.clone();
    let i1 = ed.doc.find(&l1).unwrap();
    ed.layer_select_toggle(i1);
    l.check(ed.selected().len() == 2, "выбраны два слоя (Ctrl+щелчок)");
    ed.group_selected();
    let gi = ed.doc.active;
    let gid = ed.doc.layers[gi].id.clone();
    let (k1, k2) = (ed.doc.find(&l1).unwrap(), ed.doc.find(&l2).unwrap());
    let inside = ed.doc.layers[gi].group && ed.doc.layers[k1].parent.as_deref() == Some(gid.as_str()) && ed.doc.layers[k2].parent.as_deref() == Some(gid.as_str()) && k1 < gi && k2 < gi;
    l.check(inside, "Ctrl+G: оба слоя в новой папке");
    l.check(blue(flat_px(&ed.doc, 100, 75, false)), "содержимое папки видно на холсте");
    ed.layer_props(gi, |x| x.visible = false, "Видимость");
    let hidden = flat_px(&ed.doc, 100, 75, false);
    l.check(!blue(hidden), format!("скрытая папка прячет свои слои {hidden:?}"));
    ed.layer_props(gi, |x| x.visible = true, "Видимость");
    // маска: стираем часть слоя по выделению
    let k1 = ed.doc.find(&l1).unwrap();
    ed.layer_select(k1);
    ed.mask_add();
    let (w, h) = (ed.doc.w, ed.doc.h);
    let mut m = vec![0u8; w * h];
    for y in 0..150 { for x in 0..100 { m[y * w + x] = 255; } }
    ed.apply_selection(m, crate::editor::SelOp::Replace, None, "Выделение");
    ed.layer_clear();
    ed.select_none();
    let (p_hid, p_vis) = (flat_px(&ed.doc, 50, 75, false), flat_px(&ed.doc, 150, 75, false));
    l.check(ed.doc.layer().mask.is_some() && !blue(p_hid) && blue(p_vis), format!("маска скрывает часть слоя ({p_hid:?} / {p_vis:?})"));
    l.check(ed.doc.layer().img.px[ed.doc.layer().img.idx(50, 75) + 3] == 255, "пиксели слоя под маской не тронуты");
    ed.undo(); ed.undo();
    let back = flat_px(&ed.doc, 50, 75, false);
    ed.redo(); ed.redo();
    let again = flat_px(&ed.doc, 50, 75, false);
    l.check(blue(back) && !blue(again), "отмена и повтор рисования по маске");
    // перетаскивание: второй слой — из папки наверх
    ed.layer_move_to(&l2, Drop::Above(gid.clone()));
    let k2 = ed.doc.find(&l2).unwrap();
    let gi = ed.doc.find(&gid).unwrap();
    l.check(ed.doc.layers[k2].parent.is_none() && k2 == gi + 1, "слой перетащен из папки над ней");
    ed.layer_move_to(&l2, Drop::Into(gid.clone()));
    let k2 = ed.doc.find(&l2).unwrap();
    l.check(ed.doc.layers[k2].parent.as_deref() == Some(gid.as_str()), "слой перетащен обратно в папку");
    // черновой слой не попадает в экспорт
    ed.layer_props(k2, |x| x.draft = true, "Черновой");
    let (seen, exported) = (flat_px(&ed.doc, 1500, 75, false), flat_px(&ed.doc, 1500, 75, true));
    l.check(seen[1] > 150 && exported[1] < 150 || exported[0] > 150, format!("черновой слой виден, но не экспортируется ({seen:?} / {exported:?})"));
    ed.layer_props(k2, |x| x.draft = false, "Черновой");
    // сохранение
    let before = doc::flatten(&ed.doc, true);
    let mz = formats::save_mazok(&ed.doc);
    match formats::load_mazok(&mz) {
        Ok(d) => {
            let ok = d.layers.iter().any(|x| x.group && x.id == gid) && d.layers.iter().any(|x| x.id == l1 && x.mask.is_some() && x.parent.as_deref() == Some(gid.as_str()));
            l.check(ok && hash(&doc::flatten(&d, true)) == hash(&before), ".mazok сохраняет папки и маски пиксель в пиксель");
        }
        Err(e) => l.check(false, format!(".mazok: {e}")),
    }
    match formats::import(&formats::psd_write(&ed.doc), "t.psd") {
        Ok(d) => {
            let groups = d.layers.iter().filter(|x| x.group).count();
            let kids = d.layers.iter().filter(|x| x.parent.is_some()).count();
            let diff = doc::flatten(&d, true).px.iter().zip(&before.px).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap_or(255);
            l.check(groups == 1 && kids == 2 && diff <= 3, format!("PSD: папки — группами Photoshop ({groups} папка, {kids} слоя внутри, расхождение {diff})"));
        }
        Err(e) => l.check(false, format!("PSD: {e}")),
    }
    let k1 = ed.doc.find(&l1).unwrap();
    ed.layer_select(k1);
    *LT_IDS.lock().unwrap() = vec![l1, l2, gid];
}

fn layers_test2(a: &mut App, l: &mut AutoLog) {
    let ids = LT_IDS.lock().unwrap().clone();
    let ed = a.ed.as_mut().unwrap();
    let Some(gi) = ids.get(2).and_then(|g| ed.doc.find(g)) else { l.check(false, "папка из теста пропала"); return };
    // сдвиг клавишами Ctrl+[ / Ctrl+]
    let k1 = ed.doc.find(&ids[0]).unwrap();
    ed.layer_select(k1);
    ed.layer_step(true);
    let up = ed.doc.find(&ids[0]).unwrap();
    l.check(up > k1 && ed.doc.layers[up].parent.as_deref() == Some(ids[2].as_str()), "слой сдвинут выше внутри папки");
    ed.layer_select(ed.doc.find(&ids[2]).unwrap());
    let n = ed.doc.layers.len();
    ed.merge_folder();
    let merged = ed.doc.layer();
    let px = flat_px(&ed.doc, 150, 75, false);
    l.check(!merged.group && ed.doc.layers.len() == n - 2 && blue(px) && !blue(flat_px(&ed.doc, 50, 75, false)), "папка объединена в один слой с учётом маски");
    ed.undo();
    l.check(ed.doc.find(&ids[2]).map_or(false, |g| ed.doc.layers[g].group), "отмена объединения вернула папку");
    let _ = gi;
}
static DRAG_IDS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Холст с четырьмя слоями A, B, C, D (снизу вверх) и открытая панель слоёв.
fn drag_setup(a: &mut App, l: &mut AutoLog) {
    a.create_doc("Перетаскивание слоёв", 800, 600, [255, 255, 255], true);
    let ed = a.ed.as_mut().unwrap();
    ed.doc.layers[0].name = "A".into();
    for (k, (n, c)) in [("B", [220, 40, 40]), ("C", [40, 160, 60]), ("D", [40, 80, 220])].into_iter().enumerate() {
        ed.layer_add();
        let li = ed.doc.active;
        ed.doc.layers[li].name = n.into();
        fill_rect(ed, 100 + k * 180, 100, 260 + k * 180, 400, c);
    }
    let ids: Vec<String> = ed.doc.layers.iter().map(|x| x.id.clone()).collect();
    l.check(ids.len() == 4, "холст с 4 слоями для перетаскивания");
    *DRAG_IDS.lock().unwrap() = ids;
    pop(a, crate::ui::Pop::Layers);
}

/// Путь от строки слоя from к строке to (rel — доля высоты целевой строки); grip — начать с ручки ≡.
fn row_drag(a: &App, from: usize, to: usize, grip: bool, rel: f32) -> Vec<Pos2> {
    let ids = DRAG_IDS.lock().unwrap().clone();
    let find = |k: usize| ids.get(k).and_then(|id| a.ui.layer_rows.iter().find(|r| r.0 == *id)).map(|r| (r.1, r.2));
    let (Some((fr, fg)), Some((tr, _))) = (find(from), find(to)) else { return vec![] };
    let p0 = if grip { fg.center() } else { Pos2::new(fr.left() + fr.width() * 0.5, fr.center().y) };
    let p1 = Pos2::new(p0.x, tr.top() + tr.height() * rel);
    (0..=14).map(|i| p0 + (p1 - p0) * (i as f32 / 14.0)).collect()
}

/// Порядок слоёв A, B, C, D снизу вверх (номера 0…3).
fn order_check(a: &App, l: &mut AutoLog, want: &[usize], what: &str) {
    let ids = DRAG_IDS.lock().unwrap().clone();
    let ed = a.ed.as_ref().unwrap();
    let got: Vec<usize> = ed.doc.layers.iter().filter_map(|x| ids.iter().position(|i| *i == x.id)).collect();
    l.check(got == want && a.ui.layer_drag.is_none(), format!("{what} (снизу вверх {got:?}, нужно {want:?})"));
}

/// Точки на холсте (доли области просмотра) в экранных координатах.
fn canvas_pts(a: &App, pts: &[(f32, f32)]) -> Vec<Pos2> {
    let Some(r) = a.ed.as_ref().map(|e| e.canvas_rect) else { return vec![] };
    pts.iter().map(|p| Pos2::new(r.left() + r.width() * p.0, r.top() + r.height() * p.1)).collect()
}

/// Слой, залитый цветом (200, 120, 40), кисть с чёрным цветом.
fn picker_setup(a: &mut App, l: &mut AutoLog) {
    a.ui.pop = None;
    let ed = a.ed.as_mut().unwrap();
    ed.layer_add();
    let (w, h) = (ed.doc.w, ed.doc.h);
    fill_rect(ed, 0, 0, w, h, [200, 120, 40]);
    ed.fit();
    brush(a, "pencil_hb", 0.1, [20, 20, 20]);
    l.info("пипетка: слой залит цветом (200, 120, 40), кисть чёрная");
}

/// Фото: поворот по EXIF, понятная ошибка для HEIC, вставка новым слоем — вписывается в лист.
fn photo_test(a: &mut App, l: &mut AutoLog) {
    // JPEG 40×20: слева красный, справа синий; в EXIF — «повернуть на 90° по часовой» (так снимают телефоны)
    let mut img = Img::new(40, 20);
    for y in 0..20 { for x in 0..40 { let k = img.idx(x, y); img.px[k..k + 4].copy_from_slice(if x < 20 { &[220, 30, 30, 255] } else { &[30, 30, 220, 255] }); } }
    let jpg = formats::jpeg(&img, [255, 255, 255]);
    let exif: [u8; 36] = [
        0xFF, 0xE1, 0x00, 0x22, b'E', b'x', b'i', b'f', 0, 0,
        b'I', b'I', 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00,
        0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];
    let mut bytes = jpg[..2].to_vec();
    bytes.extend_from_slice(&exif);
    bytes.extend_from_slice(&jpg[2..]);
    match formats::decode_image(&bytes) {
        Ok(d) => {
            let px = |x: usize, y: usize| { let i = d.idx(x, y); [d.px[i], d.px[i + 1], d.px[i + 2]] };
            let (top, bottom) = (px(10, 5), px(10, 34));
            l.check(d.w == 20 && d.h == 40 && top[0] > 150 && top[2] < 100 && bottom[2] > 150 && bottom[0] < 100, format!("фото с пометкой поворота в EXIF открылось повёрнутым ({}×{}, сверху {top:?}, снизу {bottom:?})", d.w, d.h));
        }
        Err(e) => l.check(false, format!("фото с EXIF: {e}")),
    }
    let heic = b"\0\0\0\x18ftypheic\0\0\0\0mif1heic".to_vec();
    l.check(formats::decode_image(&heic).err().map_or(false, |e| e.contains("HEIC")), "фото HEIC: понятное сообщение вместо непонятной ошибки");
    // большое фото — новым слоем, вписано в лист (холст с полем вокруг листа)
    a.create_doc_margin("Фото", 800, 600, [255, 255, 255], true, 0.25);
    let n0 = a.ed.as_ref().unwrap().doc.layers.len();
    let big = Img::filled(2000, 1000, [40, 160, 90, 255]);
    a.handle_open(crate::app::Purpose::InsertImage, vec![("Фото с телефона.png".into(), formats::png(&big))]);
    let ed = a.ed.as_ref().unwrap();
    l.check(ed.doc.layers.len() == n0 + 1 && ed.tool == Tool::Transform && ed.doc.layer().name == "Фото с телефона", "фото добавлено новым слоем, сразу можно двигать и масштабировать");
    a.dispatch("enter");
    let ed = a.ed.as_ref().unwrap();
    let b = ed.doc.layer().img.content_bounds();
    let s = ed.doc.sheet.unwrap_or(crate::util::Rect::new(0, 0, 0, 0));
    let inside = b.map_or(false, |b| b.x >= s.x && b.y >= s.y && b.x + b.w <= s.x + s.w && b.y + b.h <= s.y + s.h && b.w >= s.w - 2);
    l.check(inside, format!("фото вписано в лист, а не в весь холст (лист {s:?}, фото {b:?})"));
}

/// Открыть всплывающую панель интерфейса «как Procreate» (у правого края верхней полосы).
fn pop(a: &mut App, p: crate::ui::Pop) {
    a.ui.pop = Some(p);
    a.ui.pop_anchor = egui::Rect::from_min_size(egui::pos2(10_000.0, 4.0), egui::vec2(40.0, 40.0));
}

/// Толщина линии активного слоя по вертикали в точке (доли области просмотра).
fn thick_at(ed: &crate::editor::Editor, x: f32, y: f32) -> i32 {
    let r = ed.canvas_rect;
    let (dx, dy) = ed.view.to_doc(Pos2::new(r.left() + r.width() * x, r.top() + r.height() * y));
    let img = &ed.doc.layer().img;
    let (cx, cy) = (dx as i32, dy as i32);
    if cx < 0 || cx as usize >= img.w { return 0; }
    let mut n = 0;
    for yy in (cy - 150).max(0)..(cy + 150).min(img.h as i32) {
        if img.px[img.idx(cx as usize, yy as usize) + 3] > 60 { n += 1; }
    }
    n
}

/// Лист с полем: экспорт только листа, все форматы через настоящий экспорт в папку, сохранение, поворот, удаление поля.
fn sheet_test(a: &mut App, l: &mut AutoLog) {
    a.create_doc_margin("Лист с полем", 800, 600, [255, 255, 255], true, 0.25);
    let ed = a.ed.as_mut().unwrap();
    l.check(ed.doc.w == 1200 && ed.doc.h == 900 && ed.doc.sheet == Some(crate::util::Rect::new(200, 150, 800, 600)), format!("новый холст: лист 800×600 и поле вокруг (холст {}×{})", ed.doc.w, ed.doc.h));
    // красный квадрат наполовину за краем листа
    fill_rect(ed, 50, 50, 300, 300, [220, 30, 30]);
    let cut = ed.doc.cropped_to_sheet();
    let flat = doc::flatten_export(&cut, true);
    let p = |img: &Img, x: usize, y: usize| { let i = img.idx(x, y); [img.px[i], img.px[i + 1], img.px[i + 2]] };
    l.check(flat.w == 800 && flat.h == 600 && p(&flat, 0, 0) == [220, 30, 30] && p(&flat, 99, 149) == [220, 30, 30] && p(&flat, 101, 151) == [255, 255, 255],
        format!("экспорт — только лист {}×{}: часть за краем отрезана", flat.w, flat.h));
    // все форматы через настоящий экспорт в папку
    let dir = std::env::var_os("MAZOK_AUTOTEST").map(PathBuf::from).unwrap_or_default().join("export");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let (old_dir, old_q) = (a.sh.prefs.export_dir.clone(), a.sh.prefs.quick_export);
    a.sh.prefs.export_dir = dir.to_string_lossy().to_string();
    a.sh.prefs.quick_export = true;
    for kind in ["png", "jpeg", "webp", "pdf", "psd", "tiff", "bmp", "tga", "gif", "layers", "mazok"] {
        let o = crate::app::ExportOpts { kind: crate::app::EXPORT_KINDS.iter().find(|k| k.0 == kind).unwrap().0, scale: if kind == "webp" { 50.0 } else { 100.0 }, bg: kind != "png", ..Default::default() };
        a.export_with(&o, false);
    }
    a.export_with(&crate::app::ExportOpts { kind: "png", sheet_only: false, ..Default::default() }, false);
    a.sh.prefs.export_dir = old_dir;
    a.sh.prefs.quick_export = old_q;
    let files: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir).map(|rd| rd.flatten().filter_map(|e| Some((e.file_name().to_string_lossy().to_string(), std::fs::read(e.path()).ok()?))).collect()).unwrap_or_default();
    let dims = |ext: &str| files.iter().filter(|f| f.0.to_lowercase().ends_with(ext)).filter_map(|f| formats::decode_image(&f.1).ok().map(|i| (i.w, i.h))).collect::<Vec<_>>();
    l.check(files.len() == 12, format!("экспорт в папку: файлов {} из 12", files.len()));
    // GIF читаем по заголовку (декодера GIF в сборке нет — он и не нужен)
    let gif = files.iter().find(|f| f.0.ends_with(".gif")).map(|f| f.1.clone()).unwrap_or_default();
    let gdim = if gif.len() > 10 && gif.starts_with(b"GIF89a") { Some((u16::from_le_bytes([gif[6], gif[7]]), u16::from_le_bytes([gif[8], gif[9]]))) } else { None };
    l.check(gdim == Some((720, 540)), format!(".gif: размер {gdim:?}"));
    for (ext, want) in [(".jpg", (800, 600)), (".webp", (400, 300)), (".tif", (800, 600)), (".bmp", (800, 600)), (".tga", (800, 600))] {
        l.check(dims(ext) == vec![want], format!("{ext}: размер {:?}", dims(ext)));
    }
    let pngs = dims(".png");
    l.check(pngs.contains(&(800, 600)) && pngs.contains(&(1200, 900)), format!("PNG: только лист и весь холст с полем {pngs:?}"));
    let png_alpha = files.iter().find(|f| f.0.ends_with(".png") && formats::decode_image(&f.1).map_or(false, |i| i.w == 800)).and_then(|f| formats::decode_image(&f.1).ok()).map(|i| i.px[i.idx(500, 400) + 3]);
    l.check(png_alpha == Some(0), "PNG с прозрачным фоном");
    let pdf = files.iter().find(|f| f.0.ends_with(".pdf")).map(|f| f.1.clone()).unwrap_or_default();
    l.check(pdf.starts_with(b"%PDF") && String::from_utf8_lossy(&pdf).contains("/MediaBox [0 0 192.000 144.000]"), "PDF: страница по листу при 300 dpi");
    let psd = files.iter().find(|f| f.0.ends_with(".psd")).and_then(|f| formats::import(&f.1, "x.psd").ok());
    l.check(psd.map_or(false, |d| d.w == 800 && d.h == 600), "PSD — размер листа");
    let mz = files.iter().find(|f| f.0.ends_with(".mazok")).and_then(|f| formats::load_mazok(&f.1).ok());
    let ok = mz.as_ref().map_or(false, |d| d.w == 1200 && d.sheet == Some(crate::util::Rect::new(200, 150, 800, 600)) && { let i = d.layers[0].img.idx(60, 60); d.layers[0].img.px[i] == 220 });
    l.check(ok, ".mazok хранит поле и рисунок за краем листа");
    // поворот холста поворачивает и лист; удаление поля обрезает по листу
    let ed = a.ed.as_mut().unwrap();
    ed.rotate_canvas(true);
    l.check(ed.doc.sheet == Some(crate::util::Rect::new(150, 200, 600, 800)), format!("поворот холста: лист {:?}", ed.doc.sheet));
    ed.undo();
    ed.set_sheet_margin(0.0);
    let ok = ed.doc.w == 800 && ed.doc.h == 600 && ed.doc.sheet.is_none() && ed.doc.layer().img.px[3] == 255;
    ed.undo();
    l.check(ok && ed.doc.w == 1200 && ed.doc.sheet.is_some(), "поле убирается (обрезка по листу) и возвращается отменой");
    ed.fit();
}

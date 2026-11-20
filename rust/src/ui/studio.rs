//! Интерфейс в стиле Procreate: холст на весь экран, тёмная верхняя полоса, боковая панель
//! размера и непрозрачности, всплывающие панели. Все функции «как в Photoshop» — в «Действиях».

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pop { Brushes, Studio, Layers, Color, Actions, Adjust, Tools }

pub const BAR_H: f32 = 50.0;

fn bar_bg(light: bool) -> Color32 { if light { Color32::from_gray(232) } else { Color32::from_gray(20) } }
fn pop_bg(light: bool) -> Color32 { if light { Color32::from_gray(248) } else { Color32::from_gray(34) } }
fn side_bg(light: bool) -> Color32 { if light { Color32::from_rgba_unmultiplied(236, 236, 236, 235) } else { Color32::from_rgba_unmultiplied(26, 26, 28, 225) } }

/// Кнопка-значок верхней полосы.
fn top_icon(ui: &mut Ui, icon: &str, on: bool, tip: &str, s: f32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(s), Sense::click());
    if resp.hovered() { ui.painter().rect_filled(r.shrink(2.0), 8.0, ui.visuals().widgets.hovered.bg_fill.gamma_multiply(0.5)); }
    let c = if on { ACCENT } else { ui.visuals().strong_text_color() };
    paint_icon(ui.painter(), r.shrink(s * 0.19), icon, c);
    resp.on_hover_text(tip)
}

fn toggle(st: &mut UiState, p: Pop, r: Rect) {
    if st.pop == Some(p) { st.pop = None; } else { st.pop = Some(p); st.pop_anchor = r; }
}

fn top_bar(ui: &mut Ui, cx: &mut Cx) {
    let w = ui.available_width();
    let s = ((w - 20.0) / 10.6).clamp(30.0, 42.0);
    let narrow = w < 560.0;
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = if narrow { 0.0 } else { 3.0 };
        if narrow {
            if top_icon(ui, "gallery", false, "Галерея", s).clicked() { cx.acts.push("gallery"); }
        } else if ui.add(egui::Button::new(egui::RichText::new("Галерея").size(15.5).strong()).frame(false)).clicked() {
            cx.acts.push("gallery");
        }
        if !narrow { ui.add_space(8.0); }
        let r = top_icon(ui, "wrench", cx.st.pop == Some(Pop::Actions), "Действия: добавить, холст, правка, слои, экспорт, настройки", s);
        if r.clicked() { toggle(cx.st, Pop::Actions, r.rect); }
        let r = top_icon(ui, "wand", cx.st.pop == Some(Pop::Adjust) || cx.ed.filter.is_some(), "Коррекция и фильтры", s);
        if r.clicked() { toggle(cx.st, Pop::Adjust, r.rect); }
        let r = top_icon(ui, "lasso", cx.ed.tool == Tool::Select, "Выделение (M, L, W)", s);
        if r.clicked() { cx.st.pop = None; if cx.ed.tool == Tool::Select { cx.ed.set_tool(Tool::Brush); } else { cx.ed.set_tool(Tool::Select); } }
        let r = top_icon(ui, "arrow", cx.ed.tool == Tool::Transform, "Трансформация (V); повторное нажатие — применить", s);
        if r.clicked() { cx.st.pop = None; if cx.ed.xform.is_some() { cx.acts.push("enter"); } else { cx.ed.set_tool(Tool::Transform); } }
        let other = matches!(cx.ed.tool, Tool::Fill | Tool::Gradient | Tool::Text | Tool::Shapes | Tool::Clone | Tool::Hand | Tool::Zoom);
        let r = top_icon(ui, "tools", other || cx.st.pop == Some(Pop::Tools), "Другие инструменты: заливка, градиент, текст, фигуры, штамп, рука, масштаб", s);
        if r.clicked() { toggle(cx.st, Pop::Tools, r.rect); }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(s), Sense::click());
            let fg = cx.sh.colors.fg;
            ui.painter().circle_filled(r.center(), s * 0.36, Color32::from_rgb(fg[0], fg[1], fg[2]));
            ui.painter().circle_stroke(r.center(), s * 0.38, EStroke::new(2.0, if cx.st.pop == Some(Pop::Color) { ACCENT } else { ui.visuals().strong_text_color() }));
            if resp.on_hover_text("Цвет").clicked() { toggle(cx.st, Pop::Color, r); }
            let r = top_icon(ui, "layers", cx.st.pop == Some(Pop::Layers), "Слои", s);
            if r.clicked() { toggle(cx.st, Pop::Layers, r.rect); }
            for (t, icon, tip) in [(Tool::Eraser, "eraser", "Ластик (E); ещё раз — библиотека кистей"), (Tool::Smudge, "smudge", "Палец (R); ещё раз — библиотека кистей"), (Tool::Brush, "brush", "Кисть (B); ещё раз — библиотека кистей")] {
                let r = top_icon(ui, icon, cx.ed.tool == t, tip, s);
                if r.clicked() {
                    if cx.ed.tool == t { toggle(cx.st, Pop::Brushes, r.rect); } else { cx.ed.set_tool(t); if cx.st.pop == Some(Pop::Brushes) { cx.st.pop_anchor = r.rect; } }
                }
            }
        });
    });
}

/// Вертикальный ползунок боковой панели (тянуть пальцем или пером).
fn vslider(ui: &mut Ui, v: &mut f32, lo: f32, hi: f32, size: Vec2, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let pad = 8.0;
    if resp.dragged() || resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            let t = 1.0 - (p.y - rect.top() - pad) / (rect.height() - pad * 2.0);
            *v = lo + t.clamp(0.0, 1.0) * (hi - lo);
        }
    }
    let t = ((*v - lo) / (hi - lo)).clamp(0.0, 1.0);
    let p = ui.painter();
    let light = !ui.visuals().dark_mode;
    let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width() * 0.42, rect.height()));
    p.rect_filled(track, track.width() / 2.0, if light { Color32::from_gray(200) } else { Color32::from_gray(58) });
    let y = rect.bottom() - pad - t * (rect.height() - pad * 2.0);
    let fill = Rect::from_min_max(Pos2::new(track.left(), y), track.right_bottom());
    p.rect_filled(fill, track.width() / 2.0, if light { Color32::from_gray(150) } else { Color32::from_gray(110) });
    let thumb = Rect::from_center_size(Pos2::new(rect.center().x, y), Vec2::new(rect.width() - 2.0, 14.0));
    p.rect_filled(thumb, 5.0, if resp.dragged() { ACCENT } else { Color32::from_gray(238) });
    p.rect_stroke(thumb, 5.0, EStroke::new(1.0, Color32::from_black_alpha(90)), egui::StrokeKind::Outside);
    resp.on_hover_text(tip)
}

fn side_bar(ctx: &egui::Context, cx: &mut Cx, top: f32) {
    let screen = ctx.screen_rect();
    let light = cx.sh.prefs.light;
    let w = 50.0;
    let safe = cx.st.safe;
    let bottom = screen.bottom() - safe[1];
    let avail = (bottom - top - 16.0).max(220.0);
    let fixed = 34.0 + 8.0 + 2.0 * 38.0 + 30.0;
    let size_h = ((avail - fixed) * 0.56).clamp(70.0, 250.0);
    let op_h = ((avail - fixed) * 0.44).clamp(60.0, 200.0);
    let h = size_h + op_h + fixed;
    let x = if cx.sh.prefs.sidebar_right { screen.right() - safe[3] - w - 8.0 } else { screen.left() + safe[2] + 8.0 };
    let y = top + ((bottom - top - h) / 2.0).max(8.0);
    let mut bubble: Option<(f32, String, Pos2)> = None;
    egui::Area::new(egui::Id::new("studio-side")).order(egui::Order::Middle).fixed_pos(Pos2::new(x, y)).show(ctx, |ui| {
        egui::Frame::new().fill(side_bg(light)).corner_radius(14.0).inner_margin(egui::Margin::symmetric(4, 8)).show(ui, |ui| {
            ui.set_width(w - 8.0);
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let view_s = cx.ed.view.s;
                let ts = cx.ed.tool_state_mut(cx.sh);
                let mut size = ts.size;
                let mut op = ts.opacity;
                let r = vslider(ui, &mut size, 0.0, 1.0, Vec2::new(w - 12.0, size_h), "Размер кисти");
                let r_size = r.rect;
                let size_active = r.dragged() || r.is_pointer_button_down_on();
                let (sq, sresp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
                ui.painter().rect_stroke(sq.shrink(3.0), 6.0, EStroke::new(1.5, ui.visuals().strong_text_color()), egui::StrokeKind::Inside);
                paint_icon(ui.painter(), sq.shrink(7.0), "picker", ui.visuals().strong_text_color());
                if sresp.on_hover_text("Пипетка (I, Alt или удержание на холсте)").clicked() { cx.acts.push("t_picker"); }
                let r = vslider(ui, &mut op, 0.01, 1.0, Vec2::new(w - 12.0, op_h), "Непрозрачность");
                let op_active = r.dragged() || r.is_pointer_button_down_on();
                let r_op = r.rect;
                ui.add_space(4.0);
                if icon_btn(ui, "undo", false, "Отменить (Ctrl+Z, касание двумя пальцами)", Vec2::splat(36.0)).clicked() { cx.acts.push("undo"); }
                if icon_btn(ui, "redo", false, "Повторить (Ctrl+Shift+Z, касание тремя пальцами)", Vec2::splat(36.0)).clicked() { cx.acts.push("redo"); }
                let ts = cx.ed.tool_state_mut(cx.sh);
                ts.size = size;
                ts.opacity = op;
                let id = ts.brush_id.clone();
                let b = cx.sh.brushes.get(&id);
                let px = brush::size_px(b, size);
                let right = cx.sh.prefs.sidebar_right;
                let anchor = |r: Rect| if right { Pos2::new(r.left() - 160.0, r.center().y - 70.0) } else { Pos2::new(r.right() + 22.0, r.center().y - 70.0) };
                if size_active { bubble = Some(((px * view_s).clamp(2.0, 120.0), format!("{:.1} px", px), anchor(r_size))); }
                if op_active { bubble = Some((0.0, format!("{:.0} %", op * 100.0), anchor(r_op))); }
            });
        });
    });
    // подсказка с размером кисти, как в Procreate
    if let Some((d, text, pos)) = bubble {
        egui::Area::new(egui::Id::new("studio-bubble")).order(egui::Order::Tooltip).fixed_pos(pos).interactable(false).show(ctx, |ui| {
            egui::Frame::new().fill(side_bg(light)).corner_radius(12.0).inner_margin(10.0).show(ui, |ui| {
                ui.set_width(124.0);
                ui.vertical_centered(|ui| {
                    if d > 0.0 {
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(124.0), Sense::hover());
                        let fg = cx.sh.colors.fg;
                        ui.painter().circle_filled(r.center(), d / 2.0, Color32::from_rgb(fg[0], fg[1], fg[2]));
                        ui.painter().circle_stroke(r.center(), d / 2.0 + 0.5, EStroke::new(1.0, Color32::from_gray(160)));
                    }
                    ui.label(egui::RichText::new(text).size(17.0).strong());
                });
            });
        });
    }
}

/// Быстрые настройки кисти над библиотекой: режим, нажим, стабилизатор.
fn brush_quick(ui: &mut Ui, cx: &mut Cx) {
    let tool = cx.ed.tool;
    let ts = cx.ed.tool_state_mut(cx.sh).clone();
    let mut b = cx.sh.brushes.get(&ts.brush_id).clone();
    let orig = b.clone();
    ui.horizontal_wrapped(|ui| {
        if tool == Tool::Brush {
            let mut mode = Blend::from_id(&ts.mode);
            egui::ComboBox::from_id_salt("sbmode").selected_text(mode.name()).width(130.0).show_ui(ui, |ui| {
                for (i, (bl, _, n, _)) in BLENDS.iter().enumerate() { if GROUP_STARTS.contains(&i) { ui.separator(); } ui.selectable_value(&mut mode, *bl, *n); }
            });
            cx.ed.tool_state_mut(cx.sh).mode = mode.id().to_string();
        }
        let mut ps = b.p_size > 0.0;
        if ui.toggle_value(&mut ps, "Нажим → размер").on_hover_text("Толщина штриха зависит от нажима пера").changed() { b.p_size = if ps { 0.75 } else { 0.0 }; }
        let mut pf = b.p_flow > 0.0;
        if ui.toggle_value(&mut pf, "Нажим → плотность").changed() { b.p_flow = if pf { 0.6 } else { 0.0 }; }
    });
    if b.p_size > 0.0 { wide_slider(ui, "Сила нажима на размер", &mut b.p_size, 0.05..=1.0, true); }
    wide_slider(ui, "Стабилизатор", &mut cx.sh.prefs.stabilizer, 0.0..=1.0, true);
    if tool == Tool::Eraser && cx.ed.doc.layer().is_vector() {
        ui.horizontal_wrapped(|ui| { ui.label("Векторный ластик:"); vector_erase_modes(ui, &mut cx.sh.prefs.vector_erase); });
    }
    if b != orig { cx.sh.brushes.update(b); }
}

fn tab_row(ui: &mut Ui, cur: &mut usize, names: &[&str]) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
        for (i, n) in names.iter().enumerate() {
            if ui.selectable_label(*cur == i, egui::RichText::new(*n).size(14.5)).clicked() { *cur = i; }
        }
    });
    ui.separator();
}

const ACTION_TABS: [&str; 10] = ["Добавить", "Холст", "Правка", "Слои", "Выделение", "Экспорт", "Вид", "История", "Настройки", "Справка"];

fn big_button(ui: &mut Ui, label: &str) -> egui::Response {
    let w = ui.available_width();
    ui.add(egui::Button::new(egui::RichText::new(label).size(15.0)).min_size(Vec2::new(w, 34.0)))
}

fn actions_pop(ui: &mut Ui, cx: &mut Cx, h: f32) {
    let mut tab = cx.st.actions_tab;
    tab_row(ui, &mut tab, &ACTION_TABS);
    cx.st.actions_tab = tab;
    egui::ScrollArea::vertical().id_salt("act").max_height(h).auto_shrink([false, true]).show(ui, |ui| {
        match tab {
            0 => {
                for (k, n) in [("insert_image", "Вставить файл изображения…"), ("paste", "Вставить из буфера обмена  (Ctrl+V)"), ("copy", "Копировать  (Ctrl+C)"), ("copy_merged", "Копировать совмещённые  (Ctrl+Shift+C)"), ("cut", "Вырезать  (Ctrl+X)"), ("t_text", "Добавить текст"), ("layer_new", "Новый слой"), ("layer_new_vector", "Новый векторный слой"), ("layer_folder", "Новая папка"), ("open", "Открыть или импортировать файл…")] {
                    if big_button(ui, n).clicked() { cx.acts.push(k); cx.st.pop = None; }
                }
            }
            1 => { image_menu(ui, cx); ui.separator(); view_menu(ui, cx); }
            2 => edit_menu(ui, cx),
            3 => layer_menu(ui, cx),
            4 => select_menu(ui, cx),
            5 => {
                for (k, n) in [("export_dialog", "Экспорт листа… (PNG, JPEG, WebP, PDF, PSD, TIFF…)"), ("save", "Сохранить  (Ctrl+S)"), ("save_as", "Сохранить как… (выбрать папку)"), ("quick_png", "Быстрый экспорт PNG"), ("export_png", "PNG"), ("export_jpeg", "JPEG"), ("export_psd", "PSD со слоями и папками"), ("export_mazok", "Проект Мазок (.mazok)"), ("export_layers", "Слои в PNG (ZIP)"), ("export_gif", "GIF (слои — кадры)"), ("pick_export_dir", "Папка для сохранения…"), ("import_brushes", "Импорт кистей (.abr, .sut)…"), ("import_palette", "Импорт палитры…")] {
                    if big_button(ui, n).clicked() { cx.acts.push(k); cx.st.pop = None; }
                }
            }
            6 => {
                for (k, n) in [("fit", "Показать весь холст  (Ctrl+0)"), ("actual", "100 %  (Ctrl+1)"), ("zoom_in", "Увеличить"), ("zoom_out", "Уменьшить"), ("flip_view", "Зеркальный вид  (F)"), ("reset_rotation", "Сбросить поворот"), ("toggle_ui", "Скрыть панели  (Tab, касание четырьмя пальцами)")] {
                    if big_button(ui, n).clicked() { cx.acts.push(k); }
                }
            }
            7 => history_panel(ui, cx),
            8 => {
                let mut pick = false;
                settings_contents(ui, &mut cx.sh.prefs, &mut pick);
                if pick { cx.acts.push("pick_export_dir"); }
            }
            _ => {
                egui::Grid::new("shelp").striped(true).num_columns(2).show(ui, |ui| {
                    for (a, b) in HELP { ui.label(egui::RichText::new(*a).weak()); ui.label(*b); ui.end_row(); }
                });
            }
        }
    });
}

fn adjust_pop(ui: &mut Ui, cx: &mut Cx, h: f32) {
    egui::ScrollArea::vertical().id_salt("adj").max_height(h).auto_shrink([false, true]).show(ui, |ui| {
        for g in ["Цвет", "Размытие", "Эффекты"] {
            ui.label(egui::RichText::new(g).strong());
            for f in FILTERS.iter().filter(|f| f.group == g) {
                let w = ui.available_width();
                if ui.add(egui::Button::new(egui::RichText::new(f.name).size(15.0)).frame(false).min_size(Vec2::new(w, 30.0))).clicked() {
                    cx.ed.filter_open(f.id);
                    cx.st.pop = None;
                }
            }
            ui.add_space(4.0);
        }
    });
}

fn tools_pop(ui: &mut Ui, cx: &mut Cx) {
    let items: [(Tool, Option<SelShape>, &str, &str); 12] = [
        (Tool::Fill, None, "fill", "Заливка (G)"), (Tool::Gradient, None, "gradient", "Градиент"), (Tool::Text, None, "text", "Текст (T)"),
        (Tool::Shapes, None, "shapes", "Фигуры (U)"), (Tool::Clone, None, "clone", "Штамп (S)"), (Tool::Picker, None, "picker", "Пипетка (I)"),
        (Tool::Hand, None, "hand", "Рука (H)"), (Tool::Zoom, None, "zoom", "Масштаб (Z)"), (Tool::Select, Some(SelShape::Rect), "rect", "Прямоугольник"),
        (Tool::Select, Some(SelShape::Ellipse), "ellipse", "Эллипс"), (Tool::Select, Some(SelShape::Lasso), "lasso", "Лассо"), (Tool::Select, Some(SelShape::Wand), "wand", "Волшебная палочка"),
    ];
    let cw = ((ui.available_width() - 8.0) / 2.0).max(120.0);
    egui::Grid::new("stools").num_columns(2).spacing([6.0, 6.0]).show(ui, |ui| {
        for (k, (t, s, icon, name)) in items.iter().enumerate() {
            let sel = cx.ed.tool == *t && s.map_or(true, |s| cx.ed.sel_shape == s);
            let (r, resp) = ui.allocate_exact_size(Vec2::new(cw, 40.0), Sense::click());
            if sel { ui.painter().rect_filled(r, 8.0, ACCENT.gamma_multiply(0.35)); } else if resp.hovered() { ui.painter().rect_filled(r, 8.0, ui.visuals().widgets.hovered.bg_fill); }
            paint_icon(ui.painter(), Rect::from_min_size(r.min + Vec2::new(6.0, 6.0), Vec2::splat(28.0)), icon, if sel { ACCENT } else { ui.visuals().strong_text_color() });
            ui.painter().text(r.min + Vec2::new(42.0, 20.0), egui::Align2::LEFT_CENTER, *name, egui::FontId::proportional(14.5), ui.visuals().text_color());
            if resp.clicked() {
                if let Some(s) = s { cx.ed.sel_shape = *s; }
                cx.ed.set_tool(*t);
                cx.st.pop = None;
            }
            if k % 2 == 1 { ui.end_row(); }
        }
    });
}

fn popover(ctx: &egui::Context, cx: &mut Cx) {
    let Some(pop) = cx.st.pop else { return };
    let screen = ctx.screen_rect();
    let light = cx.sh.prefs.light;
    let want: f32 = match pop { Pop::Layers => 370.0, Pop::Brushes | Pop::Studio => 400.0, Pop::Color => 340.0, Pop::Actions => 430.0, Pop::Adjust => 280.0, Pop::Tools => 340.0 };
    let w = want.min(screen.width() - 16.0);
    let a = cx.st.pop_anchor;
    let x = if screen.width() < 560.0 { (screen.width() - w) / 2.0 } else if a.center().x > screen.center().x { a.right() - w } else { a.left() };
    let x = x.clamp(8.0, (screen.right() - w - 8.0).max(8.0));
    let y = BAR_H + cx.st.safe[0] + 4.0;
    let h = (screen.bottom() - cx.st.safe[1] - y - 24.0).min(680.0).max(200.0);
    egui::Area::new(egui::Id::new("studio-pop")).order(egui::Order::Foreground).fixed_pos(Pos2::new(x, y)).show(ctx, |ui| {
        egui::Frame::new().fill(pop_bg(light)).corner_radius(14.0).inner_margin(12.0)
            .shadow(egui::epaint::Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha(110) })
            .show(ui, |ui| {
                ui.set_width(w - 24.0);
                let title = |ui: &mut Ui, t: &str| { ui.label(egui::RichText::new(t).size(17.0).strong()); };
                match pop {
                    Pop::Brushes => {
                        ui.horizontal(|ui| {
                            title(ui, match cx.ed.tool { Tool::Eraser => "Ластик", Tool::Smudge => "Палец", _ => "Библиотека кистей" });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { if ui.button("Настройки кисти").clicked() { cx.st.pop = Some(Pop::Studio); } });
                        });
                        brush_quick(ui, cx);
                        ui.separator();
                        egui::ScrollArea::vertical().id_salt("sbr").max_height((h - 190.0).max(150.0)).min_scrolled_height((h - 190.0).max(150.0)).auto_shrink([false, false]).show(ui, |ui| brushes_panel(ui, cx));
                    }
                    Pop::Studio => {
                        ui.horizontal(|ui| { if ui.button("< Кисти").clicked() { cx.st.pop = Some(Pop::Brushes); } title(ui, "Настройки кисти"); });
                        egui::ScrollArea::vertical().id_salt("sst").max_height(h - 50.0).auto_shrink([false, true]).show(ui, |ui| brushset_panel(ui, cx));
                    }
                    Pop::Layers => {
                        title(ui, "Слои");
                        let lh = (h - 40.0).min(620.0);
                        ui.allocate_ui(Vec2::new(w - 24.0, lh), |ui| { ui.set_min_height(lh); ui.set_max_height(lh); layers_panel(ui, cx); });
                    }
                    Pop::Color => {
                        let mut t = cx.st.color_tab;
                        tab_row(ui, &mut t, &["Цвет", "Палитры"]);
                        cx.st.color_tab = t;
                        egui::ScrollArea::vertical().id_salt("scol").max_height(h - 60.0).auto_shrink([false, true]).show(ui, |ui| if t == 0 { color_panel(ui, cx) } else { swatches_panel(ui, cx) });
                    }
                    Pop::Actions => actions_pop(ui, cx, h - 90.0),
                    Pop::Adjust => { title(ui, "Коррекция"); adjust_pop(ui, cx, h - 50.0); }
                    Pop::Tools => { title(ui, "Инструменты"); tools_pop(ui, cx); }
                }
            });
    });
}

/// Нужна ли плавающая панель параметров (выделение, трансформация, заливка и т. п.).
fn needs_bottom(cx: &Cx) -> bool {
    !matches!(cx.ed.tool, Tool::Brush | Tool::Eraser | Tool::Smudge | Tool::Picker) || cx.ed.xform.is_some() || (cx.ed.tool == Tool::Eraser && cx.ed.doc.layer().is_vector())
}

/// Панель параметров инструмента — сверху, под основной полосой (внизу её закрывают системные кнопки Android).
fn tool_bar(ctx: &egui::Context, cx: &mut Cx, top: f32) -> f32 {
    if !needs_bottom(cx) { return 0.0; }
    let screen = ctx.screen_rect();
    let light = cx.sh.prefs.light;
    let maxw = screen.width() - 24.0;
    let r = egui::Area::new(egui::Id::new("studio-bottom")).order(egui::Order::Middle).anchor(egui::Align2::CENTER_TOP, [0.0, top + 6.0]).show(ctx, |ui| {
        egui::Frame::new().fill(side_bg(light)).corner_radius(14.0).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
            ui.set_max_width(maxw - 20.0);
            egui::ScrollArea::horizontal().id_salt("sbot").show(ui, |ui| {
                if cx.ed.tool == Tool::Eraser && cx.ed.doc.layer().is_vector() && cx.ed.xform.is_none() {
                    ui.horizontal(|ui| { ui.label("Векторный ластик:"); vector_erase_modes(ui, &mut cx.sh.prefs.vector_erase); });
                } else {
                    options_bar(ui, cx);
                }
            });
        });
    });
    r.response.rect.height() + 6.0
}

impl App {
    /// Редактор в стиле Procreate.
    pub fn studio_ui(&mut self, ctx: &egui::Context) {
        let light = self.sh.prefs.light;
        let stage = if light { Color32::from_gray(196) } else { Color32::from_gray(40) };
        let mut acts: Vec<&'static str> = vec![];
        let App { ed, sh, ui: st, tex, .. } = self;
        let Some(ed) = ed.as_mut() else { return };
        let hidden = ed.ui_hidden;
        {
            let mut cx = Cx { ed, sh, st, tex, acts: &mut acts };
            // касание холста закрывает всплывающую панель и не рисует
            if cx.st.pop.is_some() {
                let press = ctx.input(|i| if i.pointer.any_pressed() { i.pointer.press_origin() } else { None });
                if let Some(p) = press {
                    let over_area = ctx.layer_id_at(p).map_or(false, |l| l.order != egui::Order::Background);
                    if !over_area && p.y > BAR_H + cx.st.safe[0] { cx.st.pop = None; cx.ed.block_input = true; }
                }
            }
            if !hidden {
                let safe = cx.st.safe;
                let m = egui::Margin { left: (8.0 + safe[2]).min(127.0) as i8, right: (8.0 + safe[3]).min(127.0) as i8, top: (4.0 + safe[0]).min(127.0) as i8, bottom: 4 };
                egui::TopBottomPanel::top("studio-top").exact_height(BAR_H + safe[0]).frame(egui::Frame::new().fill(bar_bg(light)).inner_margin(m))
                    .show(ctx, |ui| top_bar(ui, &mut cx));
            }
            egui::CentralPanel::default().frame(egui::Frame::default().fill(stage)).show(ctx, |ui| cx.ed.canvas_ui(ui, cx.sh, stage));
            if !hidden {
                let bar = BAR_H + cx.st.safe[0];
                let th = tool_bar(ctx, &mut cx, bar);
                side_bar(ctx, &mut cx, bar + th);
                popover(ctx, &mut cx);
            } else {
                egui::Area::new(egui::Id::new("studio-show")).anchor(egui::Align2::RIGHT_TOP, [-10.0, 10.0]).show(ctx, |ui| {
                    if ui.button("Показать панели").clicked() { cx.ed.ui_hidden = false; }
                });
            }
            if let Some(editor::Request::Text { spec, layer }) = cx.ed.request.take() {
                cx.st.text = Some(TextDlg { spec, layer, font: 0 });
            }
        }
        for a in acts { self.dispatch(a); }
    }
}

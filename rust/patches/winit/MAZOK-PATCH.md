# winit 0.30.13 с наведением стилуса на Android

Копия крейта [winit](https://github.com/rust-windowing/winit) 0.30.13 (лицензия Apache-2.0).

Единственное изменение — `src/platform_impl/android/mod.rs`, обработка `InputEvent::MotionEvent`
(место помечено `MAZOK-PATCH`). Оригинальный winit на Android пропускает события наведения
(`HOVER_ENTER`, `HOVER_MOVE`, `HOVER_EXIT`: «TODO mouse events»), поэтому стилус над экраном,
не касаясь его, программе не виден, и контур кисти до касания показать нельзя.

С исправлением:
- `HoverEnter` / `HoverMove` → `WindowEvent::CursorMoved` (egui получает `PointerMoved`);
- `HoverExit` → `WindowEvent::CursorLeft` (egui получает `PointerGone`).

Касания по-прежнему приходят как `WindowEvent::Touch`. На других платформах крейт не изменён.

Подключается в `rust/Cargo.toml` через `[patch.crates-io]`.

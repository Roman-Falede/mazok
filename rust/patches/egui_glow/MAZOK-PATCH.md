# egui_glow 0.31.1 с исправлением для Android

Копия крейта [egui_glow](https://github.com/emilk/egui) 0.31.1 (лицензия MIT OR Apache-2.0).

Единственное изменение — точность в шейдерах `src/shader/vertex.glsl` и `src/shader/fragment.glsl`:
`precision highp float` вместо `precision mediump float`. На мобильных видеокартах mediump —
это 16-битные числа: координаты вершин (в точках экрана) и текстурные координаты широкого атласа
шрифта округляются на несколько пикселей, и буквы обрезаются или сдвигаются. На компьютере
точность и так полная, поэтому там ошибки не видно.

Подключается в `rust/Cargo.toml` через `[patch.crates-io]`.

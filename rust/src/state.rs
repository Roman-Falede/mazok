//! Общее состояние приложения: настройки, цвета, параметры инструментов, палитры.

use crate::brush::BrushLib;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub pressure_gamma: f32,
    pub streamline_mul: f32,
    pub fill_tol: f32,
    pub fill_expand: usize,
    pub fill_all_layers: bool,
    pub pick_layer: bool,
    pub quick_shape: bool,
    pub quick_shape_delay: f32,
    pub hold_pick: bool,
    pub hold_delay: f32,
    pub touch_draw: bool,
    pub ui_mode: String,
    pub light: bool,
    pub cursor: bool,
    pub ui_scale: f32,
    pub clone_aligned: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            pressure_gamma: 1.0, streamline_mul: 1.0, fill_tol: 22.0, fill_expand: 1, fill_all_layers: false, pick_layer: false,
            quick_shape: true, quick_shape_delay: 0.65, hold_pick: true, hold_delay: 0.45, touch_draw: true, ui_mode: "auto".into(),
            light: false, cursor: true, ui_scale: 1.0, clone_aligned: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolState {
    pub brush_id: String,
    pub size: f32,
    pub opacity: f32,
    pub mode: String,
}

impl Default for ToolState {
    fn default() -> Self {
        ToolState { brush_id: "pencil_hb".into(), size: 0.25, opacity: 1.0, mode: "source-over".into() }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolStates {
    pub brush: ToolState,
    pub eraser: ToolState,
    pub smudge: ToolState,
    pub clone: ToolState,
}

impl Default for ToolStates {
    fn default() -> Self {
        ToolStates {
            brush: ToolState::default(),
            eraser: ToolState { brush_id: "air_soft".into(), size: 0.3, ..Default::default() },
            smudge: ToolState { brush_id: "oil".into(), size: 0.3, opacity: 0.7, ..Default::default() },
            clone: ToolState { brush_id: "air_mid".into(), size: 0.3, ..Default::default() },
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Colors {
    pub fg: [u8; 3],
    pub bg: [u8; 3],
    pub recent: Vec<[u8; 3]>,
}

impl Default for Colors {
    fn default() -> Self {
        Colors { fg: [20, 20, 26], bg: [255, 255, 255], recent: vec![] }
    }
}

impl Colors {
    pub fn set(&mut self, c: [u8; 3]) {
        if c != self.fg {
            self.fg = c;
        }
    }
    pub fn used(&mut self) {
        let c = self.fg;
        self.recent.retain(|x| *x != c);
        self.recent.insert(0, c);
        self.recent.truncate(16);
    }
    pub fn swap(&mut self) {
        std::mem::swap(&mut self.fg, &mut self.bg);
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Palette {
    pub name: String,
    pub colors: Vec<[u8; 3]>,
}

pub fn default_palettes() -> Vec<Palette> {
    let p = |name: &str, hexes: &[&str]| Palette { name: name.into(), colors: hexes.iter().filter_map(|h| crate::util::parse_hex(h)).collect() };
    vec![
        p("Базовая", &["#000000", "#3d3d3d", "#7a7a7a", "#bdbdbd", "#ffffff", "#e53935", "#fb8c00", "#fdd835", "#43a047", "#1e88e5", "#3949ab", "#8e24aa", "#6d4c41", "#f48fb1", "#80deea"]),
        p("Кожа", &["#ffe0c7", "#f9c9a4", "#eab08c", "#d89a73", "#c18060", "#a26849", "#83503a", "#653c2c", "#4a2b20", "#2e1a14", "#f2b8a0", "#e3967f"]),
        p("Пастель", &["#ffd1dc", "#ffe4c4", "#fff5ba", "#d4f0c0", "#c1e7e3", "#bde0fe", "#cdb4db", "#ffc8dd", "#a2d2ff", "#e2ece9", "#fde2e4", "#dfe7fd"]),
        p("Природа", &["#1b2f1c", "#2f4f2f", "#4f772d", "#90a955", "#ecf39e", "#7f5539", "#b08968", "#ddb892", "#3a5a40", "#588157", "#a3b18a", "#dad7cd", "#386fa4", "#59a5d8", "#84d2f6"]),
        p("Ночной город", &["#0b0c1e", "#1b1f3b", "#2d3561", "#7c3aed", "#c026d3", "#f43f5e", "#fb923c", "#facc15", "#22d3ee", "#a3e635", "#f5f5f5", "#64748b"]),
    ]
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct SavedState {
    prefs: Prefs,
    tools: ToolStates,
    colors: Colors,
    palettes: Vec<Palette>,
}

pub struct Shared {
    pub brushes: BrushLib,
    pub prefs: Prefs,
    pub colors: Colors,
    pub tools: ToolStates,
    pub palettes: Vec<Palette>,
    pub data_dir: PathBuf,
}

impl Shared {
    pub fn load(data_dir: PathBuf) -> Shared {
        let _ = std::fs::create_dir_all(&data_dir);
        let brushes = BrushLib::load(Some(data_dir.join("brushes.json")));
        let saved: SavedState = std::fs::read_to_string(data_dir.join("settings.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let palettes = if saved.palettes.is_empty() { default_palettes() } else { saved.palettes };
        Shared { brushes, prefs: saved.prefs, colors: saved.colors, tools: saved.tools, palettes, data_dir }
    }
    pub fn save(&self) {
        let s = SavedState { prefs: self.prefs.clone(), tools: self.tools.clone(), colors: self.colors.clone(), palettes: self.palettes.clone() };
        if let Ok(j) = serde_json::to_string_pretty(&s) {
            let _ = std::fs::write(self.data_dir.join("settings.json"), j);
        }
        self.brushes.save();
    }
}

//! The screen Neovim draws through the `ext_linegrid` UI protocol.
//!
//! Without `ext_multigrid`, Neovim composites floating windows, popups and
//! the command line into grid 1 itself, so this model is one grid of cells
//! plus highlight definitions, the cursor and the current mode.

use std::collections::HashMap;

use rmpv::Value;

/// Colours and attributes of a highlight id (`hl_attr_define`, rgb form).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HlAttr {
    pub fg: Option<u32>,
    pub bg: Option<u32>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub reverse: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub text: String,
    pub hl: u64,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            text: " ".into(),
            hl: 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Grid {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<Cell>,
    pub cursor: (usize, usize),
    pub hl: HashMap<u64, HlAttr>,
    pub default_fg: Option<u32>,
    pub default_bg: Option<u32>,
    /// Name of the current mode (`normal`, `insert`, `cmdline_normal`, …).
    pub mode: String,
    mode_names: Vec<String>,
    /// Incremented on every `flush`, so the UI knows a frame is complete.
    pub frames: u64,
}

fn u(v: &Value) -> usize {
    v.as_u64().unwrap_or(0) as usize
}

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap_or(0)
}

impl Grid {
    pub fn resize(&mut self, width: usize, height: usize) {
        let mut cells = vec![Cell::default(); width * height];
        for r in 0..height.min(self.height) {
            for c in 0..width.min(self.width) {
                cells[r * width + c] = self.cells[r * self.width + c].clone();
            }
        }
        self.cells = cells;
        self.width = width;
        self.height = height;
        self.cursor = (
            self.cursor.0.min(height.saturating_sub(1)),
            self.cursor.1.min(width.saturating_sub(1)),
        );
    }

    pub fn cell(&self, row: usize, col: usize) -> Option<&Cell> {
        (row < self.height && col < self.width).then(|| &self.cells[row * self.width + col])
    }

    /// One row as text.
    pub fn row_text(&self, row: usize) -> String {
        (0..self.width)
            .filter_map(|c| self.cell(row, c))
            .map(|c| c.text.as_str())
            .collect()
    }

    /// The whole grid as text (rows joined with `\n`).
    pub fn text(&self) -> String {
        (0..self.height)
            .map(|r| self.row_text(r))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn clear(&mut self) {
        self.cells.iter_mut().for_each(|c| *c = Cell::default());
    }

    /// Applies one `redraw` notification (a batch of `[name, args...]` events).
    pub fn apply_redraw(&mut self, params: &[Value]) {
        for ev in params {
            let Some(ev) = ev.as_array() else { continue };
            let Some(name) = ev.first().and_then(Value::as_str) else {
                continue;
            };
            for args in ev.iter().skip(1) {
                let args = args.as_array().map(Vec::as_slice).unwrap_or(&[]);
                self.apply(name, args);
            }
        }
    }

    fn apply(&mut self, name: &str, a: &[Value]) {
        match name {
            "grid_resize" if a.len() >= 3 => self.resize(u(&a[1]), u(&a[2])),
            "grid_clear" => self.clear(),
            "grid_cursor_goto" if a.len() >= 3 => self.cursor = (u(&a[1]), u(&a[2])),
            "default_colors_set" if a.len() >= 2 => {
                self.default_fg = a[0].as_i64().filter(|v| *v >= 0).map(|v| v as u32);
                self.default_bg = a[1].as_i64().filter(|v| *v >= 0).map(|v| v as u32);
            }
            "hl_attr_define" if a.len() >= 2 => {
                let mut attr = HlAttr::default();
                for (k, v) in a[1].as_map().map(Vec::as_slice).unwrap_or(&[]) {
                    match k.as_str().unwrap_or_default() {
                        "foreground" => attr.fg = v.as_u64().map(|x| x as u32),
                        "background" => attr.bg = v.as_u64().map(|x| x as u32),
                        "bold" => attr.bold = v.as_bool().unwrap_or(false),
                        "italic" => attr.italic = v.as_bool().unwrap_or(false),
                        "underline" | "undercurl" => attr.underline = v.as_bool().unwrap_or(false),
                        "strikethrough" => attr.strikethrough = v.as_bool().unwrap_or(false),
                        "reverse" => attr.reverse = v.as_bool().unwrap_or(false),
                        _ => {}
                    }
                }
                self.hl.insert(u(&a[0]) as u64, attr);
            }
            "grid_line" if a.len() >= 4 => {
                let row = u(&a[1]);
                let mut col = u(&a[2]);
                let mut hl = 0u64;
                for cell in a[3].as_array().map(Vec::as_slice).unwrap_or(&[]) {
                    let Some(c) = cell.as_array() else { continue };
                    let text = c.first().and_then(Value::as_str).unwrap_or(" ").to_string();
                    if let Some(h) = c.get(1).and_then(Value::as_u64) {
                        hl = h;
                    }
                    let repeat = c.get(2).and_then(Value::as_u64).unwrap_or(1) as usize;
                    for _ in 0..repeat {
                        if row < self.height && col < self.width {
                            self.cells[row * self.width + col] = Cell { text: text.clone(), hl };
                        }
                        col += 1;
                    }
                }
            }
            "grid_scroll" if a.len() >= 7 => {
                let (top, bot, left, right) = (u(&a[1]), u(&a[2]), u(&a[3]), u(&a[4]));
                let rows = i(&a[5]);
                let (bot, right) = (bot.min(self.height), right.min(self.width));
                if rows > 0 {
                    let rows = rows as usize;
                    for r in top..bot.saturating_sub(rows) {
                        for c in left..right {
                            self.cells[r * self.width + c] = self.cells[(r + rows) * self.width + c].clone();
                        }
                    }
                } else if rows < 0 {
                    let rows = (-rows) as usize;
                    for r in (top + rows..bot).rev() {
                        for c in left..right {
                            self.cells[r * self.width + c] = self.cells[(r - rows) * self.width + c].clone();
                        }
                    }
                }
            }
            "mode_info_set" if a.len() >= 2 => {
                self.mode_names = a[1]
                    .as_array()
                    .map(|v| {
                        v.iter()
                            .map(|m| {
                                m.as_map()
                                    .and_then(|kv| kv.iter().find(|(k, _)| k.as_str() == Some("name")))
                                    .and_then(|(_, v)| v.as_str())
                                    .unwrap_or_default()
                                    .to_string()
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            }
            "mode_change" if !a.is_empty() => {
                self.mode = a[0].as_str().unwrap_or_default().to_string();
            }
            "flush" => self.frames += 1,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> Value {
        Value::from(x)
    }
    fn n(x: i64) -> Value {
        Value::from(x)
    }
    fn arr(v: Vec<Value>) -> Value {
        Value::Array(v)
    }
    fn ev(name: &str, calls: Vec<Vec<Value>>) -> Value {
        let mut v = vec![s(name)];
        v.extend(calls.into_iter().map(arr));
        arr(v)
    }

    #[test]
    fn applies_a_recorded_redraw_batch() {
        let mut g = Grid::default();
        g.apply_redraw(&[
            ev("grid_resize", vec![vec![n(1), n(10), n(3)]]),
            ev(
                "default_colors_set",
                vec![vec![n(0xA9B7C6), n(0x2B2B2B), n(-1), n(0), n(0)]],
            ),
            ev(
                "hl_attr_define",
                vec![vec![
                    n(5),
                    Value::Map(vec![(s("foreground"), n(0xFF0000)), (s("bold"), Value::from(true))]),
                    Value::Map(vec![]),
                    arr(vec![]),
                ]],
            ),
            ev(
                "grid_line",
                vec![vec![
                    n(1),
                    n(0),
                    n(0),
                    arr(vec![
                        arr(vec![s("h"), n(5)]),
                        arr(vec![s("i")]),
                        arr(vec![s("-"), n(0), n(3)]),
                    ]),
                    Value::from(false),
                ]],
            ),
            ev("grid_cursor_goto", vec![vec![n(1), n(0), n(1)]]),
            ev("mode_change", vec![vec![s("insert"), n(1)]]),
            ev("flush", vec![vec![]]),
        ]);
        assert_eq!((g.width, g.height), (10, 3));
        assert_eq!(g.row_text(0), "hi---     ");
        assert_eq!(g.cell(0, 1).unwrap().hl, 5, "hl id carries over to cells without one");
        assert_eq!(g.cell(0, 2).unwrap().hl, 0);
        assert!(g.hl[&5].bold);
        assert_eq!(g.hl[&5].fg, Some(0xFF0000));
        assert_eq!(g.default_bg, Some(0x2B2B2B));
        assert_eq!(g.cursor, (0, 1));
        assert_eq!(g.mode, "insert");
        assert_eq!(g.frames, 1);
    }

    #[test]
    fn scrolls_regions_up_and_down() {
        let mut g = Grid::default();
        g.resize(3, 4);
        for (r, t) in ["a", "b", "c", "d"].iter().enumerate() {
            g.apply(
                "grid_line",
                &[n(1), n(r as i64), n(0), arr(vec![arr(vec![s(t), n(0), n(3)])])],
            );
        }
        g.apply("grid_scroll", &[n(1), n(0), n(4), n(0), n(3), n(1), n(0)]);
        assert_eq!(g.text(), "bbb\nccc\nddd\nddd");
        g.apply("grid_scroll", &[n(1), n(0), n(4), n(0), n(3), n(-2), n(0)]);
        assert_eq!(g.text(), "bbb\nccc\nbbb\nccc");
    }

    #[test]
    fn resize_keeps_content_and_clamps_cursor() {
        let mut g = Grid::default();
        g.resize(4, 2);
        g.apply(
            "grid_line",
            &[n(1), n(0), n(0), arr(vec![arr(vec![s("x"), n(0), n(4)])])],
        );
        g.cursor = (1, 3);
        g.resize(2, 1);
        assert_eq!(g.text(), "xx");
        assert_eq!(g.cursor, (0, 1));
        g.apply("grid_clear", &[n(1)]);
        assert_eq!(g.text(), "  ");
    }
}

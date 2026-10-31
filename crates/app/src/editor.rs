//! 編集モードの状態と操作。Qtに依存しない。

use std::io;
use std::ops::Range;
use std::path::PathBuf;

use kakikotoba_core::Buffer;
use kakikotoba_layout::{Font, Layout, Rect, Settings};

const MARGIN: f32 = 24.0;
/// 擬A6（文庫）の既定字数。字サイズはウィンドウの高さから算出する
const CHARS_PER_COLUMN: usize = 39;
const COLUMN_PITCH_RATIO: f32 = 1.75;
const MIN_FONT_SIZE: f32 = 4.0;
const CARET_THICKNESS: f32 = 2.0;
const PREEDIT_LINE_WIDTH: f32 = 1.0;

struct Preedit {
    text: String,
    /// preedit先頭からのカーソル位置（文字単位）
    cursor: usize,
}

pub struct Editor {
    buffer: Buffer,
    path: Option<PathBuf>,
    font: Font<'static>,
    pub font_path: String,
    layout: Layout,
    settings: Settings,
    width: f32,
    height: f32,
    /// 右方向へのスクロール量。列は左へ進むので、大きいほど後ろの列が見える
    scroll: f32,
    cursor: usize,
    anchor: Option<usize>,
    preedit: Option<Preedit>,
    /// ←→で列を移るときに保つ天からの距離
    preferred_y: Option<f32>,
}

fn settings_for(height: f32) -> Settings {
    let font_size = ((height - 2.0 * MARGIN) / CHARS_PER_COLUMN as f32).max(MIN_FONT_SIZE);
    Settings {
        font_size,
        chars_per_column: CHARS_PER_COLUMN,
        column_pitch: font_size * COLUMN_PITCH_RATIO,
    }
}

impl Editor {
    pub fn new(buffer: Buffer, path: Option<PathBuf>, font: Font<'static>, font_path: String) -> Self {
        let settings = settings_for(0.0);
        let layout = Layout::new(&buffer.rope().to_string(), &font, settings);
        Self {
            buffer,
            path,
            font,
            font_path,
            layout,
            settings,
            width: 0.0,
            height: 0.0,
            scroll: 0.0,
            cursor: 0,
            anchor: None,
            preedit: None,
            preferred_y: None,
        }
    }

    pub fn font_size(&self) -> f32 {
        self.settings.font_size
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        if (width, height) == (self.width, self.height) {
            return;
        }
        let height_changed = height != self.height;
        self.width = width;
        self.height = height;
        if height_changed {
            self.settings = settings_for(height);
            self.relayout();
        }
        self.keep_caret_visible();
    }

    fn relayout(&mut self) {
        let mut text = self.buffer.rope().clone();
        if let Some(preedit) = &self.preedit {
            text.insert(self.cursor, &preedit.text);
        }
        self.layout = Layout::new(&text.to_string(), &self.font, self.settings);
    }

    fn to_screen(&self, rect: Rect) -> Rect {
        Rect {
            x: self.width - MARGIN + self.scroll + rect.x,
            y: MARGIN + rect.y,
            ..rect
        }
    }

    fn max_scroll(&self) -> f32 {
        let content = self.layout.column_count() as f32 * self.settings.column_pitch;
        (content - (self.width - 2.0 * MARGIN)).max(0.0)
    }

    fn keep_caret_visible(&mut self) {
        let rect = self.cursor_rect();
        if rect.x < MARGIN {
            self.scroll += MARGIN - rect.x;
        } else if rect.x + rect.width > self.width - MARGIN {
            self.scroll -= rect.x + rect.width - (self.width - MARGIN);
        }
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    fn caret_index(&self) -> usize {
        self.cursor + self.preedit.as_ref().map_or(0, |p| p.cursor)
    }

    /// 字面1字分のカーソル矩形（画面座標）。IMEへ渡す。
    pub fn cursor_rect(&self) -> Rect {
        self.to_screen(self.layout.cursor_rect(self.caret_index()))
    }

    /// 描画用の横棒カーソル（画面座標）。
    pub fn caret_rect(&self) -> Rect {
        Rect { height: CARET_THICKNESS, ..self.cursor_rect() }
    }

    pub fn cursor_position(&self) -> usize {
        self.caret_index()
    }

    pub fn anchor_position(&self) -> usize {
        self.anchor.unwrap_or(self.caret_index())
    }

    /// 画面内のグリフを `(番号, x, y)` で返す。
    pub fn visible_glyphs(&self) -> impl Iterator<Item = (u32, f32, f32)> + '_ {
        let offset_x = self.width - MARGIN + self.scroll;
        let (left, right) = (-self.settings.column_pitch, self.width + self.settings.column_pitch);
        self.layout.glyphs.iter().filter_map(move |g| {
            let x = g.x + offset_x;
            (left..right).contains(&x).then_some((g.id as u32, x, g.y + MARGIN))
        })
    }

    fn selection(&self) -> Option<Range<usize>> {
        let anchor = self.anchor.filter(|&a| a != self.cursor)?;
        Some(anchor.min(self.cursor)..anchor.max(self.cursor))
    }

    pub fn selection_rects(&self) -> Vec<Rect> {
        let range = self.selection().unwrap_or(0..0);
        self.layout.selection_rects(range).into_iter().map(|r| self.to_screen(r)).collect()
    }

    /// preeditの傍線（列の右側、画面座標）。
    pub fn preedit_lines(&self) -> Vec<Rect> {
        let Some(preedit) = &self.preedit else {
            return Vec::new();
        };
        let range = self.cursor..self.cursor + preedit.text.chars().count();
        self.layout
            .selection_rects(range)
            .into_iter()
            .map(|r| {
                let r = self.to_screen(r);
                Rect { x: r.x + r.width, width: PREEDIT_LINE_WIDTH, ..r }
            })
            .collect()
    }

    fn set_cursor(&mut self, index: usize, extend: bool) {
        if extend {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = index;
        self.keep_caret_visible();
    }

    pub fn move_down(&mut self, extend: bool) {
        self.preferred_y = None;
        self.set_cursor(self.layout.next_char(self.cursor), extend);
    }

    pub fn move_up(&mut self, extend: bool) {
        self.preferred_y = None;
        self.set_cursor(self.layout.prev_char(self.cursor), extend);
    }

    /// `delta` は正で左の列へ。
    pub fn move_columns(&mut self, delta: isize, extend: bool) {
        let y = *self.preferred_y.get_or_insert(self.layout.cursor_rect(self.cursor).y);
        self.set_cursor(self.layout.move_column(self.cursor, delta, y), extend);
    }

    pub fn click(&mut self, x: f32, y: f32, extend: bool) {
        if self.preedit.is_some() {
            return;
        }
        self.preferred_y = None;
        let index = self.layout.hit_test(x - (self.width - MARGIN + self.scroll), y - MARGIN);
        self.set_cursor(index, extend);
    }

    pub fn scroll_by(&mut self, delta: f32) {
        self.scroll = (self.scroll + delta).clamp(0.0, self.max_scroll());
    }

    fn edited(&mut self, cursor: usize) {
        self.cursor = cursor;
        self.anchor = None;
        self.preferred_y = None;
        self.relayout();
        self.keep_caret_visible();
    }

    pub fn insert(&mut self, text: &str) {
        let range = self.selection().unwrap_or(self.cursor..self.cursor);
        let cursor = self.buffer.replace(range, text);
        self.edited(cursor);
    }

    pub fn backspace(&mut self) {
        let range = self.selection().unwrap_or(self.layout.prev_char(self.cursor)..self.cursor);
        let cursor = self.buffer.replace(range, "");
        self.edited(cursor);
    }

    pub fn delete(&mut self) {
        let range = self.selection().unwrap_or(self.cursor..self.layout.next_char(self.cursor));
        let cursor = self.buffer.replace(range, "");
        self.edited(cursor);
    }

    pub fn undo(&mut self) {
        if let Some(cursor) = self.buffer.undo() {
            self.edited(cursor);
        }
    }

    pub fn redo(&mut self) {
        if let Some(cursor) = self.buffer.redo() {
            self.edited(cursor);
        }
    }

    pub fn is_composing(&self) -> bool {
        self.preedit.is_some()
    }

    /// `cursor` はpreedit先頭からの文字数。`None` なら末尾。
    pub fn set_preedit(&mut self, text: &str, cursor: Option<usize>) {
        if text.is_empty() {
            self.preedit = None;
        } else {
            if let Some(range) = self.selection() {
                self.cursor = self.buffer.replace(range, "");
                self.anchor = None;
            }
            let length = text.chars().count();
            self.preedit = Some(Preedit {
                text: text.to_owned(),
                cursor: cursor.unwrap_or(length).min(length),
            });
        }
        self.relayout();
        self.keep_caret_visible();
    }

    /// カーソルから `start` 文字先の `length` 文字を `text` で置き換えて確定する。
    pub fn commit(&mut self, start: isize, length: usize, text: &str) {
        self.preedit = None;
        let range = if (start, length) == (0, 0) {
            self.selection().unwrap_or(self.cursor..self.cursor)
        } else {
            let end_of_text = self.buffer.rope().len_chars();
            let from = self.cursor.saturating_add_signed(start).min(end_of_text);
            from..(from + length).min(end_of_text)
        };
        let cursor = self.buffer.replace(range, text);
        self.edited(cursor);
    }

    /// 保存先が未指定なら `InvalidInput` エラーを返す。
    pub fn save(&mut self) -> io::Result<()> {
        let path = self.path.clone().ok_or(io::ErrorKind::InvalidInput)?;
        self.buffer.save(&path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_editor(text: &str, test: impl FnOnce(&mut Editor)) {
        let path = "/usr/share/fonts/noto-cjk/NotoSerifCJK-Regular.ttc";
        let Ok(data) = std::fs::read(path) else {
            eprintln!("スキップ: {path} がない");
            return;
        };
        let font = Font::new(Box::leak(data.into_boxed_slice()), 0).unwrap();
        let mut editor = Editor::new(Buffer::new(text), None, font, path.to_owned());
        editor.resize(400.0, 39.0 * 10.0 + 2.0 * MARGIN);
        test(&mut editor);
    }

    fn text(editor: &Editor) -> String {
        editor.buffer.rope().to_string()
    }

    #[test]
    fn preedit_is_displayed_but_not_in_buffer_until_commit() {
        with_editor("あい", |e| {
            e.move_down(false);
            let before = e.cursor_rect().y;
            e.set_preedit("かき", None);
            assert_eq!(text(e), "あい");
            assert_eq!(e.cursor_rect().y, before + 20.0);
            assert_eq!(e.preedit_lines()[0].height, 20.0);

            e.set_preedit("かき", Some(1));
            assert_eq!(e.cursor_rect().y, before + 10.0);

            e.commit(0, 0, "柿");
            assert!(!e.is_composing());
            assert_eq!(text(e), "あ柿い");
            assert_eq!(e.cursor_rect().y, before + 10.0);
            e.undo();
            assert_eq!(text(e), "あい");
        });
    }

    #[test]
    fn preedit_replaces_selection_and_cancel_restores_display() {
        with_editor("あいう", |e| {
            e.move_down(true);
            e.set_preedit("か", None);
            assert_eq!(text(e), "いう");
            e.set_preedit("", None);
            assert!(!e.is_composing());
            assert!(e.preedit_lines().is_empty());
            assert_eq!(e.cursor_rect().y, MARGIN);
        });
    }

    #[test]
    fn typing_keeps_caret_on_screen_as_columns_grow() {
        with_editor("", |e| {
            for _ in 0..50 {
                e.insert("あ\n");
                let rect = e.cursor_rect();
                assert!(rect.x >= MARGIN && rect.x + rect.width <= e.width - MARGIN, "{rect:?}");
            }
            assert!(e.scroll > 0.0);
            for _ in 0..50 {
                e.undo();
            }
            assert_eq!(e.scroll, 0.0);
        });
    }

    #[test]
    fn left_right_keep_preferred_height_across_short_column() {
        with_editor("あいう\nか\nさしす", |e| {
            e.move_down(false);
            e.move_down(false);
            e.move_columns(1, false);
            e.move_columns(1, false);
            assert_eq!(e.cursor, 8);
            e.move_down(false);
            e.move_columns(-1, false);
            assert_eq!(e.cursor, 5);
        });
    }

    #[test]
    fn click_maps_screen_position_after_scroll() {
        with_editor("あいう\nか", |e| {
            let rect = e.layout.cursor_rect(4);
            let screen = e.to_screen(rect);
            e.click(screen.x + 1.0, screen.y + 1.0, false);
            assert_eq!(e.cursor, 4);
            e.click(screen.x + 1.0, screen.y + 1.0, true);
            assert_eq!(e.selection(), None);
        });
    }

    #[test]
    fn backspace_removes_crlf_as_one() {
        with_editor("あ\r\nい", |e| {
            e.click(e.cursor_rect().x, 1000.0, false);
            e.move_columns(1, false);
            e.move_up(false);
            assert_eq!(e.cursor, 3);
            e.move_up(false);
            e.move_down(false);
            e.backspace();
            assert_eq!(text(e), "あい");
        });
    }
}

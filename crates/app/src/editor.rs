//! 編集モードの状態と操作。Qtに依存しない。

use std::io;
use std::ops::Range;
use std::path::PathBuf;

use kakikotoba_core::Buffer;
use kakikotoba_layout::page::{PageFormat, PageSettings, SpreadMode};
use kakikotoba_layout::{Font, Layout, Rect, Run, Settings};
use kakikotoba_markup::highlight::{Dialect, Highlighter, Options};

use crate::page_view::PageView;

const MARGIN: f32 = 24.0;
/// 擬A6（文庫）の既定字数。字サイズはウィンドウの高さから算出する
const CHARS_PER_COLUMN: usize = 39;
/// 行末のぶら下げが画面の外へ出ないよう、1字分を余分に確保する
const HANGING_CHARS: usize = 1;
const COLUMN_PITCH_RATIO: f32 = 1.75;
const MIN_FONT_SIZE: f32 = 4.0;
const CARET_THICKNESS: f32 = 2.0;
const PREEDIT_LINE_WIDTH: f32 = 1.0;

pub struct Fonts {
    pub serif: Font<'static>,
    pub serif_path: String,
    /// 太字（ゴシック）
    pub sans: Font<'static>,
    pub sans_path: String,
}

/// 画面に描くグリフ。
pub struct VisibleGlyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub color: u8,
    /// 字サイズ（px）
    pub size: f32,
    pub rotated: bool,
}

struct Preedit {
    text: String,
    /// preedit先頭からのカーソル位置（文字単位）
    cursor: usize,
}

pub struct Editor {
    buffer: Buffer,
    path: Option<PathBuf>,
    pub fonts: Fonts,
    layout: Layout,
    settings: Settings,
    highlighter: Highlighter,
    highlight_enabled: bool,
    width: f32,
    height: f32,
    /// 右方向へのスクロール量。列は左へ進むので、大きいほど後ろの列が見える
    scroll: f32,
    cursor: usize,
    anchor: Option<usize>,
    preedit: Option<Preedit>,
    /// ←→で列を移るときに保つ天からの距離
    preferred_y: Option<f32>,
    page_settings: PageSettings,
    /// ページモード中だけある
    page_view: Option<PageView>,
}

fn settings_for(height: f32) -> Settings {
    let font_size = ((height - 2.0 * MARGIN) / (CHARS_PER_COLUMN + HANGING_CHARS) as f32).max(MIN_FONT_SIZE);
    Settings {
        font_size,
        chars_per_column: CHARS_PER_COLUMN,
        column_pitch: font_size * COLUMN_PITCH_RATIO,
    }
}

impl Editor {
    pub fn new(buffer: Buffer, path: Option<PathBuf>, fonts: Fonts) -> Self {
        let settings = settings_for(0.0);
        let dialect = Dialect::from_path(path.as_deref());
        let layout = Layout::new("", &fonts.serif, settings, &[]);
        let (chars_per_line, lines_per_page) = PageFormat::A6.default_size();
        let mut editor = Self {
            buffer,
            path,
            fonts,
            layout,
            settings,
            highlighter: Highlighter::new(Options { dialect, symbol_forms: true }),
            highlight_enabled: true,
            width: 0.0,
            height: 0.0,
            scroll: 0.0,
            cursor: 0,
            anchor: None,
            preedit: None,
            preferred_y: None,
            page_settings: PageSettings {
                format: PageFormat::A6,
                chars_per_line,
                lines_per_page,
                spread: SpreadMode::LeftStart,
            },
            page_view: None,
        };
        editor.relayout();
        editor
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
        let text = text.to_string();
        let mut runs = Vec::new();
        if self.highlight_enabled {
            self.highlighter.update(&text);
            // 色0は既定の文字色
            runs.extend(self.highlighter.spans().map(|span| Run { range: span.range, color: span.kind as u8 + 1 }));
        }
        self.layout = Layout::new(&text, &self.fonts.serif, self.settings, &runs);
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

    /// 画面内のグリフを返す。
    pub fn visible_glyphs(&self) -> impl Iterator<Item = VisibleGlyph> + '_ {
        let offset_x = self.width - MARGIN + self.scroll;
        let (left, right) = (-self.settings.column_pitch, self.width + self.settings.column_pitch);
        self.layout.glyphs.iter().filter_map(move |g| {
            let x = g.x + offset_x;
            (left..right).contains(&x).then_some(VisibleGlyph {
                id: g.id as u32,
                x,
                y: g.y + MARGIN,
                color: g.color,
                size: self.settings.font_size * g.scale,
                rotated: g.rotated,
            })
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

    pub fn selected_text(&self) -> Option<String> {
        self.selection().map(|range| self.buffer.rope().slice(range).to_string())
    }

    pub fn select_all(&mut self) {
        self.preferred_y = None;
        self.anchor = Some(0);
        self.set_cursor(self.buffer.rope().len_chars(), true);
    }

    /// `query` の次（`backward` なら前）の一致を選択する。端まで行ったら反対の端へ戻る。
    // ponytail: 検索のたびに全文を走査。長い文書で遅くなったら索引を持つ
    pub fn find(&mut self, query: &str, backward: bool) -> bool {
        let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
        let query: Vec<char> = query.chars().map(fold).collect();
        let text: Vec<char> = self.buffer.rope().chars().map(fold).collect();
        if query.is_empty() {
            return false;
        }
        let matches = |&start: &usize| text[start..].starts_with(&query);
        let Some(last) = text.len().checked_sub(query.len()) else {
            return false;
        };
        let (start, end) = self.selection().map_or((self.cursor, self.cursor), |range| (range.start, range.end));
        let found = if backward {
            let before = start.checked_sub(query.len()).map_or(0..0, |limit| 0..limit + 1);
            before.rev().find(matches).or_else(|| (0..=last).rev().find(matches))
        } else {
            (end..=last).find(matches).or_else(|| (0..=last).find(matches))
        };
        let Some(found) = found else {
            return false;
        };
        self.preferred_y = None;
        self.anchor = Some(found);
        self.set_cursor(found + query.len(), true);
        true
    }

    pub fn is_modified(&self) -> bool {
        self.buffer.is_modified()
    }

    pub fn has_file(&self) -> bool {
        self.path.is_some()
    }

    /// ファイル名。未保存の新規文書なら空文字列。
    pub fn document_name(&self) -> String {
        let name = self.path.as_deref().and_then(|path| path.file_name());
        name.map_or(String::new(), |name| name.to_string_lossy().into_owned())
    }

    /// 保存先が未指定なら `InvalidInput` エラーを返す。
    pub fn save(&mut self) -> io::Result<()> {
        let path = self.path.clone().ok_or(io::ErrorKind::InvalidInput)?;
        self.buffer.save(&path)
    }

    pub fn save_as(&mut self, path: PathBuf) -> io::Result<()> {
        self.buffer.save(&path)?;
        self.path = Some(path);
        self.update_highlighter(self.highlighter.options().symbol_forms);
        self.relayout();
        self.rebuild_page_view();
        Ok(())
    }

    pub fn set_highlight_options(&mut self, enabled: bool, symbol_forms: bool) {
        self.highlight_enabled = enabled;
        self.update_highlighter(symbol_forms);
        self.relayout();
        self.rebuild_page_view();
    }

    /// 方言や記号形式が変わったときだけ解析結果を捨てる。
    fn update_highlighter(&mut self, symbol_forms: bool) {
        let options = Options { dialect: Dialect::from_path(self.path.as_deref()), symbol_forms };
        if options != self.highlighter.options() {
            self.highlighter = Highlighter::new(options);
        }
    }

    pub fn page_view(&self) -> Option<&PageView> {
        self.page_view.as_ref()
    }

    pub fn page_view_mut(&mut self) -> Option<&mut PageView> {
        self.page_view.as_mut()
    }

    fn markup_options(&self) -> Options {
        Options {
            dialect: Dialect::from_path(self.path.as_deref()),
            symbol_forms: self.highlighter.options().symbol_forms,
        }
    }

    fn build_page_view(&self, source: usize) -> PageView {
        let document = kakikotoba_markup::parse(&self.buffer.rope().to_string(), self.markup_options());
        let directory = self.path.as_deref().and_then(|path| path.parent()).map(PathBuf::from);
        PageView::new(&document, &self.fonts.serif, &self.fonts.sans, self.page_settings, source, directory)
    }

    /// ページモードと編集モードを切り替える。変換中は切り替えない。
    pub fn toggle_page_mode(&mut self) {
        if self.is_composing() {
            return;
        }
        self.page_view = match self.page_view {
            Some(_) => None,
            None => Some(self.build_page_view(self.cursor)),
        };
    }

    /// ページ上の位置に対応する編集位置へ移り、編集モードへ戻る。文字がなければ何もしない。
    pub fn jump_to_editing(&mut self, x: f32, y: f32) {
        let Some(source) = self.page_view.as_ref().and_then(|view| view.hit_test(x, y, self.width, self.height)) else {
            return;
        };
        self.page_view = None;
        self.preferred_y = None;
        self.set_cursor(source.min(self.buffer.rope().len_chars()), false);
    }

    pub fn set_page_settings(&mut self, settings: PageSettings) {
        self.page_settings = settings;
        self.rebuild_page_view();
    }

    /// 組版に関わる設定が変わったら、同じ箇所を開いたまま組み直す。
    fn rebuild_page_view(&mut self) {
        if let Some(view) = &self.page_view {
            let source = view.current_source().unwrap_or(self.cursor);
            self.page_view = Some(self.build_page_view(source));
        }
    }

    /// 文書情報を `項目: 値` の行にして返す。
    pub fn metadata_text(&self) -> String {
        let document = kakikotoba_markup::parse(&self.buffer.rope().to_string(), self.markup_options());
        let lines: Vec<String> = document.metadata.iter().map(|m| format!("{}: {}", m.label, m.value)).collect();
        lines.join("\n")
    }

    /// 読み込みに失敗したら、編集中の文書はそのまま残す。
    pub fn open(&mut self, path: PathBuf) -> io::Result<()> {
        let buffer = Buffer::load(&path)?;
        self.replace_document(buffer, Some(path));
        Ok(())
    }

    pub fn new_document(&mut self) {
        self.replace_document(Buffer::new(""), None);
    }

    fn replace_document(&mut self, buffer: Buffer, path: Option<PathBuf>) {
        self.buffer = buffer;
        self.page_view = None;
        self.path = path;
        self.update_highlighter(self.highlighter.options().symbol_forms);
        self.scroll = 0.0;
        self.preedit = None;
        self.edited(0);
    }

    /// 編集の操作を受け付けられるか。
    pub fn is_editable(&self) -> bool {
        self.page_view.is_none() && !self.is_composing()
    }

    pub fn can_undo(&self) -> bool {
        self.is_editable() && self.buffer.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.is_editable() && self.buffer.can_redo()
    }

    pub fn has_selection(&self) -> bool {
        self.is_editable() && self.selection().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kakikotoba_markup::highlight::Kind;

    fn with_editor(text: &str, test: impl FnOnce(&mut Editor)) {
        let path = "/usr/share/fonts/noto-cjk/NotoSerifCJK-Regular.ttc";
        let Ok(data) = std::fs::read(path) else {
            eprintln!("スキップ: {path} がない");
            return;
        };
        let data: &'static [u8] = Box::leak(data.into_boxed_slice());
        let fonts = Fonts {
            serif: Font::new(data, 0).unwrap(),
            serif_path: path.to_owned(),
            sans: Font::new(data, 0).unwrap(),
            sans_path: path.to_owned(),
        };
        let mut editor = Editor::new(Buffer::new(text), None, fonts);
        editor.resize(400.0, (CHARS_PER_COLUMN + HANGING_CHARS) as f32 * 10.0 + 2.0 * MARGIN);
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
    fn select_all_returns_whole_text_and_typing_replaces_it() {
        with_editor("あい\nう", |e| {
            assert_eq!(e.selected_text(), None);
            e.select_all();
            assert_eq!(e.selected_text().as_deref(), Some("あい\nう"));
            e.insert("か");
            assert_eq!(text(e), "か");
            assert_eq!(e.selected_text(), None);
        });
    }

    #[test]
    fn find_moves_through_matches_and_wraps_at_both_ends() {
        with_editor("猫と犬\nCat猫", |e| {
            assert!(e.find("猫", false));
            assert_eq!(e.selection(), Some(0..1));
            assert_eq!(e.selected_text().as_deref(), Some("猫"));
            assert!(e.find("猫", false));
            assert_eq!(e.selection(), Some(7..8));
            assert!(e.find("猫", false));
            assert_eq!(e.selection(), Some(0..1));
            assert!(e.find("猫", true));
            assert_eq!(e.selection(), Some(7..8));
            assert!(e.find("猫", true));
            assert_eq!(e.selection(), Some(0..1));
        });
    }

    #[test]
    fn find_ignores_case_and_reports_no_match_without_moving() {
        with_editor("猫と犬\nCat", |e| {
            assert!(e.find("cAT", false));
            assert_eq!(e.selected_text().as_deref(), Some("Cat"));
            assert!(!e.find("鳥", false));
            assert!(!e.find("", false));
            assert!(!e.find("とても長い検索語で本文より長い", true));
            assert_eq!(e.selected_text().as_deref(), Some("Cat"));
            // 改行をまたがない
            assert!(!e.find("犬C", false));
        });
    }

    #[test]
    fn save_as_then_open_switches_document_and_failed_open_keeps_it() {
        with_editor("", |e| {
            let dir = std::env::temp_dir().join(format!("kakikotoba-app-test-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let (first, second, broken) = (dir.join("一.txt"), dir.join("二.txt"), dir.join("壊.txt"));
            std::fs::write(&second, "別の文書").unwrap();
            std::fs::write(&broken, [0xff, 0xfe]).unwrap();

            assert!(!e.has_file());
            assert_eq!(e.document_name(), "");
            assert_eq!(e.save().unwrap_err().kind(), io::ErrorKind::InvalidInput);

            e.insert("あ");
            assert!(e.is_modified());
            e.save_as(first.clone()).unwrap();
            assert!(!e.is_modified());
            assert_eq!(e.document_name(), "一.txt");

            e.insert("い");
            assert!(e.open(broken).is_err());
            assert_eq!(text(e), "あい");
            assert_eq!(e.document_name(), "一.txt");
            assert!(e.is_modified());

            e.open(second).unwrap();
            assert_eq!(text(e), "別の文書");
            assert_eq!(e.document_name(), "二.txt");
            assert!(!e.is_modified());
            assert_eq!(e.cursor, 0);
            assert_eq!(e.buffer.undo(), None);

            std::fs::remove_dir_all(&dir).unwrap();
        });
    }

    fn colors(editor: &Editor) -> Vec<u8> {
        editor.visible_glyphs().map(|glyph| glyph.color).collect()
    }

    #[test]
    fn highlight_follows_edits_options_and_preedit() {
        with_editor("!あ", |e| {
            let comment = Kind::Comment as u8 + 1;
            assert_eq!(colors(e), [comment, comment]);

            e.set_highlight_options(true, false);
            assert_eq!(colors(e), [0, 0]);
            e.set_highlight_options(false, true);
            assert_eq!(colors(e), [0, 0]);
            e.set_highlight_options(true, true);
            assert_eq!(colors(e), [comment, comment]);

            // 編集とUndo、変換中の文字にも追従する
            e.backspace();
            e.delete();
            assert_eq!(colors(e), [0]);
            e.undo();
            assert_eq!(colors(e), [comment, comment]);
            e.set_preedit("か", None);
            assert_eq!(colors(e), [comment, comment, comment]);
        });
    }

    #[test]
    fn dialect_follows_saved_and_opened_file() {
        with_editor("*強調*", |e| {
            let dir = std::env::temp_dir().join(format!("kakikotoba-dialect-test-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let (emphasis, delimiter, tag) = (Kind::Emphasis as u8 + 1, Kind::Delimiter as u8 + 1, Kind::Tag as u8 + 1);
            assert_eq!(colors(e), [tag, 0, 0, 0]);

            e.save_as(dir.join("a.md")).unwrap();
            assert_eq!(colors(e), [delimiter, emphasis, emphasis, delimiter]);

            let txt = dir.join("b.txt");
            std::fs::write(&txt, "*強調*").unwrap();
            e.open(txt).unwrap();
            assert_eq!(colors(e), [tag, 0, 0, 0]);
            std::fs::remove_dir_all(&dir).unwrap();
        });
    }

    const SMALL_PAGE: PageSettings = PageSettings {
        format: PageFormat::A6,
        chars_per_line: 5,
        lines_per_page: 2,
        spread: SpreadMode::FullSpread,
    };

    #[test]
    fn page_mode_opens_at_cursor_and_click_jumps_back() {
        // 1ページ10字。全角12字目は2ページ目
        with_editor("あいうえおかきくけこさしすせそ", |e| {
            e.set_page_settings(SMALL_PAGE);
            for _ in 0..12 {
                e.move_down(false);
            }
            e.toggle_page_mode();
            let view = e.page_view().unwrap();
            assert_eq!(view.label(), "1-2 / 2");
            let placed = view.placed_pages(e.width, e.height);
            let em = view.em(e.width, e.height);
            let stop = view.document.pages[1].stops[2].clone();
            assert_eq!(stop.source, 12);
            let (x, y) = (placed[1].x + (stop.rect.x + 0.5) * em, placed[1].y + (stop.rect.y + 0.5) * em);

            // 余白の外（ページの外）では何も起きない
            e.jump_to_editing(0.0, 0.0);
            assert!(e.page_view().is_some());

            e.jump_to_editing(x, y);
            assert!(e.page_view().is_none());
            assert_eq!(e.cursor, 12);
        });
    }

    #[test]
    fn spread_navigation_clamps_and_settings_keep_the_place() {
        with_editor(&"あ".repeat(50), |e| {
            e.set_page_settings(SMALL_PAGE);
            e.toggle_page_mode();
            let view = e.page_view_mut().unwrap();
            view.previous_spread();
            assert_eq!(view.label(), "1-2 / 5");
            view.next_spread();
            assert_eq!(view.label(), "3-4 / 5");
            view.last_spread();
            assert_eq!(view.label(), "5 / 5");
            view.next_spread();
            assert_eq!(view.label(), "5 / 5");
            view.first_spread();
            view.next_spread();

            // 21字目が見えている見開きのまま、字数を倍にする
            e.set_page_settings(PageSettings { chars_per_line: 10, ..SMALL_PAGE });
            assert_eq!(e.page_view().unwrap().label(), "1-2 / 3");
            e.toggle_page_mode();
            assert_eq!(e.cursor, 0, "編集位置は動かない");
        });
    }

    #[test]
    fn composing_blocks_page_mode_and_open_leaves_it() {
        with_editor("あ", |e| {
            e.set_preedit("か", None);
            e.toggle_page_mode();
            assert!(e.page_view().is_none());
            e.commit(0, 0, "か");
            e.toggle_page_mode();
            assert!(e.page_view().is_some());
            let dir = std::env::temp_dir().join(format!("kakikotoba-page-test-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("a.txt");
            std::fs::write(&path, "い").unwrap();
            e.open(path).unwrap();
            assert!(e.page_view().is_none());
            std::fs::remove_dir_all(&dir).unwrap();
        });
    }

    #[test]
    fn metadata_text_lists_document_information() {
        with_editor("#TITLE# 題\n本文\n#筆者# 誰か", |e| {
            assert_eq!(e.metadata_text(), "タイトル: 題\n筆者: 誰か");
        });
    }

    #[test]
    fn image_paths_are_relative_to_the_document() {
        with_editor("#IMG:N# a.jpg", |e| {
            let dir = std::env::temp_dir().join(format!("kakikotoba-image-test-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            e.save_as(dir.join("本文.txt")).unwrap();
            e.toggle_page_mode();
            let view = e.page_view().unwrap();
            assert_eq!(view.document.pages.iter().filter_map(|p| p.image.as_deref()).collect::<Vec<_>>(), ["a.jpg"]);
            assert_eq!(view.image_path("a.jpg"), dir.join("a.jpg"));
            assert_eq!(view.image_path("/x/a.jpg"), PathBuf::from("/x/a.jpg"));
            std::fs::remove_dir_all(&dir).unwrap();
        });
    }

    #[test]
    fn new_document_clears_text_path_and_history() {
        with_editor("あ", |e| {
            let dir = std::env::temp_dir().join(format!("kakikotoba-new-test-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            e.save_as(dir.join("a.md")).unwrap();
            e.insert("い");
            e.toggle_page_mode();
            e.new_document();
            assert_eq!(text(e), "");
            assert!(!e.has_file() && !e.is_modified() && !e.can_undo());
            assert!(e.page_view().is_none());
            // 新規文書はPreTNFに戻る
            assert_eq!(e.markup_options().dialect, Dialect::PreTnf);
            std::fs::remove_dir_all(&dir).unwrap();
        });
    }

    #[test]
    fn edit_commands_are_unavailable_in_page_mode_and_while_composing() {
        with_editor("あ", |e| {
            e.select_all();
            e.insert("い");
            e.select_all();
            assert!(e.can_undo() && e.has_selection() && e.is_editable());

            e.toggle_page_mode();
            assert!(!e.can_undo() && !e.has_selection() && !e.is_editable());
            e.toggle_page_mode();

            e.undo();
            assert!(e.can_redo());
            e.set_preedit("か", None);
            assert!(!e.can_undo() && !e.can_redo() && !e.is_editable());
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

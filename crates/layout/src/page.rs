//! ページモードの組版。共通ASTからページ割りを作る。
//!
//! 長さはすべてem単位（字サイズ1）で、ウィンドウサイズに依存しない。描画時に拡縮する。
//! 座標はページの左上が原点で、yは下向き。

use std::collections::VecDeque;

use kakikotoba_markup::ast::{Block, BlockKind, Document, ImageAlign, Inline, InlineKind};

use crate::{Font, Rect, TCY_BASELINE, kinsoku};

/// A判・B判は縦横比が同じ
const PAGE_ASPECT: f32 = std::f32::consts::SQRT_2;
const RUBY_SCALE: f32 = 0.5;
/// 列の中心からルビの中心まで（字サイズ比）
const RUBY_CENTER: f32 = 0.75;
const CHAPTER_SCALE: f32 = 1.5;
const CHAPTER_COLUMNS: usize = 2;
const STRONG_SCALE: f32 = 1.25;
const HORIZONTAL_SCALE: f32 = 0.8;
const HORIZONTAL_LINE_HEIGHT: f32 = 1.2;
const LINE_THICKNESS: f32 = 0.05;
const WAVE_CHARS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageFormat {
    /// 擬A6（文庫）
    A6,
    /// 擬B6
    B6,
}

impl PageFormat {
    /// 既定の `(字数, 行数)`
    pub fn default_size(self) -> (usize, usize) {
        match self {
            Self::A6 => (39, 17),
            Self::B6 => (43, 18),
        }
    }

    /// `(天地の余白, 左右の余白)`。ページの長さに対する片側の比率
    fn margins(self) -> (f32, f32) {
        match self {
            Self::A6 => (0.09, 0.10),
            Self::B6 => (0.08, 0.09),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpreadMode {
    /// 1ページ目を左に単独表示し、右は空白
    LeftStart,
    /// 1ページ目を右、2ページ目を左
    FullSpread,
}

impl SpreadMode {
    /// 0始まりのページ番号が左ページか。
    fn is_left(self, page: usize) -> bool {
        match self {
            Self::LeftStart => page.is_multiple_of(2),
            Self::FullSpread => !page.is_multiple_of(2),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSettings {
    pub format: PageFormat,
    pub chars_per_line: usize,
    pub lines_per_page: usize,
    pub spread: SpreadMode,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageGlyph {
    pub id: u16,
    pub x: f32,
    pub y: f32,
    /// 字サイズ（em）
    pub scale: f32,
    /// ゴシック（太字）か
    pub sans: bool,
}

/// 元ソースとの対応点。
#[derive(Clone, Debug, PartialEq)]
pub struct Stop {
    pub source: usize,
    pub rect: Rect,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    pub glyphs: Vec<PageGlyph>,
    /// 傍線、ダッシュ、水平線
    pub lines: Vec<Rect>,
    /// 波線
    pub waves: Vec<Rect>,
    /// 挿絵ページの画像ファイル名
    pub image: Option<String>,
    pub stops: Vec<Stop>,
}

pub struct PagedDocument {
    pub pages: Vec<Page>,
    pub page_width: f32,
    pub page_height: f32,
    spread: SpreadMode,
}

impl PagedDocument {
    pub fn new(document: &Document, serif: &Font, sans: &Font, settings: PageSettings) -> Self {
        let mut flow = Flow::new(serif, sans, settings);
        for block in &document.blocks {
            flow.block(block);
        }
        flow.finish()
    }

    pub fn spread_count(&self) -> usize {
        match self.spread {
            SpreadMode::LeftStart => self.pages.len() / 2 + 1,
            SpreadMode::FullSpread => self.pages.len().div_ceil(2).max(1),
        }
    }

    /// 見開きの `(右ページ, 左ページ)`。
    pub fn spread_pages(&self, spread: usize) -> (Option<usize>, Option<usize>) {
        let (right, left) = match self.spread {
            SpreadMode::LeftStart => ((spread * 2).checked_sub(1), spread * 2),
            SpreadMode::FullSpread => (Some(spread * 2), spread * 2 + 1),
        };
        (right.filter(|&p| p < self.pages.len()), Some(left).filter(|&p| p < self.pages.len()))
    }

    pub fn spread_of_page(&self, page: usize) -> usize {
        match self.spread {
            SpreadMode::LeftStart => page.div_ceil(2),
            SpreadMode::FullSpread => page / 2,
        }
    }

    /// 元ソースの位置を含むページ。
    pub fn page_of_source(&self, source: usize) -> usize {
        let mut best = (0, 0);
        for (page, content) in self.pages.iter().enumerate() {
            for stop in &content.stops {
                if stop.source <= source && stop.source >= best.1 {
                    best = (page, stop.source);
                }
            }
        }
        best.0
    }

    /// ページ上の座標に最も近い元ソースの位置。
    pub fn hit_test(&self, page: usize, x: f32, y: f32) -> Option<usize> {
        let distance = |rect: &Rect| {
            let dx = (rect.x - x).max(x - (rect.x + rect.width)).max(0.0);
            let dy = (rect.y - y).max(y - (rect.y + rect.height)).max(0.0);
            dx * dx + dy * dy
        };
        let stops = &self.pages.get(page)?.stops;
        let nearest = stops.iter().min_by(|a, b| distance(&a.rect).total_cmp(&distance(&b.rect)))?;
        Some(nearest.source)
    }
}

/// 列中心・ペン位置からの相対位置で持つ、1単位の組版要素。
#[derive(Clone, Debug, Default)]
struct Piece {
    first: char,
    last: char,
    advance: f32,
    source: usize,
    glyphs: Vec<PageGlyph>,
    lines: Vec<Rect>,
    waves: Vec<Rect>,
}

impl Piece {
    fn new(first: char, last: char, advance: f32, source: usize) -> Self {
        Self { first, last, advance, source, ..Self::default() }
    }
}

#[derive(Clone, Copy)]
struct Style {
    scale: f32,
    sans: bool,
    emphasis: bool,
    sideline: bool,
}

const BODY: Style = Style { scale: 1.0, sans: false, emphasis: false, sideline: false };

struct Column {
    pieces: Vec<Piece>,
    /// 占める行数
    width: usize,
    y_offset: f32,
    /// 空の列の元ソース位置
    source: usize,
}

struct PendingImage {
    align: ImageAlign,
    source_name: String,
    source: usize,
}

struct Flow<'a, 'f> {
    serif: &'a Font<'f>,
    sans: &'a Font<'f>,
    settings: PageSettings,
    page_width: f32,
    page_height: f32,
    margin_top: f32,
    text_right: f32,
    pitch: f32,
    pages: Vec<Page>,
    current: Page,
    /// 現在のページで使った行数
    used: usize,
    images: VecDeque<PendingImage>,
    /// ページセンタリング中の列
    centered: Option<Vec<Column>>,
    /// 全角ダブルクォートの開閉
    quote_open: bool,
}

impl<'a, 'f> Flow<'a, 'f> {
    fn new(serif: &'a Font<'f>, sans: &'a Font<'f>, settings: PageSettings) -> Self {
        let (margin_v, margin_h) = settings.format.margins();
        let page_height = settings.chars_per_line as f32 / (1.0 - 2.0 * margin_v);
        let page_width = page_height / PAGE_ASPECT;
        Self {
            serif,
            sans,
            settings,
            page_width,
            page_height,
            margin_top: page_height * margin_v,
            text_right: page_width * (1.0 - margin_h),
            pitch: page_width * (1.0 - 2.0 * margin_h) / settings.lines_per_page as f32,
            pages: Vec::new(),
            current: Page::default(),
            used: 0,
            images: VecDeque::new(),
            centered: None,
            quote_open: false,
        }
    }

    fn chars(&self) -> f32 {
        self.settings.chars_per_line as f32
    }

    fn block(&mut self, block: &Block) {
        self.quote_open = false;
        let start = block.span.start;
        match &block.kind {
            BlockKind::PageBreak => {
                if self.used > 0 {
                    self.next_page();
                }
            }
            BlockKind::PageCenterStart => self.centered = Some(Vec::new()),
            BlockKind::PageCenterEnd => self.finish_centered(),
            BlockKind::Image { align, source } => {
                self.images.push_back(PendingImage { align: *align, source_name: source.clone(), source: start });
            }
            BlockKind::Rule => {
                let mut piece = Piece::new(' ', ' ', self.chars(), start);
                piece.lines.push(Rect {
                    x: -LINE_THICKNESS / 2.0,
                    y: 0.0,
                    width: LINE_THICKNESS,
                    height: self.chars(),
                });
                self.emit(Column { pieces: vec![piece], width: 1, y_offset: 0.0, source: start });
            }
            BlockKind::Horizontal { lines } => self.horizontal(lines, start),
            BlockKind::Paragraph => self.text_block(block, BODY, 1, 0.0, Alignment::Top),
            BlockKind::ListItem { depth } => self.text_block(block, BODY, 1, *depth as f32, Alignment::Top),
            BlockKind::Chapter => {
                let style = Style { scale: CHAPTER_SCALE, ..BODY };
                self.text_block(block, style, CHAPTER_COLUMNS, 0.0, Alignment::Top);
            }
            BlockKind::Strong => {
                self.text_block(block, Style { scale: STRONG_SCALE, ..BODY }, 1, 0.0, Alignment::Top);
            }
            BlockKind::Center => self.text_block(block, BODY, 1, 0.0, Alignment::Center),
            BlockKind::Under => self.text_block(block, BODY, 1, 0.0, Alignment::Bottom),
        }
    }

    fn text_block(&mut self, block: &Block, style: Style, width: usize, indent: f32, alignment: Alignment) {
        let mut lines = vec![Vec::new()];
        self.inlines(&block.inlines, style, &mut lines);
        let limit = self.chars() - indent;
        for pieces in lines {
            if pieces.is_empty() {
                self.emit(Column { pieces, width, y_offset: 0.0, source: block.span.start });
                continue;
            }
            let metrics: Vec<_> = pieces.iter().map(|p| (p.first, p.last, p.advance)).collect();
            let mut pieces = pieces.into_iter();
            let mut start = 0;
            for end in kinsoku::break_columns(&metrics, limit, false) {
                let column: Vec<Piece> = pieces.by_ref().take(end - start).collect();
                let length: f32 = column.iter().map(|p| p.advance).sum();
                let y_offset = indent
                    + match alignment {
                        Alignment::Top => 0.0,
                        Alignment::Center => (limit - length).max(0.0) / 2.0,
                        Alignment::Bottom => (limit - length).max(0.0),
                    };
                self.emit(Column { pieces: column, width, y_offset, source: block.span.start });
                start = end;
            }
        }
    }

    fn inlines(&mut self, inlines: &[Inline], style: Style, lines: &mut Vec<Vec<Piece>>) {
        for inline in inlines {
            match &inline.kind {
                InlineKind::Text(text) => {
                    let out = lines.last_mut().expect("常に1つ以上ある");
                    self.text(text, inline.span.start, style, out);
                }
                InlineKind::Ruby { base, ruby } => {
                    let piece = self.ruby(base, ruby, inline.span.start, style);
                    lines.last_mut().expect("常に1つ以上ある").push(piece);
                }
                InlineKind::Emphasis(children) => self.inlines(children, Style { emphasis: true, ..style }, lines),
                InlineKind::Sideline(children) => self.inlines(children, Style { sideline: true, ..style }, lines),
                InlineKind::Bold(children) => self.inlines(children, Style { sans: true, ..style }, lines),
                InlineKind::LineBreak => lines.push(Vec::new()),
            }
        }
    }

    fn font(&self, style: Style) -> &'a Font<'f> {
        if style.sans { self.sans } else { self.serif }
    }

    /// 表示時の自動変換を適用して文字列を要素に分ける。ソースとは1文字ずつ対応する。
    fn text(&mut self, text: &str, source: usize, style: Style, out: &mut Vec<Piece>) {
        let chars: Vec<char> = text
            .chars()
            .map(|c| match c {
                '“' => {
                    self.quote_open = true;
                    '〝'
                }
                '”' => {
                    self.quote_open = !self.quote_open;
                    if self.quote_open { '〝' } else { '〟' }
                }
                _ => c,
            })
            .collect();
        let first_piece = out.len();
        let mut plain_start = 0;
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            let special = if is_tcy_pair(c, chars.get(i + 1).copied()) {
                let pair: String = chars[i..i + 2].iter().collect();
                Some((self.tcy(&pair, source + i, style), 2))
            } else if c == '―' {
                Some((self.dash(source + i, style), 1))
            } else if is_wave(c) && chars[i..].iter().take(WAVE_CHARS).filter(|c| is_wave(**c)).count() == WAVE_CHARS {
                Some((self.wave(source + i, style), WAVE_CHARS))
            } else {
                None
            };
            let Some((piece, length)) = special else {
                i += 1;
                continue;
            };
            self.plain(&chars[plain_start..i], source + plain_start, style, out);
            out.push(piece);
            i += length;
            plain_start = i;
        }
        self.plain(&chars[plain_start..], source + plain_start, style, out);

        // 連続約物の字詰め
        for i in first_piece..out.len().saturating_sub(1) {
            if out[i].first == out[i + 1].first && matches!(out[i].first, '「' | '」' | '『' | '』') {
                out[i].advance /= 2.0;
            }
        }
    }

    fn plain(&self, chars: &[char], source: usize, style: Style, out: &mut Vec<Piece>) {
        if chars.is_empty() {
            return;
        }
        let text: String = chars.iter().collect();
        let shaped = self.font(style).shape(&text, style.scale);
        let mut char_index = 0;
        for (i, cluster) in shaped.iter().enumerate() {
            let byte_end = shaped.get(i + 1).map_or(text.len(), |c| c.byte_start);
            let content = &text[cluster.byte_start..byte_end];
            let first = content.chars().next().unwrap_or(' ');
            let last = content.chars().next_back().unwrap_or(first);
            let mut piece = Piece::new(first, last, cluster.advance, source + char_index);
            piece.glyphs.extend(cluster.glyphs.iter().map(|&(id, x, y)| PageGlyph {
                id,
                x,
                y,
                scale: style.scale,
                sans: style.sans,
            }));
            self.decorate(&mut piece, style);
            char_index += content.chars().count();
            out.push(piece);
        }
    }

    /// 傍点と傍線を付ける。
    fn decorate(&self, piece: &mut Piece, style: Style) {
        if style.emphasis && !piece.first.is_whitespace() {
            let size = style.scale * RUBY_SCALE;
            for cluster in self.serif.shape("﹅", size) {
                let top = (piece.advance - cluster.advance) / 2.0;
                piece.glyphs.extend(cluster.glyphs.iter().map(|&(id, x, y)| PageGlyph {
                    id,
                    x: style.scale * RUBY_CENTER + x,
                    y: top + y,
                    scale: size,
                    sans: false,
                }));
            }
        }
        if style.sideline {
            piece.lines.push(Rect {
                x: style.scale * 0.6,
                y: 0.0,
                width: style.scale * LINE_THICKNESS,
                height: piece.advance,
            });
        }
    }

    fn ruby(&self, base: &str, ruby: &str, source: usize, style: Style) -> Piece {
        let first = base.chars().next().unwrap_or(' ');
        let last = base.chars().next_back().unwrap_or(first);
        let mut piece = Piece::new(first, last, 0.0, source);
        for cluster in self.font(style).shape(base, style.scale) {
            piece.glyphs.extend(cluster.glyphs.iter().map(|&(id, x, y)| PageGlyph {
                id,
                x,
                y: piece.advance + y,
                scale: style.scale,
                sans: style.sans,
            }));
            piece.advance += cluster.advance;
        }
        let size = style.scale * RUBY_SCALE;
        let shaped = self.serif.shape(ruby, size);
        let length: f32 = shaped.iter().map(|c| c.advance).sum();
        let mut pen = (piece.advance - length) / 2.0;
        for cluster in shaped {
            piece.glyphs.extend(cluster.glyphs.iter().map(|&(id, x, y)| PageGlyph {
                id,
                x: style.scale * RUBY_CENTER + x,
                y: pen + y,
                scale: size,
                sans: false,
            }));
            pen += cluster.advance;
        }
        if style.sideline {
            self.decorate(&mut piece, Style { emphasis: false, ..style });
        }
        piece
    }

    /// 縦中横。字面に収まるよう縮める。
    fn tcy(&self, text: &str, source: usize, style: Style) -> Piece {
        let first = text.chars().next().unwrap_or(' ');
        let last = text.chars().next_back().unwrap_or(first);
        let mut piece = Piece::new(first, last, style.scale, source);
        let (glyphs, width) = self.font(style).shape_line(text, style.scale);
        let fit = (style.scale / width).min(1.0);
        let baseline = style.scale / 2.0 + TCY_BASELINE * style.scale * fit;
        piece.glyphs.extend(glyphs.iter().map(|&(id, x, y)| PageGlyph {
            id,
            x: (x - width / 2.0) * fit,
            y: baseline + y * fit,
            scale: style.scale * fit,
            sans: style.sans,
        }));
        self.decorate(&mut piece, Style { emphasis: false, ..style });
        piece
    }

    fn dash(&self, source: usize, style: Style) -> Piece {
        let mut piece = Piece::new('―', '―', style.scale, source);
        piece.lines.push(Rect {
            x: -style.scale * LINE_THICKNESS / 2.0,
            y: 0.0,
            width: style.scale * LINE_THICKNESS,
            height: style.scale,
        });
        piece
    }

    fn wave(&self, source: usize, style: Style) -> Piece {
        let length = style.scale * WAVE_CHARS as f32;
        let mut piece = Piece::new('〜', '〜', length, source);
        piece.waves.push(Rect {
            x: -style.scale * 0.1,
            y: 0.0,
            width: style.scale * 0.2,
            height: length,
        });
        piece
    }

    // ponytail: 版面の幅・天地を超える分は折り返さず切れる。折り返しが要るコードや表が出てきたら対応
    fn horizontal(&mut self, lines: &[String], source: usize) {
        let shaped: Vec<_> = lines.iter().map(|line| self.serif.shape_line(line, HORIZONTAL_SCALE)).collect();
        let widest = shaped.iter().map(|(_, width)| *width).fold(0.0, f32::max);
        let width = ((widest / self.pitch).ceil() as usize).clamp(1, self.settings.lines_per_page);
        let block_width = width as f32 * self.pitch;
        let line_height = HORIZONTAL_SCALE * HORIZONTAL_LINE_HEIGHT;
        let mut piece = Piece::new(' ', ' ', 0.0, source);
        for (glyphs, _) in shaped.iter().take((self.chars() / line_height) as usize) {
            piece.advance += line_height;
            let baseline = piece.advance;
            piece.glyphs.extend(glyphs.iter().map(|&(id, x, y)| PageGlyph {
                id,
                x: x - block_width / 2.0,
                y: baseline + y,
                scale: HORIZONTAL_SCALE,
                sans: false,
            }));
        }
        self.emit(Column { pieces: vec![piece], width, y_offset: 0.0, source });
    }

    fn emit(&mut self, column: Column) {
        match &mut self.centered {
            Some(columns) => columns.push(column),
            None => self.place(column),
        }
    }

    fn place(&mut self, column: Column) {
        if self.used > 0 && self.used + column.width > self.settings.lines_per_page {
            self.next_page();
        }
        let center = self.text_right - (self.used as f32 + column.width as f32 / 2.0) * self.pitch;
        let top = self.margin_top + column.y_offset;
        if column.pieces.is_empty() {
            self.current.stops.push(Stop {
                source: column.source,
                rect: Rect { x: center - 0.5, y: top, width: 1.0, height: 1.0 },
            });
        }
        let mut pen = top;
        for piece in column.pieces {
            self.current.stops.push(Stop {
                source: piece.source,
                rect: Rect { x: center - 0.5, y: pen, width: 1.0, height: piece.advance },
            });
            self.current.glyphs.extend(piece.glyphs.iter().map(|g| PageGlyph { x: center + g.x, y: pen + g.y, ..*g }));
            let offset = |r: &Rect| Rect { x: center + r.x, y: pen + r.y, ..*r };
            self.current.lines.extend(piece.lines.iter().map(offset));
            self.current.waves.extend(piece.waves.iter().map(offset));
            pen += piece.advance;
        }
        self.used += column.width;
    }

    fn finish_centered(&mut self) {
        let Some(columns) = self.centered.take() else { return };
        let lines = self.settings.lines_per_page;
        let total: usize = columns.iter().map(|c| c.width).sum();
        if total > lines {
            // 1ページに収まらなければ通常どおり流す
            columns.into_iter().for_each(|column| self.place(column));
            return;
        }
        if self.used > 0 {
            self.next_page();
        }
        let longest = columns
            .iter()
            .map(|c| c.pieces.iter().map(|p| p.advance).sum::<f32>())
            .fold(0.0, f32::max);
        let y_offset = (self.chars() - longest).max(0.0) / 2.0;
        self.used = (lines - total) / 2;
        for column in columns {
            self.place(Column { y_offset, ..column });
        }
        self.next_page();
    }

    fn next_page(&mut self) {
        self.pages.push(std::mem::take(&mut self.current));
        self.used = 0;
        self.place_ready_images();
    }

    /// 先頭の挿絵から順に、次のページに置けるものを置く。追い越しはしない。
    fn place_ready_images(&mut self) {
        while let Some(image) = self.images.front() {
            let is_left = self.settings.spread.is_left(self.pages.len());
            let ready = match image.align {
                ImageAlign::Next => true,
                ImageAlign::Left => is_left,
                ImageAlign::Right => !is_left,
            };
            if !ready {
                return;
            }
            let image = self.images.pop_front().expect("直前に確認済み");
            self.pages.push(Page {
                image: Some(image.source_name),
                stops: vec![Stop {
                    source: image.source,
                    rect: Rect { x: 0.0, y: 0.0, width: self.page_width, height: self.page_height },
                }],
                ..Page::default()
            });
        }
    }

    fn finish(mut self) -> PagedDocument {
        // 閉じていないページセンタリングは通常の本文として流す
        if let Some(columns) = self.centered.take() {
            columns.into_iter().for_each(|column| self.place(column));
        }
        if self.used > 0 || self.pages.is_empty() {
            self.next_page();
        } else {
            self.place_ready_images();
        }
        while !self.images.is_empty() {
            self.pages.push(Page::default());
            self.place_ready_images();
        }
        PagedDocument {
            pages: self.pages,
            page_width: self.page_width,
            page_height: self.page_height,
            spread: self.settings.spread,
        }
    }
}

#[derive(Clone, Copy)]
enum Alignment {
    Top,
    Center,
    Bottom,
}

/// 縦中横にする感嘆符の組。
fn is_tcy_pair(first: char, second: Option<char>) -> bool {
    matches!(
        (first, second),
        ('!', Some('?' | '!')) | ('?', Some('!')) | ('！', Some('？' | '！')) | ('？', Some('！'))
    )
}

fn is_wave(c: char) -> bool {
    matches!(c, '〜' | '～')
}

#[cfg(test)]
mod tests {
    use super::*;
    use kakikotoba_markup::highlight::{Dialect, Options};

    const SETTINGS: PageSettings = PageSettings {
        format: PageFormat::A6,
        chars_per_line: 5,
        lines_per_page: 3,
        spread: SpreadMode::LeftStart,
    };

    fn with_fonts(test: impl FnOnce(&Font)) {
        let path = "/usr/share/fonts/noto-cjk/NotoSerifCJK-Regular.ttc";
        let Ok(data) = std::fs::read(path) else {
            eprintln!("スキップ: {path} がない");
            return;
        };
        test(&Font::new(&data, 0).expect("フォントを読めない"));
    }

    fn paged(text: &str, font: &Font, settings: PageSettings) -> PagedDocument {
        let options = Options { dialect: Dialect::PreTnf, symbol_forms: true };
        PagedDocument::new(&kakikotoba_markup::parse(text, options), font, font, settings)
    }

    /// ページごとの元ソース位置。
    fn sources(document: &PagedDocument) -> Vec<Vec<usize>> {
        document.pages.iter().map(|page| page.stops.iter().map(|s| s.source).collect()).collect()
    }

    #[test]
    fn columns_fill_pages_with_kinsoku() {
        with_fonts(|font| {
            // 5字×3行。「。」は行頭に来ないので追い出しになる
            let d = paged("あいうえお。かきくけこさしすせそたちつ", font, SETTINGS);
            assert_eq!(d.pages.len(), 2);
            assert_eq!(d.pages[0].stops.len(), 14);
            assert_eq!(d.pages[1].stops.len(), 5);

            // 1行目は右端、下へ進み、次の行は左へ
            let stops = &d.pages[0].stops;
            assert_eq!(stops[1].rect.x, stops[0].rect.x);
            assert!((stops[1].rect.y - stops[0].rect.y - 1.0).abs() < 1e-3);
            assert!(stops[4].rect.x < stops[3].rect.x);
            assert!(stops[0].rect.x + stops[0].rect.width < d.page_width);
        });
    }

    #[test]
    fn page_shape_follows_format() {
        with_fonts(|font| {
            let d = paged("あ", font, SETTINGS);
            assert!((d.page_height / d.page_width - PAGE_ASPECT).abs() < 1e-4);
            // 版面の天地が字数と一致する
            let margin = d.page_height * 0.09;
            assert!((d.page_height - 2.0 * margin - 5.0).abs() < 1e-3);
        });
    }

    #[test]
    fn empty_line_takes_a_column_and_page_break_is_ignored_on_empty_page() {
        with_fonts(|font| {
            let d = paged(":\nあ\n\nい\n:\n:\nう", font, SETTINGS);
            assert_eq!(sources(&d), [vec![2, 4, 5], vec![11]]);
        });
    }

    #[test]
    fn ruby_stays_on_one_column_and_is_beside_base() {
        with_fonts(|font| {
            let d = paged("あいう[漢字|かんじ]", font, SETTINGS);
            // ルビは分割されず、次の行へ送られない限り1つの対応点
            assert_eq!(sources(&d), [vec![0, 1, 2, 3]]);
            let glyphs = &d.pages[0].glyphs;
            // 親字2 + ルビ3 + あいう3
            assert_eq!(glyphs.len(), 8);
            let ruby: Vec<_> = glyphs.iter().filter(|g| g.scale == RUBY_SCALE).collect();
            assert_eq!(ruby.len(), 3);
            let base = glyphs[3];
            assert!(ruby[0].x > base.x);
        });
    }

    #[test]
    fn emphasis_and_sideline_add_marks() {
        with_fonts(|font| {
            let d = paged("\"傍点\"'線'", font, SETTINGS);
            let page = &d.pages[0];
            assert_eq!(page.glyphs.iter().filter(|g| g.scale == RUBY_SCALE).count(), 2);
            assert_eq!(page.lines.len(), 1);
            // 記号は表示されず、元ソースの位置を保つ
            let source: Vec<_> = page.stops.iter().map(|s| s.source).collect();
            assert_eq!(source, [1, 2, 5]);
        });
    }

    #[test]
    fn automatic_conversions_are_display_only_and_keep_source_positions() {
        with_fonts(|font| {
            // 感嘆符2字は1字分、ダッシュは罫線、波線は4字ごと
            let d = paged("あ！？い――う〜〜〜〜〜え", font, PageSettings { chars_per_line: 20, ..SETTINGS });
            let page = &d.pages[0];
            let source: Vec<_> = page.stops.iter().map(|s| s.source).collect();
            assert_eq!(source, [0, 1, 3, 4, 5, 6, 7, 11, 12]);
            assert_eq!(page.lines.len(), 2);
            assert_eq!(page.waves.len(), 1);
            // 縦中横は字面に収まる
            let tcy: Vec<_> = page.glyphs.iter().filter(|g| g.scale < 1.0).collect();
            assert_eq!(tcy.len(), 2);
            assert!((page.stops[1].rect.height - 1.0).abs() < 1e-3);
            assert!((page.waves[0].height - 4.0).abs() < 1e-3);
        });
    }

    #[test]
    fn quotes_and_repeated_brackets() {
        with_fonts(|font| {
            let settings = PageSettings { chars_per_line: 20, ..SETTINGS };
            let shaped = |text: &str| paged(text, font, settings).pages.remove(0);
            let quoted = shaped("”あ”");
            let converted = shaped("〝あ〟");
            let ids = |page: &Page| page.glyphs.iter().map(|g| g.id).collect::<Vec<_>>();
            assert_eq!(ids(&quoted), ids(&converted));

            let tracked = shaped("「「あ」」");
            let heights: Vec<f32> = tracked.stops.iter().map(|s| s.rect.height).collect();
            assert!((heights[0] - 0.5).abs() < 1e-3 && (heights[1] - 1.0).abs() < 1e-3);
            assert!((heights[3] - 0.5).abs() < 1e-3 && (heights[4] - 1.0).abs() < 1e-3);
        });
    }

    fn image_order(document: &PagedDocument) -> Vec<Option<&str>> {
        document.pages.iter().map(|p| p.image.as_deref()).collect()
    }

    #[test]
    fn images_wait_for_their_side_without_overtaking() {
        with_fonts(|font| {
            // 左ページ始まり: 0始まりの偶数ページが左
            let text = "#IMG:L# l.jpg\n#IMG:R# r.jpg\nあ\n:\nい\n:\nう";
            let d = paged(text, font, SETTINGS);
            // あ(0左) → 次は1(右)なのでLは待つ → い(1右) → 次は2(左)でl、3(右)でr → う(4)
            assert_eq!(image_order(&d), [None, None, Some("l.jpg"), Some("r.jpg"), None]);

            let spread = PageSettings { spread: SpreadMode::FullSpread, ..SETTINGS };
            let d = paged(text, font, spread);
            // 最初から見開き: あ(0右) → 次は1(左)でl、2(右)でr → い(3左) → う(4)
            assert_eq!(image_order(&d), [None, Some("l.jpg"), Some("r.jpg"), None, None]);
        });
    }

    #[test]
    fn remaining_images_are_placed_at_end_with_blank_pages() {
        with_fonts(|font| {
            // あ(0左)の次は右。Lを置くには空白ページが要る
            let d = paged("#IMG:L# l.jpg\nあ", font, SETTINGS);
            assert_eq!(image_order(&d), [None, None, Some("l.jpg")]);
            assert!(d.pages[1].stops.is_empty());

            let d = paged("#IMG:N# n.jpg\nあ", font, SETTINGS);
            assert_eq!(image_order(&d), [None, Some("n.jpg")]);
        });
    }

    #[test]
    fn spreads_follow_the_mode() {
        with_fonts(|font| {
            let text = "あ\n:\nい\n:\nう\n:\nえ";
            let d = paged(text, font, SETTINGS);
            assert_eq!(d.spread_count(), 3);
            assert_eq!(d.spread_pages(0), (None, Some(0)));
            assert_eq!(d.spread_pages(1), (Some(1), Some(2)));
            assert_eq!(d.spread_pages(2), (Some(3), None));
            assert_eq!(d.spread_of_page(0), 0);
            assert_eq!(d.spread_of_page(2), 1);
            assert_eq!(d.spread_of_page(3), 2);

            let d = paged(text, font, PageSettings { spread: SpreadMode::FullSpread, ..SETTINGS });
            assert_eq!(d.spread_count(), 2);
            assert_eq!(d.spread_pages(0), (Some(0), Some(1)));
            assert_eq!(d.spread_pages(1), (Some(2), Some(3)));
            assert_eq!(d.spread_of_page(3), 1);
        });
    }

    #[test]
    fn source_and_page_map_both_ways() {
        with_fonts(|font| {
            let d = paged("あいう\n:\nかきく", font, SETTINGS);
            assert_eq!(d.page_of_source(1), 0);
            assert_eq!(d.page_of_source(6), 1);
            // 改ページの行は直前の文字のページ
            assert_eq!(d.page_of_source(4), 0);

            let stop = &d.pages[1].stops[2];
            let hit = d.hit_test(1, stop.rect.x + 0.5, stop.rect.y + 0.5);
            assert_eq!(hit, Some(stop.source));
            assert_eq!(d.hit_test(9, 0.0, 0.0), None);
        });
    }

    #[test]
    fn page_center_and_alignment() {
        with_fonts(|font| {
            let d = paged("あ\n{\nか\n}\nさ", font, SETTINGS);
            assert_eq!(sources(&d), [vec![0], vec![4], vec![8]]);
            let centered = &d.pages[1].stops[0].rect;
            // 3行の中央の行。天地も中央へ寄る
            let first_line_x = d.pages[0].stops[0].rect.x;
            assert!(centered.x < first_line_x);
            assert!(centered.y > d.pages[0].stops[0].rect.y);

            let d = paged("#地付き# あ\n#中央# あ\nあ", font, SETTINGS);
            let ys: Vec<f32> = d.pages[0].stops.iter().map(|s| s.rect.y).collect();
            assert!(ys[0] > ys[1] && ys[1] > ys[2]);
            assert!((ys[0] - ys[2] - 4.0).abs() < 1e-3);
        });
    }

    #[test]
    fn chapter_is_large_and_wide() {
        with_fonts(|font| {
            let d = paged("#章# 一\nあ", font, SETTINGS);
            let page = &d.pages[0];
            assert_eq!(page.glyphs[0].scale, CHAPTER_SCALE);
            // 見出しが2行分を占めるので、本文は3行目
            let (heading, body) = (page.stops[0].rect.x, page.stops[1].rect.x);
            let pitch = (heading - body) / 1.5;
            assert!(pitch > 1.0);
        });
    }

    #[test]
    fn markdown_blocks_flow() {
        with_fonts(|font| {
            let options = Options { dialect: Dialect::Markdown, symbol_forms: true };
            let text = "本文\n\n---\n\n- 項目\n\n```\ncode\n```\n";
            let settings = PageSettings { lines_per_page: 10, ..SETTINGS };
            let d = PagedDocument::new(&kakikotoba_markup::parse(text, options), font, font, settings);
            let page = &d.pages[0];
            assert_eq!(page.lines.len(), 1);
            assert!((page.lines[0].height - 5.0).abs() < 1e-3);
            // 字下げ
            let item = page.stops.iter().find(|s| s.source == 11).expect("項目の文字");
            assert!((item.rect.y - page.stops[0].rect.y - 1.0).abs() < 1e-3);
            // 横組みブロックは横に並ぶ
            let code: Vec<_> = page.glyphs.iter().filter(|g| g.scale == HORIZONTAL_SCALE).collect();
            assert_eq!(code.len(), 4);
            assert!(code[1].x > code[0].x && code[1].y == code[0].y);
        });
    }

    #[test]
    fn empty_document_has_one_page() {
        with_fonts(|font| {
            let d = paged("", font, SETTINGS);
            assert_eq!(d.pages.len(), 1);
            assert_eq!(d.spread_count(), 1);
            assert_eq!(d.page_of_source(0), 0);
        });
    }
}

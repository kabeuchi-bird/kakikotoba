//! 縦組みレイアウト。入力: テキスト・設定 / 出力: グリフ配置列・カーソル矩形
//!
//! 座標は画面座標系（yは下向き）で、原点は先頭列の右上。列は左へ進むので、xは常に負になる。
//! 文字位置はすべて文字（char）単位で、`ropey` の位置と一致する。

mod kinsoku;

use std::ops::Range;

use rustybuzz::ttf_parser::Tag;
use rustybuzz::{Direction, Face, Feature, Language, UnicodeBuffer, script};

pub struct Font<'a> {
    face: Face<'a>,
    features: [Feature; 3],
    language: Language,
}

impl<'a> Font<'a> {
    /// `index` はフォントコレクション（.ttc）内の番号。不正なフォントなら `None`。
    pub fn new(data: &'a [u8], index: u32) -> Option<Self> {
        Some(Self {
            face: Face::from_slice(data, index)?,
            features: [b"vert", b"vrt2", b"vkna"].map(|tag| Feature::new(Tag::from_bytes(tag), 1, ..)),
            language: "ja".parse().ok()?,
        })
    }

    fn shape(&self, text: &str, font_size: f32) -> Vec<ShapedCluster> {
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.set_direction(Direction::TopToBottom);
        // 段落先頭の文字種で字形が変わらないよう固定する
        buffer.set_script(script::HAN);
        buffer.set_language(self.language.clone());
        let shaped = rustybuzz::shape(&self.face, &self.features, buffer);

        let scale = font_size / self.face.units_per_em() as f32;
        let mut clusters: Vec<ShapedCluster> = Vec::new();
        for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
            let byte_start = info.cluster as usize;
            if clusters.last().is_none_or(|c| c.byte_start != byte_start) {
                clusters.push(ShapedCluster {
                    byte_start,
                    advance: 0.0,
                    glyphs: Vec::new(),
                });
            }
            let cluster = clusters.last_mut().expect("直前で追加済み");
            cluster.glyphs.push((
                info.glyph_id as u16,
                pos.x_offset as f32 * scale,
                cluster.advance - pos.y_offset as f32 * scale,
            ));
            cluster.advance -= pos.y_advance as f32 * scale;
        }
        clusters
    }
}

struct ShapedCluster {
    byte_start: usize,
    advance: f32,
    /// グリフ番号と、列の中心・クラスタ先頭からの相対位置
    glyphs: Vec<(u16, f32, f32)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub font_size: f32,
    pub chars_per_column: usize,
    /// 列の送り（列の中心から次の列の中心まで）
    pub column_pitch: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// 描画用のグリフ。`(x, y)` はベースライン原点で、`drawGlyphRun` の位置にそのまま渡せる。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub id: u16,
    pub x: f32,
    pub y: f32,
    pub char_index: usize,
}

/// カーソルが止まれる位置。各段落の末尾には送り量0の要素（改行または文書末）を置く。
struct Stop {
    char_index: usize,
    column: usize,
    y: f32,
    advance: f32,
}

pub struct Layout {
    pub glyphs: Vec<Glyph>,
    settings: Settings,
    stops: Vec<Stop>,
    /// 各列に属する `stops` の範囲
    columns: Vec<Range<usize>>,
}

impl Layout {
    // ponytail: 編集のたびに全文を組み直す（20万字で約50ms）。入力が重ければ段落単位のキャッシュへ
    pub fn new(text: &str, font: &Font, settings: Settings) -> Self {
        let mut layout = Self {
            glyphs: Vec::new(),
            settings,
            stops: Vec::new(),
            columns: Vec::new(),
        };
        let mut char_index = 0;
        for line in text.split('\n') {
            layout.push_paragraph(line.strip_suffix('\r').unwrap_or(line), char_index, font);
            char_index += line.chars().count() + 1;
        }
        layout
    }

    fn push_paragraph(&mut self, text: &str, first_char: usize, font: &Font) {
        let Settings { font_size, chars_per_column, .. } = self.settings;
        let shaped = font.shape(text, font_size);

        let byte_end = |i: usize| shaped.get(i + 1).map_or(text.len(), |c| c.byte_start);
        let metrics: Vec<_> = shaped
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let source = &text[c.byte_start..byte_end(i)];
                let first = source.chars().next().unwrap_or(' ');
                (first, source.chars().next_back().unwrap_or(first), c.advance)
            })
            .collect();
        let ends = kinsoku::break_columns(&metrics, chars_per_column as f32 * font_size);

        let mut char_index = first_char;
        let mut y = 0.0;
        let mut start = 0;
        for end in ends {
            let column = self.columns.len();
            let center = self.column_center(column);
            let first_stop = self.stops.len();
            y = 0.0;
            for (i, cluster) in shaped.iter().enumerate().take(end).skip(start) {
                self.stops.push(Stop { char_index, column, y, advance: cluster.advance });
                for &(id, dx, dy) in &cluster.glyphs {
                    self.glyphs.push(Glyph { id, x: center + dx, y: y + dy, char_index });
                }
                y += cluster.advance;
                char_index += text[cluster.byte_start..byte_end(i)].chars().count();
            }
            self.columns.push(first_stop..self.stops.len());
            start = end;
        }
        // 空段落も1列を占める
        if shaped.is_empty() {
            self.columns.push(self.stops.len()..self.stops.len());
        }
        let column = self.columns.len() - 1;
        self.stops.push(Stop { char_index, column, y, advance: 0.0 });
        self.columns[column].end = self.stops.len();
    }

    fn column_center(&self, column: usize) -> f32 {
        -(column as f32 + 0.5) * self.settings.column_pitch
    }

    fn stop_index(&self, char_index: usize) -> usize {
        self.stops.partition_point(|s| s.char_index <= char_index).saturating_sub(1)
    }

    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// 文字位置の字面1字分の矩形。描画用カーソルとIME向け矩形はここから作る。
    pub fn cursor_rect(&self, char_index: usize) -> Rect {
        let stop = &self.stops[self.stop_index(char_index)];
        let size = self.settings.font_size;
        Rect {
            x: self.column_center(stop.column) - size / 2.0,
            y: stop.y,
            width: size,
            height: size,
        }
    }

    /// ↓キー: 次の文字へ。文書末ならそのまま。
    pub fn next_char(&self, char_index: usize) -> usize {
        self.stops.get(self.stop_index(char_index) + 1).map_or(char_index, |s| s.char_index)
    }

    /// ↑キー: 前の文字へ。文書先頭ならそのまま。
    pub fn prev_char(&self, char_index: usize) -> usize {
        let i = self.stop_index(char_index);
        if self.stops[i].char_index < char_index {
            return self.stops[i].char_index;
        }
        i.checked_sub(1).map_or(char_index, |i| self.stops[i].char_index)
    }

    /// ←→キー: `delta` 列先（正で左へ）の、天からの距離 `y` に最も近い位置へ。
    pub fn move_column(&self, char_index: usize, delta: isize, y: f32) -> usize {
        let column = self.stops[self.stop_index(char_index)].column;
        let last = self.columns.len() - 1;
        self.char_in_column(column.saturating_add_signed(delta).min(last), y)
    }

    /// 座標に最も近い文字位置を返す。
    pub fn hit_test(&self, x: f32, y: f32) -> usize {
        let column = (-x / self.settings.column_pitch).max(0.0) as usize;
        self.char_in_column(column.min(self.columns.len() - 1), y)
    }

    fn char_in_column(&self, column: usize, y: f32) -> usize {
        let stops = &self.stops[self.columns[column].clone()];
        // 列末を越えても次の列へ飛ばず、この列の最後の位置に留める
        let i = stops.partition_point(|s| s.y + s.advance / 2.0 <= y).min(stops.len() - 1);
        stops[i].char_index
    }

    /// 選択範囲を列ごとの矩形にして返す。
    pub fn selection_rects(&self, range: Range<usize>) -> Vec<Rect> {
        let size = self.settings.font_size;
        let mut rects: Vec<(usize, Rect)> = Vec::new();
        let first = self.stop_index(range.start);
        for stop in self.stops[first..].iter().take_while(|s| s.char_index < range.end) {
            match rects.last_mut() {
                Some((column, rect)) if *column == stop.column => {
                    rect.height = stop.y + stop.advance - rect.y;
                }
                _ => rects.push((stop.column, Rect {
                    x: self.column_center(stop.column) - size / 2.0,
                    y: stop.y,
                    width: size,
                    height: stop.advance,
                })),
            }
        }
        rects.into_iter().map(|(_, rect)| rect).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS: Settings = Settings { font_size: 10.0, chars_per_column: 5, column_pitch: 16.0 };

    /// 実フォントが必要な検証。無い環境ではスキップする。
    fn with_font(test: impl FnOnce(&Font)) {
        let path = "/usr/share/fonts/noto-cjk/NotoSerifCJK-Regular.ttc";
        let Ok(data) = std::fs::read(path) else {
            eprintln!("スキップ: {path} がない");
            return;
        };
        test(&Font::new(&data, 0).expect("フォントを読めない"));
    }

    fn column_text(layout: &Layout, text: &str, column: usize) -> String {
        let chars: Vec<char> = text.chars().collect();
        layout.stops[layout.columns[column].clone()]
            .iter()
            .filter_map(|s| chars.get(s.char_index))
            .filter(|c| **c != '\n')
            .collect()
    }

    #[test]
    fn vert_substitutes_punctuation_and_centers_on_column() {
        with_font(|font| {
            let layout = Layout::new("。", font, SETTINGS);
            let mut horizontal = UnicodeBuffer::new();
            horizontal.push_str("。");
            let horizontal = rustybuzz::shape(&font.face, &[], horizontal);
            assert_ne!(layout.glyphs[0].id as u32, horizontal.glyph_infos()[0].glyph_id);

            // 全角字は字面の左端が列の中心から半字分左、ベースラインは字面の内側
            let g = layout.glyphs[0];
            assert_eq!(g.x, -8.0 - 5.0);
            assert!(g.y > 0.0 && g.y < 10.0, "y = {}", g.y);
        });
    }

    #[test]
    fn wraps_with_kinsoku_and_keeps_newlines_as_columns() {
        with_font(|font| {
            let text = "あいうえお。か\n\nき";
            let layout = Layout::new(text, font, SETTINGS);
            assert_eq!(layout.column_count(), 4);
            assert_eq!(column_text(&layout, text, 0), "あいうえ");
            assert_eq!(column_text(&layout, text, 1), "お。か");
            assert_eq!(column_text(&layout, text, 2), "");
            assert_eq!(column_text(&layout, text, 3), "き");
        });
    }

    #[test]
    fn cursor_rect_follows_columns() {
        with_font(|font| {
            let layout = Layout::new("あい\nう", font, SETTINGS);
            assert_eq!(layout.cursor_rect(0), Rect { x: -13.0, y: 0.0, width: 10.0, height: 10.0 });
            assert_eq!(layout.cursor_rect(1).y, 10.0);
            // 改行の位置は行末
            assert_eq!(layout.cursor_rect(2).y, 20.0);
            assert_eq!(layout.cursor_rect(3), Rect { x: -29.0, y: 0.0, width: 10.0, height: 10.0 });
            // 文書末
            assert_eq!(layout.cursor_rect(4), Rect { x: -29.0, y: 10.0, width: 10.0, height: 10.0 });
        });
    }

    #[test]
    fn up_down_walk_every_position_including_crlf() {
        with_font(|font| {
            let layout = Layout::new("あ\r\nい", font, SETTINGS);
            // あ(0) 改行(1,2) い(3) 文書末(4)
            let seen = std::iter::successors(Some(0), |&i| Some(layout.next_char(i)).filter(|&n| n != i));
            assert_eq!(seen.collect::<Vec<_>>(), [0, 1, 3, 4]);
            assert_eq!(layout.prev_char(3), 1);
            assert_eq!(layout.prev_char(2), 1);
            assert_eq!(layout.prev_char(0), 0);
        });
    }

    #[test]
    fn left_right_keep_height_and_clamp() {
        with_font(|font| {
            let layout = Layout::new("あいう\nか\nさしす", font, SETTINGS);
            // 2列目は短いので行末に寄り、3列目で元の高さへ戻る
            assert_eq!(layout.move_column(2, 1, 20.0), 5);
            assert_eq!(layout.move_column(5, 1, 20.0), 8);
            assert_eq!(layout.move_column(8, -2, 20.0), 2);
            // 両端で止まる
            assert_eq!(layout.move_column(0, -1, 0.0), 0);
            assert_eq!(layout.move_column(8, 5, 20.0), 8);
        });
    }

    #[test]
    fn hit_test_finds_nearest_boundary() {
        with_font(|font| {
            let layout = Layout::new("あいう\nか", font, SETTINGS);
            assert_eq!(layout.hit_test(-8.0, 12.0), 1);
            assert_eq!(layout.hit_test(-8.0, 16.0), 2);
            assert_eq!(layout.hit_test(-24.0, 0.0), 4);
            // 範囲外は端に寄せる
            assert_eq!(layout.hit_test(100.0, -5.0), 0);
            assert_eq!(layout.hit_test(-999.0, 999.0), 5);
        });
    }

    #[test]
    fn selection_spans_columns() {
        with_font(|font| {
            let layout = Layout::new("あいうえおかき", font, SETTINGS);
            // あいうえお / かき の いうえおか を選択
            let rects = layout.selection_rects(1..6);
            assert_eq!(rects, [
                Rect { x: -13.0, y: 10.0, width: 10.0, height: 40.0 },
                Rect { x: -29.0, y: 0.0, width: 10.0, height: 10.0 },
            ]);
            assert!(layout.selection_rects(2..2).is_empty());
        });
    }

    #[test]
    fn empty_text_has_one_position() {
        with_font(|font| {
            let layout = Layout::new("", font, SETTINGS);
            assert_eq!(layout.column_count(), 1);
            assert_eq!(layout.cursor_rect(0).y, 0.0);
            assert_eq!(layout.next_char(0), 0);
            assert_eq!(layout.hit_test(-50.0, 50.0), 0);
        });
    }
}

//! 縦組みレイアウト。入力: テキスト・設定 / 出力: グリフ配置列・カーソル矩形
//!
//! 座標は画面座標系（yは下向き）で、原点は先頭列の右上。列は左へ進むので、xは常に負になる。
//! 文字位置はすべて文字（char）単位で、`ropey` の位置と一致する。

mod kinsoku;
pub mod page;
mod segment;

use std::ops::Range;

use rustybuzz::ttf_parser::Tag;
use rustybuzz::{Direction, Face, Feature, Language, UnicodeBuffer, script};

use segment::Orientation;

/// 縦中横の文字列を字面に収めるときのベースライン位置（字サイズ比）
const TCY_BASELINE: f32 = 0.35;
/// 90°回転した欧文のベースラインの、列の中心からの距離（字サイズ比）。仮想ボディの中心に揃える
const ROTATED_BASELINE: f32 = 0.38;

pub struct Font<'a> {
    face: Face<'a>,
    features: [Feature; 3],
    /// 回転する欧文でも文字ごとにカーソルが止まるよう、合字を作らない
    rotated_features: [Feature; 2],
    language: Language,
}

impl<'a> Font<'a> {
    /// `index` はフォントコレクション（.ttc）内の番号。不正なフォントなら `None`。
    pub fn new(data: &'a [u8], index: u32) -> Option<Self> {
        Some(Self {
            face: Face::from_slice(data, index)?,
            features: [b"vert", b"vrt2", b"vkna"].map(|tag| Feature::new(Tag::from_bytes(tag), 1, ..)),
            rotated_features: [b"liga", b"clig"].map(|tag| Feature::new(Tag::from_bytes(tag), 0, ..)),
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
                    scale: 1.0,
                    rotated: false,
                    stops: 1,
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

    /// 編集モード用。文字の向きを判定しながら組む。
    fn shape_oriented(&self, text: &str, font_size: f32) -> Vec<ShapedCluster> {
        let mut clusters = Vec::new();
        for (orientation, range) in segment::segments(text) {
            let part = &text[range.clone()];
            let shaped = match orientation {
                Orientation::Upright => self.shape(part, font_size),
                Orientation::Tcy => vec![self.shape_tcy(part, font_size)],
                Orientation::Rotated => self.shape_rotated(part, font_size),
            };
            clusters.extend(shaped.into_iter().map(|cluster| ShapedCluster {
                byte_start: range.start + cluster.byte_start,
                ..cluster
            }));
        }
        clusters
    }

    /// 縦中横。全体を1字分に収める。
    fn shape_tcy(&self, text: &str, font_size: f32) -> ShapedCluster {
        let (glyphs, width) = self.shape_line(text, font_size);
        let fit = (font_size / width).min(1.0);
        let baseline = font_size * (0.5 + TCY_BASELINE * fit);
        ShapedCluster {
            byte_start: 0,
            advance: font_size,
            glyphs: glyphs.iter().map(|&(id, x, y)| (id, (x - width / 2.0) * fit, baseline + y * fit)).collect(),
            scale: fit,
            rotated: false,
            stops: text.chars().count(),
        }
    }

    /// 90°回転。横組みの右方向が画面の下方向になる。
    fn shape_rotated(&self, text: &str, font_size: f32) -> Vec<ShapedCluster> {
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.set_direction(Direction::LeftToRight);
        let shaped = rustybuzz::shape(&self.face, &self.rotated_features, buffer);

        let scale = font_size / self.face.units_per_em() as f32;
        let baseline = -ROTATED_BASELINE * font_size;
        let mut clusters: Vec<ShapedCluster> = Vec::new();
        for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
            let byte_start = info.cluster as usize;
            if clusters.last().is_none_or(|c| c.byte_start != byte_start) {
                clusters.push(ShapedCluster {
                    byte_start,
                    advance: 0.0,
                    glyphs: Vec::new(),
                    scale: 1.0,
                    rotated: true,
                    stops: 1,
                });
            }
            let cluster = clusters.last_mut().expect("直前で追加済み");
            cluster.glyphs.push((
                info.glyph_id as u16,
                baseline + pos.y_offset as f32 * scale,
                cluster.advance + pos.x_offset as f32 * scale,
            ));
            cluster.advance += pos.x_advance as f32 * scale;
        }
        clusters
    }

    /// 横組みで1行を組む。グリフ番号と行頭からの相対位置、行の幅を返す。
    fn shape_line(&self, text: &str, font_size: f32) -> (Vec<(u16, f32, f32)>, f32) {
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.set_direction(Direction::LeftToRight);
        buffer.set_script(script::HAN);
        buffer.set_language(self.language.clone());
        let shaped = rustybuzz::shape(&self.face, &[], buffer);

        let scale = font_size / self.face.units_per_em() as f32;
        let mut pen = 0.0;
        let glyphs = shaped
            .glyph_infos()
            .iter()
            .zip(shaped.glyph_positions())
            .map(|(info, pos)| {
                let glyph = (info.glyph_id as u16, pen + pos.x_offset as f32 * scale, -pos.y_offset as f32 * scale);
                pen += pos.x_advance as f32 * scale;
                glyph
            })
            .collect();
        (glyphs, pen)
    }
}

struct ShapedCluster {
    byte_start: usize,
    advance: f32,
    /// グリフ番号と、列の中心・クラスタ先頭からの相対位置
    glyphs: Vec<(u16, f32, f32)>,
    /// 字サイズに対する倍率
    scale: f32,
    /// 90°回転して描くか
    rotated: bool,
    /// カーソルが止まる数。縦中横は桁ごとに止まる
    stops: usize,
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
    /// 色属性。どのランにも含まれなければ0
    pub color: u8,
    /// 字サイズに対する倍率
    pub scale: f32,
    /// 時計回りに90°回転して描くか
    pub rotated: bool,
}

/// 色属性のラン。範囲は文字単位で、重ならず、先頭から順に並べる。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub range: Range<usize>,
    pub color: u8,
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
    pub fn new(text: &str, font: &Font, settings: Settings, runs: &[Run]) -> Self {
        let mut layout = Self {
            glyphs: Vec::new(),
            settings,
            stops: Vec::new(),
            columns: Vec::new(),
        };
        let hyphen = font.shape_rotated("-", settings.font_size);
        let mut char_index = 0;
        for line in text.split('\n') {
            layout.push_paragraph(line.strip_suffix('\r').unwrap_or(line), char_index, font, &hyphen);
            char_index += line.chars().count() + 1;
        }
        layout.apply_runs(runs);
        layout
    }

    /// グリフは文字順に並ぶので、ランと並行に進めて色属性を付ける。
    fn apply_runs(&mut self, runs: &[Run]) {
        let mut runs = runs.iter().peekable();
        for glyph in &mut self.glyphs {
            while runs.next_if(|run| run.range.end <= glyph.char_index).is_some() {}
            if let Some(run) = runs.peek().filter(|run| run.range.contains(&glyph.char_index)) {
                glyph.color = run.color;
            }
        }
    }

    fn push_paragraph(&mut self, text: &str, first_char: usize, font: &Font, hyphen: &[ShapedCluster]) {
        let Settings { font_size, chars_per_column, .. } = self.settings;
        let shaped = font.shape_oriented(text, font_size);

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
        let ends = kinsoku::break_columns(
            &metrics,
            chars_per_column as f32 * font_size,
            kinsoku::Wrap { hanging: true, hyphen: hyphen.first().map(|c| c.advance) },
        );

        let mut char_index = first_char;
        let mut y = 0.0;
        let mut start = 0;
        for end in ends {
            let column = self.columns.len();
            let center = self.column_center(column);
            let first_stop = self.stops.len();
            y = 0.0;
            for (i, cluster) in shaped.iter().enumerate().take(end).skip(start) {
                let stop_advance = cluster.advance / cluster.stops as f32;
                for stop in 0..cluster.stops {
                    self.stops.push(Stop {
                        char_index: char_index + stop,
                        column,
                        y: y + stop_advance * stop as f32,
                        advance: stop_advance,
                    });
                }
                for &(id, dx, dy) in &cluster.glyphs {
                    self.glyphs.push(Glyph {
                        id,
                        x: center + dx,
                        y: y + dy,
                        char_index,
                        color: 0,
                        scale: cluster.scale,
                        rotated: cluster.rotated,
                    });
                }
                y += cluster.advance;
                char_index += text[cluster.byte_start..byte_end(i)].chars().count();
            }
            // 単語の途中で分けたら列末にハイフンを出す。カーソルは止まらない
            if end > 0
                && let Some(next) = metrics.get(end)
                && kinsoku::splits_word(metrics[end - 1].1, next.0)
            {
                self.push_hyphen(hyphen, center, y, char_index - 1);
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

    fn push_hyphen(&mut self, hyphen: &[ShapedCluster], center: f32, y: f32, char_index: usize) {
        for cluster in hyphen {
            for &(id, dx, dy) in &cluster.glyphs {
                self.glyphs.push(Glyph {
                    id,
                    x: center + dx,
                    y: y + dy,
                    char_index,
                    color: 0,
                    scale: cluster.scale,
                    rotated: cluster.rotated,
                });
            }
        }
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

    #[test]
    fn rotated_latin_stops_at_every_character() {
        with_font(|font| {
            let text = "office fi fl The";
            let settings = Settings { chars_per_column: 200, ..SETTINGS };
            let layout = Layout::new(text, font, settings, &[]);
            for index in 0..text.len() {
                assert_eq!(layout.next_char(index), index + 1);
            }
        });
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
            let layout = Layout::new("。", font, SETTINGS, &[]);
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
            let layout = Layout::new(text, font, SETTINGS, &[]);
            assert_eq!(layout.column_count(), 4);
            assert!(layout.cursor_rect(5).y >= 50.0, "ぶら下げは列の外に出る");
            // 「。」は行末にぶら下がる
            assert_eq!(column_text(&layout, text, 0), "あいうえお。");
            assert_eq!(column_text(&layout, text, 1), "か");
            assert_eq!(column_text(&layout, text, 2), "");
            assert_eq!(column_text(&layout, text, 3), "き");
        });
    }

    #[test]
    fn cursor_rect_follows_columns() {
        with_font(|font| {
            let layout = Layout::new("あい\nう", font, SETTINGS, &[]);
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
            let layout = Layout::new("あ\r\nい", font, SETTINGS, &[]);
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
            let layout = Layout::new("あいう\nか\nさしす", font, SETTINGS, &[]);
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
            let layout = Layout::new("あいう\nか", font, SETTINGS, &[]);
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
            let layout = Layout::new("あいうえおかき", font, SETTINGS, &[]);
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
    fn runs_color_glyphs_across_newlines() {
        with_font(|font| {
            let runs = [Run { range: 1..2, color: 3 }, Run { range: 3..5, color: 7 }];
            let layout = Layout::new("あい\nうえお", font, SETTINGS, &runs);
            let colors: Vec<u8> = layout.glyphs.iter().map(|g| g.color).collect();
            assert_eq!(colors, [0, 3, 7, 7, 0]);
        });
    }

    #[test]
    fn two_or_three_digits_share_one_cell_with_a_stop_per_digit() {
        with_font(|font| {
            let layout = Layout::new("あ123い", font, SETTINGS, &[]);
            // 桁ごとにカーソルが止まるが、全体で1字分
            let seen = std::iter::successors(Some(0), |&i| Some(layout.next_char(i)).filter(|&n| n != i));
            assert_eq!(seen.collect::<Vec<_>>(), [0, 1, 2, 3, 4, 5]);
            assert_eq!(layout.cursor_rect(1).y, 10.0);
            assert_eq!(layout.cursor_rect(4).y, 20.0);
            assert!(layout.cursor_rect(2).y > 10.0 && layout.cursor_rect(2).y < 20.0);

            let digits: Vec<_> = layout.glyphs.iter().filter(|g| g.scale < 1.0).collect();
            assert_eq!(digits.len(), 3);
            assert!(digits.iter().all(|g| !g.rotated && g.y > 10.0 && g.y < 20.0));
            // 横に並び、字面の幅に収まる
            assert!(digits[0].x < digits[1].x && digits[1].x < digits[2].x);
            assert!(digits[0].x >= -8.0 - 5.0 - 0.01 && digits[2].x < -8.0 + 5.0);
        });
    }

    #[test]
    fn latin_is_rotated_and_advances_down_the_column() {
        with_font(|font| {
            let layout = Layout::new("あAi い", font, Settings { chars_per_column: 20, ..SETTINGS }, &[]);
            let rotated: Vec<_> = layout.glyphs.iter().filter(|g| g.rotated).collect();
            assert_eq!(rotated.len(), 2);
            // 同じベースライン上を下へ進む
            assert_eq!(rotated[0].x, rotated[1].x);
            assert!(rotated[0].y >= 10.0 && rotated[1].y > rotated[0].y);
            // ベースラインは列の中心より左（文字の上側が右を向く）
            assert!(rotated[0].x < -8.0);

            // 英字は字幅に応じて進み、全角より狭い
            let (a, i) = (layout.cursor_rect(1).y, layout.cursor_rect(2).y);
            assert!(i > a && i - a < 10.0);
        });
    }

    #[test]
    fn split_word_gets_a_display_only_hyphen() {
        with_font(|font| {
            let text = "international";
            let settings = Settings { chars_per_column: 3, ..SETTINGS };
            let layout = Layout::new(text, font, settings, &[]);
            assert!(layout.column_count() >= 2);

            // 文字数より、分割の数だけグリフが多い
            let hyphens = layout.column_count() - 1;
            assert_eq!(layout.glyphs.len(), text.len() + hyphens);

            // カーソルは元の文字にしか止まらず、ソースの文字数のまま
            let seen = std::iter::successors(Some(0), |&i| Some(layout.next_char(i)).filter(|&n| n != i));
            assert_eq!(seen.collect::<Vec<_>>(), (0..=text.len()).collect::<Vec<_>>());

            // ハイフンは列の字数に収まる
            let limit = settings.chars_per_column as f32 * settings.font_size;
            assert!(layout.glyphs.iter().all(|g| g.y <= limit + 0.01), "{:?}", layout.glyphs);
        });
    }

    #[test]
    fn hyphen_takes_the_color_of_the_word_and_runs_stay_ordered() {
        with_font(|font| {
            let settings = Settings { chars_per_column: 3, ..SETTINGS };
            let runs = [Run { range: 0..13, color: 2 }];
            let layout = Layout::new("internationalあ", font, settings, &runs);
            let (last, word) = layout.glyphs.split_last().unwrap();
            assert!(word.iter().all(|g| g.color == 2));
            assert_eq!(last.color, 0);
        });
    }

    #[test]
    fn no_hyphen_for_japanese() {
        with_font(|font| {
            let settings = Settings { chars_per_column: 2, ..SETTINGS };
            let layout = Layout::new("あいうえお", font, settings, &[]);
            assert_eq!(layout.glyphs.len(), 5);
        });
    }

    #[test]
    fn empty_text_has_one_position() {
        with_font(|font| {
            let layout = Layout::new("", font, SETTINGS, &[]);
            assert_eq!(layout.column_count(), 1);
            assert_eq!(layout.cursor_rect(0).y, 0.0);
            assert_eq!(layout.next_char(0), 0);
            assert_eq!(layout.hit_test(-50.0, 50.0), 0);
        });
    }
}

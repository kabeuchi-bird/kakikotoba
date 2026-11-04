//! フォントの検索と読み込み。Qtに依存しない。

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::ops::RangeInclusive;
use std::process::Command;
use std::rc::Rc;

use kakikotoba_layout::Font;

// 単に serif:lang=ja で探すと、中国語フォントが選ばれる環境がある
const SERIF_PATTERN: &str = "Noto Serif CJK JP,Source Han Serif JP,IPAexMincho,IPAMincho,serif:lang=ja";
const SANS_PATTERN: &str = "Noto Sans CJK JP,Source Han Sans JP,IPAexGothic,IPAGothic,sans:lang=ja:bold";

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontSlot {
    /// 編集モードの本文、preedit、候補窓
    Edit,
    /// ページモードの本文
    Page,
    /// ページモードの太字
    Bold,
}

impl FontSlot {
    fn default_pattern(self) -> &'static str {
        match self {
            Self::Edit | Self::Page => SERIF_PATTERN,
            Self::Bold => SANS_PATTERN,
        }
    }
}

pub struct LoadedFont {
    pub font: Font<'static>,
    pub path: String,
}

pub struct Fonts {
    pub edit: LoadedFont,
    pub page: LoadedFont,
    pub bold: LoadedFont,
    shared: Rc<RefCell<Shared>>,
}

/// 補う候補のフォント。
struct Candidate {
    path: String,
    /// 持っている字の範囲
    chars: Vec<RangeInclusive<u32>>,
}

/// 字を補うフォントの状態。用途ごとの `Font` から共有する。
#[derive(Default)]
struct Shared {
    /// 読み込み済みのファイル
    data: HashMap<String, &'static [u8]>,
    /// パターンごとの候補。`fc-match -s` の順
    candidates: HashMap<&'static str, Rc<[Candidate]>>,
    /// パターンと字ごとの、選んだフォントの番号
    chosen: HashMap<(&'static str, char), Option<u16>>,
    /// 補うフォント。番号は読み込んだ順に1から
    loaded: Vec<(String, &'static Font<'static>)>,
}

impl Shared {
    fn read(&mut self, path: &str) -> Result<&'static [u8], String> {
        if let Some(bytes) = self.data.get(path) {
            return Ok(bytes);
        }
        // フォントはアプリ終了まで使い続けるので解放しない
        // ponytail: 選んだことのあるフォントの分だけメモリに残る。頻繁に切り替えて重くなるなら、使わなくなった分を解放する
        let bytes: &'static [u8] = Box::leak(fs::read(path).map_err(|e| format!("{path}: {e}"))?.into_boxed_slice());
        self.data.insert(path.to_owned(), bytes);
        Ok(bytes)
    }

    fn load_fallback(&mut self, path: &str) -> Option<(u16, &'static Font<'static>)> {
        let position = self.loaded.iter().position(|(loaded, _)| loaded == path);
        let index = match position {
            Some(index) => index,
            None => {
                // ponytail: QRawFontがコレクションの先頭しか読めないため番号は0固定
                let font = Font::new(self.read(path).ok()?, 0)?;
                self.loaded.push((path.to_owned(), Box::leak(Box::new(font))));
                self.loaded.len() - 1
            }
        };
        Some((index as u16 + 1, self.loaded[index].1))
    }

    fn fallback(&mut self, pattern: &'static str, c: char) -> Option<(u16, &'static Font<'static>)> {
        if let Some(chosen) = self.chosen.get(&(pattern, c)) {
            return chosen.map(|id| (id, self.loaded[id as usize - 1].1));
        }
        let candidates = self.candidates.entry(pattern).or_insert_with(|| list_candidates(pattern).into()).clone();
        let found = candidates
            .iter()
            .filter(|candidate| candidate.chars.iter().any(|range| range.contains(&(c as u32))))
            .find_map(|candidate| self.load_fallback(&candidate.path));
        self.chosen.insert((pattern, c), found.map(|(id, _)| id));
        found
    }
}

impl Fonts {
    pub fn load_defaults() -> Result<Self, String> {
        let shared = Rc::default();
        let load = |slot: FontSlot| load_font(&shared, slot, &find_font(slot.default_pattern())?);
        let (edit, page, bold) = (load(FontSlot::Edit)?, load(FontSlot::Page)?, load(FontSlot::Bold)?);
        Ok(Self { edit, page, bold, shared })
    }

    /// すべて同じフォントにする。
    #[cfg(test)]
    pub fn from_file(path: &str) -> Result<Self, String> {
        let shared = Rc::default();
        let load = |slot: FontSlot| load_font(&shared, slot, path);
        let (edit, page, bold) = (load(FontSlot::Edit)?, load(FontSlot::Page)?, load(FontSlot::Bold)?);
        Ok(Self { edit, page, bold, shared })
    }

    pub fn get(&self, slot: FontSlot) -> &LoadedFont {
        match slot {
            FontSlot::Edit => &self.edit,
            FontSlot::Page => &self.page,
            FontSlot::Bold => &self.bold,
        }
    }

    /// 描画に使うファイル。`font` は `Glyph::font` の番号。
    pub fn path(&self, slot: FontSlot, font: u16) -> String {
        match font.checked_sub(1) {
            None => self.get(slot).path.clone(),
            Some(index) => self.shared.borrow().loaded[index as usize].0.clone(),
        }
    }

    /// `path` が空なら既定のフォントにする。変わったらtrue。失敗したら今のフォントのまま。
    pub fn set(&mut self, slot: FontSlot, path: &str) -> Result<bool, String> {
        let path = if path.is_empty() { find_font(slot.default_pattern())? } else { path.to_owned() };
        if self.get(slot).path == path {
            return Ok(false);
        }
        let loaded = load_font(&self.shared, slot, &path)?;
        match slot {
            FontSlot::Edit => self.edit = loaded,
            FontSlot::Page => self.page = loaded,
            FontSlot::Bold => self.bold = loaded,
        }
        Ok(true)
    }
}

fn find_font(pattern: &str) -> Result<String, String> {
    let output = Command::new("fc-match")
        .args(["-f", "%{file}", pattern])
        .output()
        .map_err(|e| format!("fc-match を実行できません: {e}"))?;
    String::from_utf8(output.stdout)
        .ok()
        .filter(|path| !path.is_empty())
        .ok_or_else(|| "日本語フォントが見つかりません".to_owned())
}

fn load_font(shared: &Rc<RefCell<Shared>>, slot: FontSlot, path: &str) -> Result<LoadedFont, String> {
    let bytes = shared.borrow_mut().read(path)?;
    // ponytail: QRawFontがコレクションの先頭しか読めないため番号は0固定。先頭以外を使うならQRawFontの読み込みごと見直す
    let font = Font::new(bytes, 0).ok_or_else(|| format!("{path}: フォントを読めません"))?;
    let shared = Rc::clone(shared);
    let pattern = slot.default_pattern();
    let font = font.with_fallback(Box::new(move |c| shared.borrow_mut().fallback(pattern, c)));
    Ok(LoadedFont { font, path: path.to_owned() })
}

/// 字を補う候補を、fontconfigの順に返す。
fn list_candidates(pattern: &str) -> Vec<Candidate> {
    let format = "%{file}\t%{index}\t%{color}\t%{charset}\n";
    let output = Command::new("fc-match").args(["-s", "-f", format, pattern]).output();
    output.map_or_else(|_| Vec::new(), |output| parse_candidates(&String::from_utf8_lossy(&output.stdout)))
}

/// コレクションの先頭の書体だけを候補にする。カラーフォントはQRawFontで崩れて描かれるので除く。
/// `charset` は `20-7e a0 ...` の形。
fn parse_candidates(list: &str) -> Vec<Candidate> {
    list.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let (path, index, color, charset) = (fields.next()?, fields.next()?, fields.next()?, fields.next()?);
            let code = |text: &str| u32::from_str_radix(text, 16).ok();
            let chars = charset
                .split(' ')
                .filter_map(|range| match range.split_once('-') {
                    Some((start, end)) => Some(code(start)?..=code(end)?),
                    None => Some(code(range)?..=code(range)?),
                })
                .collect();
            (index == "0" && color != "True").then(|| Candidate { path: path.to_owned(), chars })
        })
        .collect()
}

/// 選べるフォントの `(表示名, パス)` を、表示名の順に返す。
pub fn list_fonts() -> Vec<(String, String)> {
    let output = Command::new("fc-list")
        .args([":lang=ja", "--format", "%{family[0]}\t%{style[0]}\t%{file}\t%{index}\n"])
        .output();
    output.map_or_else(|_| Vec::new(), |output| parse_font_list(&String::from_utf8_lossy(&output.stdout)))
}

/// コレクションの先頭の書体だけを選べるようにする。
fn parse_font_list(list: &str) -> Vec<(String, String)> {
    let mut fonts: Vec<(String, String)> = list
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let (family, style, path, index) = (fields.next()?, fields.next()?, fields.next()?, fields.next()?);
            (index == "0").then(|| (format!("{family} {style}"), path.to_owned()))
        })
        .collect();
    fonts.sort();
    fonts.dedup();
    fonts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // 範囲の列（`chars`）と比べるので、範囲が1つでも配列のままでよい
    #[allow(clippy::single_range_in_vec_init)]
    fn candidates_keep_their_order_and_skip_later_faces_and_color_fonts() {
        let list = "/b.ttf\t0\tFalse\t20-7e 3042\n/c.ttc\t1\tFalse\t3042\n/e.ttf\t0\tTrue\t3042\n/a.ttf\t0\tFalse\t3042-3044\n壊れた行\n";
        let candidates = parse_candidates(list);
        let paths: Vec<&str> = candidates.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, ["/b.ttf", "/a.ttf"]);
        assert_eq!(candidates[0].chars, [0x20..=0x7e, 0x3042..=0x3042]);
        assert_eq!(candidates[1].chars, [0x3042..=0x3044]);
    }

    #[test]
    fn missing_character_is_found_through_fontconfig_and_cached() {
        let path = kakikotoba_test_fonts::path(kakikotoba_test_fonts::LATIN_SERIF);
        let fonts = Fonts::from_file(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let found = fonts.shared.borrow_mut().fallback(SERIF_PATTERN, 'あ').map(|(id, _)| id);
        let id = found.expect("fontconfigで日本語フォントが見つからない（noto-fonts-cjk を入れる）");
        assert_eq!(fonts.shared.borrow_mut().fallback(SERIF_PATTERN, 'い').map(|(id, _)| id), Some(id));
        assert_eq!(fonts.shared.borrow().loaded.len(), 1);
        assert_ne!(fonts.path(FontSlot::Edit, id), fonts.path(FontSlot::Edit, 0));
        // どのフォントにもない字
        assert!(fonts.shared.borrow_mut().fallback(SERIF_PATTERN, '\u{10ffff}').is_none());
    }

    #[test]
    fn font_list_has_only_the_first_face_of_each_file() {
        let list = "明朝\tRegular\t/m.ttc\t0\n明朝 KR\tRegular\t/m.ttc\t1\nA\tBold\t/a.ttf\t0\nA\tBold\t/a.ttf\t0\n壊れた行\n";
        let path = |label: &str, path: &str| (label.to_owned(), path.to_owned());
        assert_eq!(parse_font_list(list), [path("A Bold", "/a.ttf"), path("明朝 Regular", "/m.ttc")]);
    }
}

//! フォントの検索と読み込み。Qtに依存しない。

use std::collections::HashMap;
use std::fs;
use std::process::Command;

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
    /// 読み込み済みのファイル
    data: HashMap<String, &'static [u8]>,
}

impl Fonts {
    pub fn load_defaults() -> Result<Self, String> {
        let mut data = HashMap::new();
        let mut load = |slot: FontSlot| load_font(&mut data, &find_font(slot.default_pattern())?);
        let (edit, page, bold) = (load(FontSlot::Edit)?, load(FontSlot::Page)?, load(FontSlot::Bold)?);
        Ok(Self { edit, page, bold, data })
    }

    /// すべて同じフォントにする。
    #[cfg(test)]
    pub fn from_file(path: &str) -> Result<Self, String> {
        let mut data = HashMap::new();
        let (edit, page, bold) = (load_font(&mut data, path)?, load_font(&mut data, path)?, load_font(&mut data, path)?);
        Ok(Self { edit, page, bold, data })
    }

    pub fn get(&self, slot: FontSlot) -> &LoadedFont {
        match slot {
            FontSlot::Edit => &self.edit,
            FontSlot::Page => &self.page,
            FontSlot::Bold => &self.bold,
        }
    }

    /// `path` が空なら既定のフォントにする。変わったらtrue。失敗したら今のフォントのまま。
    pub fn set(&mut self, slot: FontSlot, path: &str) -> Result<bool, String> {
        let path = if path.is_empty() { find_font(slot.default_pattern())? } else { path.to_owned() };
        if self.get(slot).path == path {
            return Ok(false);
        }
        let loaded = load_font(&mut self.data, &path)?;
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

fn load_font(data: &mut HashMap<String, &'static [u8]>, path: &str) -> Result<LoadedFont, String> {
    let bytes = match data.get(path) {
        Some(bytes) => *bytes,
        None => {
            // フォントはアプリ終了まで使い続けるので解放しない
            // ponytail: 選んだことのあるフォントの分だけメモリに残る。頻繁に切り替えて重くなるなら、使わなくなった分を解放する
            let bytes: &'static [u8] = Box::leak(fs::read(path).map_err(|e| format!("{path}: {e}"))?.into_boxed_slice());
            data.insert(path.to_owned(), bytes);
            bytes
        }
    };
    // ponytail: QRawFontがコレクションの先頭しか読めないため番号は0固定。先頭以外を使うならQRawFontの読み込みごと見直す
    let font = Font::new(bytes, 0).ok_or_else(|| format!("{path}: フォントを読めません"))?;
    Ok(LoadedFont { font, path: path.to_owned() })
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
    fn font_list_has_only_the_first_face_of_each_file() {
        let list = "明朝\tRegular\t/m.ttc\t0\n明朝 KR\tRegular\t/m.ttc\t1\nA\tBold\t/a.ttf\t0\nA\tBold\t/a.ttf\t0\n壊れた行\n";
        let path = |label: &str, path: &str| (label.to_owned(), path.to_owned());
        assert_eq!(parse_font_list(list), [path("A Bold", "/a.ttf"), path("明朝 Regular", "/m.ttc")]);
    }
}

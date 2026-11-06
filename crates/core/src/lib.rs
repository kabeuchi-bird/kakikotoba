//! バッファ、Undo/Redo（操作ログ方式）

pub mod recovery;

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::ops::Range;
use std::path::Path;

use ropey::Rope;

/// 1回の置換操作。逆適用でUndoする。
struct Edit {
    at: usize,
    removed: String,
    inserted: String,
}

/// 位置はすべて文字（char）単位。
pub struct Buffer {
    rope: Rope,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    /// 保存時点のUndo段数。保存時点へ戻れなくなったら `None`。
    saved_depth: Option<usize>,
    /// 本文が変わるたびに増える。自動保存で変化を見分ける
    revision: u64,
    /// 最後のUndoの段が、打ちかけの語で、まだ字を足せるか
    typing: bool,
}

/// Undoで語ごとにまとめる字。英数字（ラテン、ギリシャ、キリル）と `'` `-`、語の中の結合文字。
/// 全角の英数字（U+FF00〜）は含めない。
fn is_word_char(c: char) -> bool {
    matches!(c, '\'' | '-') || (c.is_alphanumeric() || is_combining_mark(c)) && in_word_script(c)
}

/// アクセントなどを付ける結合文字（ダイアクリティカルマーク）。
fn is_combining_mark(c: char) -> bool {
    matches!(c as u32, 0x300..=0x36f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe20..=0xfe2f)
}

/// ラテン、ギリシャ、キリルの各ブロックか（Unicodeのブロック一覧に従う）。
fn in_word_script(c: char) -> bool {
    matches!(c as u32,
        // ラテン: 基本・Latin-1補助・拡張A/B、IPA、音声記号（拡張・補助を含む）
        0x0000..=0x02af | 0x1d00..=0x1dbf
        // ラテン: 拡張追加、拡張C/D/E、合字（ﬀ ﬁ など）、拡張F/G
        | 0x1e00..=0x1eff | 0x2c60..=0x2c7f | 0xa720..=0xa7ff | 0xab30..=0xab6f | 0xfb00..=0xfb06
        | 0x10780..=0x107bf | 0x1df00..=0x1dfff
        // ギリシャ（拡張を含む）
        | 0x0370..=0x03ff | 0x1f00..=0x1fff
        // キリル（補助、拡張A/B/C/Dを含む）
        | 0x0400..=0x052f | 0x1c80..=0x1c8f | 0x2de0..=0x2dff | 0xa640..=0xa69f | 0x1e030..=0x1e08f
        // 結合文字のブロック（文字そのものは is_combining_mark で絞る）
        | 0x0300..=0x036f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe20..=0xfe2f)
}

impl Buffer {
    pub fn new(text: &str) -> Self {
        Self::from_rope(Rope::from_str(text))
    }

    /// UTF-8以外のファイルは `InvalidData` エラーになる。
    pub fn load(path: &Path) -> io::Result<Self> {
        Ok(Self::from_rope(Rope::from_reader(BufReader::new(File::open(path)?))?))
    }

    /// 異常終了から復元した本文。ファイルの中身とは違うので、未保存の状態で始める。
    pub fn recovered(text: &str) -> Self {
        Self { saved_depth: None, ..Self::new(text) }
    }

    fn from_rope(rope: Rope) -> Self {
        Self {
            rope,
            undo: Vec::new(),
            redo: Vec::new(),
            saved_depth: Some(0),
            revision: 0,
            typing: false,
        }
    }

    /// 書き込み途中で失敗しても元ファイルを壊さないよう、一時ファイル経由で置き換える。
    pub fn save(&mut self, path: &Path) -> io::Result<()> {
        // シンボリックリンク自体を置き換えないよう、実体のパスに解決する
        let target = match fs::canonicalize(path) {
            Ok(target) => target,
            Err(e) if e.kind() == io::ErrorKind::NotFound => path.to_path_buf(),
            Err(e) => return Err(e),
        };
        let name = target
            .file_name()
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        let mut tmp_name = name.to_os_string();
        tmp_name.push(".kakikotoba-tmp");
        let tmp = target.with_file_name(tmp_name);

        let result = self.write_replacing(&tmp, &target);
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result?;
        self.saved_depth = Some(self.undo.len());
        self.typing = false;
        Ok(())
    }

    fn write_replacing(&self, tmp: &Path, target: &Path) -> io::Result<()> {
        let mut writer = BufWriter::new(File::create(tmp)?);
        self.rope.write_to(&mut writer)?;
        writer.flush()?;
        let file = writer.into_inner().map_err(|e| e.into_error())?;
        match fs::metadata(target) {
            Ok(meta) => file.set_permissions(meta.permissions())?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        file.sync_all()?;
        fs::rename(tmp, target)?;
        // リネームの永続化。失敗しても保存内容自体は書き込み済み
        if let Some(parent) = target.parent().filter(|p| !p.as_os_str().is_empty()) {
            let _ = File::open(parent).and_then(|dir| dir.sync_all());
        }
        Ok(())
    }

    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn is_modified(&self) -> bool {
        self.saved_depth != Some(self.undo.len())
    }

    /// 本文の版。編集、Undo、Redoのたびに増える。自動保存で変化を見分けるのに使う。
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// 打った字を `at` に入れ、入れた後のカーソル位置を返す。
    /// 語の字が続けて届いたら、Undoの1段にまとめる。直後の半角空白1つまで含めて区切る。
    /// 1字でないもの（IMEで確定した文字列など）や語の字でないものは、`replace` と同じく1段にする。
    pub fn type_text(&mut self, at: usize, text: &str) -> usize {
        let mut chars = text.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else {
            return self.replace(at..at, text);
        };
        if !is_word_char(c) && c != ' ' {
            return self.replace(at..at, text);
        }
        let joins = self.typing
            && self.redo.is_empty()
            // 保存した時点の段を書き換えると、未保存かどうかの判定が狂う
            && self.saved_depth != Some(self.undo.len())
            && self.undo.last().is_some_and(|edit| edit.removed.is_empty() && edit.at + edit.inserted.chars().count() == at);
        let cursor = if joins {
            let cursor = self.apply("", text, at);
            self.undo.last_mut().expect("joins で確かめた").inserted.push(c);
            cursor
        } else {
            self.replace(at..at, text)
        };
        // 空白で語を閉じる
        self.typing = is_word_char(c);
        cursor
    }

    /// 打ちかけの語を区切る。カーソルを動かしたときに呼ぶ。
    pub fn end_typing(&mut self) {
        self.typing = false;
    }

    /// `range` を `text` に置き換え、置換後のカーソル位置を返す。
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> usize {
        let at = range.start;
        // 何も変えない操作（空の確定、文頭でのBackspaceなど）では、打ちかけの語を区切らない
        if range.is_empty() && text.is_empty() {
            return at;
        }
        self.typing = false;
        // 保存時点より前に戻ってから編集すると、保存時点へは戻れなくなる
        if self.saved_depth.is_some_and(|depth| depth > self.undo.len()) {
            self.saved_depth = None;
        }
        let edit = Edit {
            at,
            removed: self.rope.slice(range).to_string(),
            inserted: text.to_owned(),
        };
        let cursor = self.apply(&edit.removed, &edit.inserted, at);
        self.undo.push(edit);
        self.redo.clear();
        cursor
    }

    /// Undoできれば、Undo後のカーソル位置を返す。
    pub fn undo(&mut self) -> Option<usize> {
        self.typing = false;
        let edit = self.undo.pop()?;
        let cursor = self.apply(&edit.inserted, &edit.removed, edit.at);
        self.redo.push(edit);
        Some(cursor)
    }

    /// Redoできれば、Redo後のカーソル位置を返す。
    pub fn redo(&mut self) -> Option<usize> {
        self.typing = false;
        let edit = self.redo.pop()?;
        let cursor = self.apply(&edit.removed, &edit.inserted, edit.at);
        self.undo.push(edit);
        Some(cursor)
    }

    fn apply(&mut self, old: &str, new: &str, at: usize) -> usize {
        self.revision += 1;
        self.rope.remove(at..at + old.chars().count());
        self.rope.insert(at, new);
        at + new.chars().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo_restores_text_and_cursor() {
        let mut b = Buffer::new("吾輩は猫");
        assert_eq!(b.replace(3..4, "犬である"), 7);
        assert_eq!(b.rope().to_string(), "吾輩は犬である");
        assert_eq!(b.undo(), Some(4));
        assert_eq!(b.rope().to_string(), "吾輩は猫");
        assert!(b.can_redo());
        assert_eq!(b.redo(), Some(7));
        assert!(b.can_undo() && !b.can_redo());
        assert_eq!(b.rope().to_string(), "吾輩は犬である");
        assert_eq!(b.redo(), None);
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut b = Buffer::new("");
        b.replace(0..0, "あ");
        b.undo();
        b.replace(0..0, "い");
        assert_eq!(b.redo(), None);
        assert_eq!(b.rope().to_string(), "い");
    }

    #[test]
    fn modified_tracks_saved_state() {
        let dir = std::env::temp_dir().join(format!("kakikotoba-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("本文.txt");

        let mut b = Buffer::new("");
        assert!(!b.is_modified());
        b.replace(0..0, "あ");
        assert!(b.is_modified());
        b.save(&path).unwrap();
        assert!(!b.is_modified());
        assert_eq!(fs::read_to_string(&path).unwrap(), "あ");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "一時ファイルが残っている");

        b.undo();
        assert!(b.is_modified());
        b.redo();
        assert!(!b.is_modified());

        // 保存時点より前に戻って編集したら、同じ段数でも未保存扱い
        b.undo();
        b.replace(0..0, "い");
        assert!(b.is_modified());

        assert_eq!(Buffer::load(&path).unwrap().rope().to_string(), "あ");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// 字を1つずつ打つ。
    fn type_chars(b: &mut Buffer, at: usize, text: &str) -> usize {
        text.chars().fold(at, |at, c| b.type_text(at, &c.to_string()))
    }

    /// Undoを1段ずつ戻し、各段の後の本文を返す。
    fn undo_steps(b: &mut Buffer) -> Vec<String> {
        std::iter::from_fn(|| b.undo().map(|_| b.rope().to_string())).collect()
    }

    #[test]
    fn typed_words_undo_one_word_at_a_time_with_their_trailing_space() {
        let mut b = Buffer::new("");
        let cursor = type_chars(&mut b, 0, "don't re-read 2026  x");
        assert_eq!(cursor, 21);
        // 拡張ブロックのラテン、ギリシャ、キリル、結合文字も語の字
        let mut v = Buffer::new("");
        type_chars(&mut v, 0, "tệ aⱥꝛﬁ ἄλφα ԁᲀꙁ e\u{301}");
        assert_eq!(undo_steps(&mut v), ["tệ aⱥꝛﬁ ἄλφα ԁᲀꙁ ", "tệ aⱥꝛﬁ ἄλφα ", "tệ aⱥꝛﬁ ", "tệ ", ""]);
        // 全角の英字はまとめない
        let mut w = Buffer::new("");
        type_chars(&mut w, 0, "ＡＢ");
        assert_eq!(undo_steps(&mut w), ["Ａ", ""]);
        // 2つ目の空白は語の後ではないので、それだけで1段
        assert_eq!(undo_steps(&mut b), ["don't re-read 2026  ", "don't re-read 2026 ", "don't re-read ", "don't ", ""]);
        // Redoも同じ段で進む
        assert_eq!(b.redo(), Some(6));
        assert_eq!(b.rope().to_string(), "don't ");
    }

    #[test]
    fn committed_text_newlines_and_other_characters_are_steps_of_their_own() {
        let mut b = Buffer::new("");
        let at = type_chars(&mut b, 0, "ab");
        // IMEで確定した文字列、全角の英字、句読点、改行
        let at = b.type_text(at, "漢字");
        let at = type_chars(&mut b, at, "ＡＢ、\ncd");
        b.replace(at..at, "貼り付け");
        assert_eq!(
            undo_steps(&mut b),
            ["ab漢字ＡＢ、\ncd", "ab漢字ＡＢ、\n", "ab漢字ＡＢ、", "ab漢字ＡＢ", "ab漢字Ａ", "ab漢字", "ab", ""]
        );
    }

    #[test]
    fn moving_the_cursor_undoing_or_saving_ends_the_word() {
        let mut b = Buffer::new("");
        let at = type_chars(&mut b, 0, "ab");
        b.end_typing();
        type_chars(&mut b, at, "cd");
        // 何も変えない操作では区切らない
        b.replace(at + 2..at + 2, "");
        b.type_text(at + 2, "");
        type_chars(&mut b, at + 2, "e");
        // 離れた位置に打てば、区切りを呼ばなくても別の段
        type_chars(&mut b, 0, "x");
        assert_eq!(undo_steps(&mut b), ["abcde", "ab", ""]);

        let dir = std::env::temp_dir().join(format!("kakikotoba-typing-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut b = Buffer::new("");
        let at = type_chars(&mut b, 0, "ab");
        b.save(&dir.join("本文.txt")).unwrap();
        type_chars(&mut b, at, "c");
        assert!(b.is_modified());
        b.undo();
        assert!(!b.is_modified(), "保存した時点まで戻れる");
        assert_eq!(b.rope().to_string(), "ab");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn revision_changes_on_every_edit_and_recovered_starts_modified() {
        let mut b = Buffer::new("");
        let start = b.revision();
        b.replace(0..0, "あ");
        let edited = b.revision();
        assert_ne!(edited, start);
        b.undo();
        assert_ne!(b.revision(), edited);
        // 何も変えない置換では増えない
        let before = b.revision();
        b.replace(0..0, "");
        assert_eq!(b.revision(), before);

        let r = Buffer::recovered("い");
        assert!(r.is_modified());
        assert_eq!(r.rope().to_string(), "い");
    }
}

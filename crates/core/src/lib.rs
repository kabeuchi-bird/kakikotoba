//! バッファ、Undo/Redo（操作ログ方式）

pub mod recovery;
pub mod script;

use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::ops::Range;
use std::os::unix::fs::{MetadataExt, fchown};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

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
    /// 本文が変わるたびに新しい値になる。プロセス内のどのバッファとも重ならない
    revision: u64,
    /// 打ちかけの語の終わりの位置。最後のUndoの段がその語で、この位置に打てば同じ段に足す
    typing: Option<usize>,
    /// 開いたファイルの先頭にBOMがあったか。本文には入れず、保存のときに付け直す
    bom: bool,
}

const BOM: &str = "\u{feff}";

/// 版の番号。文書を替えても前の文書の版と見分けられるよう、全バッファで通して数える
fn next_revision() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Undoで語ごとにまとめる字。英字、数字、`'` `-`。全角の英数字は含めない。
fn is_word_char(c: char) -> bool {
    script::is_letter(c) || matches!(c, '0'..='9' | '\'' | '-')
}

impl Buffer {
    pub fn new(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            undo: Vec::new(),
            redo: Vec::new(),
            saved_depth: Some(0),
            revision: next_revision(),
            typing: None,
            bom: false,
        }
    }

    /// UTF-8以外のファイルは `InvalidData` エラーになる。先頭のBOMは本文に入れない。
    pub fn load(path: &Path) -> io::Result<Self> {
        let text = fs::read_to_string(path)?;
        let body = text.strip_prefix(BOM);
        Ok(Self { bom: body.is_some(), ..Self::new(body.unwrap_or(&text)) })
    }

    /// 異常終了から復元した本文。ファイルの中身とは違うので、未保存の状態で始める。
    /// 写しはBOMの有無を持たないので、保存先になる元のファイルに合わせる。
    pub fn recovered(text: &str, path: Option<&Path>) -> Self {
        let mut head = [0; BOM.len()];
        let bom = path.is_some_and(|path| {
            File::open(path).and_then(|mut file| file.read_exact(&mut head)).is_ok() && head == *BOM.as_bytes()
        });
        Self { saved_depth: None, bom, ..Self::new(text) }
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
        self.typing = None;
        Ok(())
    }

    fn write_body(&self, file: File) -> io::Result<File> {
        let mut writer = BufWriter::new(file);
        if self.bom {
            writer.write_all(BOM.as_bytes())?;
        }
        self.rope.write_to(&mut writer)?;
        writer.into_inner().map_err(|e| e.into_error())
    }

    fn write_replacing(&self, tmp: &Path, target: &Path) -> io::Result<()> {
        let file = match File::create(tmp) {
            Ok(file) => file,
            // ディレクトリに書けなくても、ファイルには書けることがある。置き換えられないので直接書く
            // （途中で失敗すると元の中身は残らないが、保存できないよりよい）
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied && target.is_file() => {
                return self.write_body(File::options().write(true).truncate(true).open(target)?)?.sync_all();
            }
            Err(e) => return Err(e),
        };
        let file = self.write_body(file)?;
        match fs::metadata(target) {
            Ok(meta) => {
                // 所有者を引き継ぐ。権限がなくて変えられなければ、自分の所有になる
                let _ = fchown(&file, Some(meta.uid()), Some(meta.gid()));
                file.set_permissions(meta.permissions())?;
            }
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

    /// 本文の版。編集、Undo、Redoのたびに変わり、別のバッファの版とも重ならない。
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
        // `typing` を立てるのは直前の打鍵だけで、Undo・Redo・保存・その他の編集で下ろす。
        // だから同じ段に足すときは、Redoは空で、保存した時点の段でもない
        let cursor = if self.typing == Some(at) {
            debug_assert!(self.redo.is_empty() && self.saved_depth != Some(self.undo.len()));
            let cursor = self.apply("", text, at);
            self.undo.last_mut().expect("typing のときは段がある").inserted.push(c);
            cursor
        } else {
            self.replace(at..at, text)
        };
        // 空白で語を閉じる
        self.typing = is_word_char(c).then_some(cursor);
        cursor
    }

    /// 打ちかけの語を区切る。カーソルを動かしたときに呼ぶ。
    pub fn end_typing(&mut self) {
        self.typing = None;
    }

    /// `range` を `text` に置き換え、置換後のカーソル位置を返す。
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> usize {
        let at = range.start;
        // 何も変えない操作（空の確定、文頭でのBackspaceなど）では、打ちかけの語を区切らない
        if range.is_empty() && text.is_empty() {
            return at;
        }
        self.typing = None;
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
        self.typing = None;
        let edit = self.undo.pop()?;
        let cursor = self.apply(&edit.inserted, &edit.removed, edit.at);
        self.redo.push(edit);
        Some(cursor)
    }

    /// Redoできれば、Redo後のカーソル位置を返す。
    pub fn redo(&mut self) -> Option<usize> {
        self.typing = None;
        let edit = self.redo.pop()?;
        let cursor = self.apply(&edit.removed, &edit.inserted, edit.at);
        self.undo.push(edit);
        Some(cursor)
    }

    fn apply(&mut self, old: &str, new: &str, at: usize) -> usize {
        self.revision = next_revision();
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
    fn bom_stays_out_of_the_text_and_is_kept_on_save() {
        let dir = std::env::temp_dir().join(format!("kakikotoba-bom-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let (with_bom, without) = (dir.join("bom.txt"), dir.join("plain.txt"));
        fs::write(&with_bom, "\u{feff}あ\u{feff}い").unwrap();
        fs::write(&without, "あ").unwrap();

        let mut b = Buffer::load(&with_bom).unwrap();
        // 先頭のBOMだけを外す。途中のU+FEFFは本文
        assert_eq!(b.rope().to_string(), "あ\u{feff}い");
        assert!(!b.is_modified());
        b.replace(0..0, "う");
        b.save(&with_bom).unwrap();
        assert_eq!(fs::read_to_string(&with_bom).unwrap(), "\u{feff}うあ\u{feff}い");

        let mut b = Buffer::load(&without).unwrap();
        b.save(&without).unwrap();
        assert_eq!(fs::read_to_string(&without).unwrap(), "あ");

        // 復元した文書は元のファイルに合わせる
        for (path, expected) in [(&with_bom, "\u{feff}え"), (&without, "え")] {
            Buffer::recovered("え", Some(path)).save(path).unwrap();
            assert_eq!(fs::read_to_string(path).unwrap(), expected);
        }
        fs::remove_dir_all(&dir).unwrap();
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

    #[test]
    fn save_writes_in_place_when_the_directory_is_not_writable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("kakikotoba-readonly-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("本文.txt");
        fs::write(&path, "前").unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
        let mut b = Buffer::load(&path).unwrap();
        b.replace(0..1, "後");
        let saved = b.save(&path);
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        saved.unwrap();
        assert!(!b.is_modified());
        assert_eq!(fs::read_to_string(&path).unwrap(), "後");
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

        let r = Buffer::recovered("い", None);
        assert!(r.is_modified());
        assert_eq!(r.rope().to_string(), "い");
    }
}

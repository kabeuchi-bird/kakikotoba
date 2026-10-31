//! バッファ、Undo/Redo（操作ログ方式）

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
}

impl Buffer {
    pub fn new(text: &str) -> Self {
        Self::from_rope(Rope::from_str(text))
    }

    /// UTF-8以外のファイルは `InvalidData` エラーになる。
    pub fn load(path: &Path) -> io::Result<Self> {
        Ok(Self::from_rope(Rope::from_reader(BufReader::new(File::open(path)?))?))
    }

    fn from_rope(rope: Rope) -> Self {
        Self {
            rope,
            undo: Vec::new(),
            redo: Vec::new(),
            saved_depth: Some(0),
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

    pub fn is_modified(&self) -> bool {
        self.saved_depth != Some(self.undo.len())
    }

    /// `range` を `text` に置き換え、置換後のカーソル位置を返す。
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> usize {
        let at = range.start;
        if range.is_empty() && text.is_empty() {
            return at;
        }
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
        let edit = self.undo.pop()?;
        let cursor = self.apply(&edit.inserted, &edit.removed, edit.at);
        self.redo.push(edit);
        Some(cursor)
    }

    /// Redoできれば、Redo後のカーソル位置を返す。
    pub fn redo(&mut self) -> Option<usize> {
        let edit = self.redo.pop()?;
        let cursor = self.apply(&edit.removed, &edit.inserted, edit.at);
        self.undo.push(edit);
        Some(cursor)
    }

    fn apply(&mut self, old: &str, new: &str, at: usize) -> usize {
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
        assert_eq!(b.redo(), Some(7));
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
}

//! cxx-qtによるQt統合、UI

mod editor;
mod fcitx;
mod fonts;
mod item;
mod page_view;

use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cxx_qt_lib::QString;
use kakikotoba_core::Buffer;

use editor::Editor;
use fonts::Fonts;

fn open_buffer(path: &Path) -> io::Result<Buffer> {
    match Buffer::load(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Buffer::new("")),
        result => result,
    }
}

/// 起動時に指定されたファイルを開く。バッファ、結び付けるパス、開けなかったときのエラーメッセージを返す。
fn open_at_startup(path: Option<PathBuf>) -> (Buffer, Option<PathBuf>, Option<String>) {
    let Some(path) = path else {
        return (Buffer::new(""), None, None);
    };
    // 復旧の写しに残すパスが起動時のディレクトリに左右されないよう、絶対パスで持つ
    let path = std::path::absolute(&path).unwrap_or(path);
    match open_buffer(&path) {
        Ok(buffer) => (buffer, Some(path), None),
        // 読めないファイルに結び付けた空の文書は、保存で中身を失う。無題の文書にする
        Err(e) => (Buffer::new(""), None, Some(format!("{} を開けません: {e}", path.display()))),
    }
}

fn startup() -> Result<Editor, String> {
    let (buffer, path, error) = open_at_startup(std::env::args_os().nth(1).map(PathBuf::from));
    if let Some(error) = error {
        eprintln!("kakikotoba: {error}");
        item::set_startup_error(error);
    }
    let mut editor = Editor::new(buffer, path, Fonts::load_defaults()?);
    // 置き場所が決まらない環境（HOMEがない）では自動保存しない
    if let Some(dir) = kakikotoba_core::recovery::default_dir() {
        editor.enable_recovery(&dir);
    }
    Ok(editor)
}

fn main() -> ExitCode {
    match startup() {
        Ok(editor) => item::set_startup_editor(editor),
        Err(message) => {
            eprintln!("kakikotoba: {message}");
            return ExitCode::FAILURE;
        }
    }
    match item::qobject::run_application(&QString::from("qrc:/qt/qml/kakikotoba/qml/main.qml")) {
        0 => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_file_is_bound_by_absolute_path_and_unreadable_one_opens_untitled() {
        let dir = std::env::temp_dir().join(format!("kakikotoba-startup-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let broken = dir.join("壊.txt");
        std::fs::write(&broken, [0xff, 0xfe]).unwrap();

        let (buffer, path, error) = open_at_startup(Some(broken));
        assert_eq!(buffer.rope().len_chars(), 0);
        assert_eq!(path, None, "読めないファイルには結び付けない");
        assert!(error.unwrap().contains("壊.txt"));

        // ないファイルは、そのパスの新しい文書
        let (_, path, error) = open_at_startup(Some(PathBuf::from("ない.txt")));
        assert_eq!(path, Some(std::env::current_dir().unwrap().join("ない.txt")));
        assert_eq!(error, None);
        assert_eq!(open_at_startup(None).1, None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

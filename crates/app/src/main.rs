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

fn startup() -> Result<Editor, String> {
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    // 読めないファイルを空のバッファで開くと、保存で中身を失う
    let buffer = match &path {
        Some(path) => open_buffer(path).map_err(|e| format!("{}: {e}", path.display()))?,
        None => Buffer::new(""),
    };
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

//! cxx-qtによるQt統合、UI

mod editor;
mod fcitx;
mod fonts;
mod item;
mod page_view;

use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};
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
    Ok(Editor::new(buffer, path, Fonts::load_defaults()?))
}

fn main() -> ExitCode {
    match startup() {
        Ok(editor) => item::set_startup_editor(editor),
        Err(message) => {
            eprintln!("kakikotoba: {message}");
            return ExitCode::FAILURE;
        }
    }
    let mut app = QGuiApplication::new();
    // QMLのSettingsの保存先を決めるのに必要
    if let Some(mut app) = app.as_mut() {
        app.as_mut().set_organization_name(&QString::from("kakikotoba"));
        app.as_mut().set_application_name(&QString::from("kakikotoba"));
    }
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/kakikotoba/qml/main.qml"));
    }
    match app.as_mut().map(|app| app.exec()) {
        Some(0) => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

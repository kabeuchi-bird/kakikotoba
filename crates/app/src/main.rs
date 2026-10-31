//! cxx-qtによるQt統合、UI

mod editor;
mod item;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{fs, io};

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};
use kakikotoba_core::Buffer;
use kakikotoba_layout::Font;

use editor::Editor;

// ponytail: 明朝の候補を固定。フォント設定を作るときに選択可能に
fn find_font() -> Result<String, String> {
    // 単に serif:lang=ja で探すと、中国語フォントが選ばれる環境がある
    let output = Command::new("fc-match")
        .args(["-f", "%{file}", "Noto Serif CJK JP,Source Han Serif JP,IPAexMincho,IPAMincho,serif:lang=ja"])
        .output()
        .map_err(|e| format!("fc-match を実行できません: {e}"))?;
    String::from_utf8(output.stdout)
        .ok()
        .filter(|path| !path.is_empty())
        .ok_or_else(|| "日本語フォントが見つかりません".to_owned())
}

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
    let font_path = find_font()?;
    let data = fs::read(&font_path).map_err(|e| format!("{font_path}: {e}"))?;
    // フォントはアプリ終了まで使い続けるので解放しない
    // ponytail: QRawFontがコレクションの先頭しか読めないため番号は0固定。先頭以外を使うならQRawFontの読み込みごと見直す
    let font = Font::new(Box::leak(data.into_boxed_slice()), 0)
        .ok_or_else(|| format!("{font_path}: フォントを読めません"))?;
    Ok(Editor::new(buffer, path, font, font_path))
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
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/kakikotoba/qml/main.qml"));
    }
    match app.as_mut().map(|app| app.exec()) {
        Some(0) => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

//! cxx-qtによるQt統合、UI

mod editor;
mod item;
mod page_view;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{fs, io};

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};
use kakikotoba_core::Buffer;
use kakikotoba_layout::Font;

use editor::{Editor, Fonts};

// 単に serif:lang=ja で探すと、中国語フォントが選ばれる環境がある
// ponytail: 候補を固定。フォント設定を作るときに選択可能に
const SERIF_PATTERN: &str = "Noto Serif CJK JP,Source Han Serif JP,IPAexMincho,IPAMincho,serif:lang=ja";
const SANS_PATTERN: &str = "Noto Sans CJK JP,Source Han Sans JP,IPAexGothic,IPAGothic,sans:lang=ja:bold";

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
    let (serif, serif_path) = load_font(SERIF_PATTERN)?;
    let (sans, sans_path) = load_font(SANS_PATTERN)?;
    Ok(Editor::new(buffer, path, Fonts { serif, serif_path, sans, sans_path }))
}

fn load_font(pattern: &str) -> Result<(Font<'static>, String), String> {
    let path = find_font(pattern)?;
    let data = fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    // フォントはアプリ終了まで使い続けるので解放しない
    // ponytail: QRawFontがコレクションの先頭しか読めないため番号は0固定。先頭以外を使うならQRawFontの読み込みごと見直す
    let font = Font::new(Box::leak(data.into_boxed_slice()), 0).ok_or_else(|| format!("{path}: フォントを読めません"))?;
    Ok((font, path))
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

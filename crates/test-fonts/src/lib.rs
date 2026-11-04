//! テストが読む実フォントの場所。ディストリビューションごとにパスが違うため、fontconfigで探す。

use std::process::Command;

/// 本文用の明朝
pub const SERIF_CJK: &str = "Noto Serif CJK JP:style=Regular";
/// 本文用と別のフォントへの切り替えを確かめる用
pub const SANS_CJK_BOLD: &str = "Noto Sans CJK JP:style=Bold";
/// 日本語を持たないフォント
pub const LATIN_SERIF: &str = "DejaVu Serif:style=Book";

/// fontconfigのパターンに合うフォントファイルのパス。コレクション（.ttc）なら先頭の書体であるものに限る。
/// 見つからなければ失敗させる（テストを何も確かめずに通さない）。
pub fn path(pattern: &str) -> String {
    let output = Command::new("fc-list")
        .args(["-f", "%{index}\t%{file}\n", pattern])
        .output()
        .unwrap_or_else(|e| panic!("fc-list を実行できない（fontconfig を入れる）: {e}"));
    let list = String::from_utf8_lossy(&output.stdout);
    let mut paths: Vec<&str> = list.lines().filter_map(|line| line.strip_prefix("0\t")).collect();
    paths.sort_unstable();
    paths.first().map(|path| path.to_string()).unwrap_or_else(|| {
        panic!("テスト用のフォントがない: {pattern}（Noto CJK と DejaVu を入れる。パッケージ名はREADMEの「開発」を参照）")
    })
}

/// フォントファイルを読む。`Font` が `'static` を要るテスト向けに、読んだデータは解放しない。
pub fn read(pattern: &str) -> &'static [u8] {
    let path = path(pattern);
    let data = std::fs::read(&path).unwrap_or_else(|e| panic!("{path} を読めない: {e}"));
    Box::leak(data.into_boxed_slice())
}

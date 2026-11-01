# kakikotoba

小説執筆向けの、Linux 用縦書きテキストエディタです。テキストファイルを縦書きで編集し、PreTNF と Markdown を縦組みのページとして確認できます。

ファイルの編集に専念するため、PDF や EPUB の出力機能は持ちません。

## 主な機能

**編集モード**

- 縦書きでの編集（縦用字形、禁則処理、句読点のぶら下げ）
- 縦中横、欧文の90°回転、英単語の折り返し時のハイフン表示
- IME 入力（変換中の文字も縦書きで表示）
- 記法ハイライト（PreTNF、Markdown）
- 本文の検索（置換はありません）、Undo/Redo、コピーと貼り付け

**ページモード（閲覧専用）**

- 字数×行数を指定した版面での見開き表示（擬A6、擬B6）
- ルビ、傍点、傍線、章見出し、挿絵など、記法を反映した組版
- 文書情報（タイトル、筆者など）の表示
- 編集位置とページの相互ジャンプ

## 動作環境

Linux 専用です。CachyOS の KDE Plasma 6（Wayland）と fcitx5 で動作を確認しています。

必要なもの:

- Rust（1.92 で確認。edition 2024）
- Qt 6（Base、Declarative）
- KDE Kirigami
- fontconfig（`fc-match` でフォントを探します）
- 日本語フォント（Noto CJK、源ノ、IPA のいずれかを優先します）

Arch Linux 系でのインストール例:

```sh
sudo pacman -S --needed rust qt6-base qt6-declarative kirigami fontconfig noto-fonts-cjk
```

## ビルドと起動

```sh
git clone https://github.com/kabeuchi-bird/kakikotoba.git
cd kakikotoba
cargo run --release -p kakikotoba            # 新規文書で起動
cargo run --release -p kakikotoba -- 本文.txt # ファイルを開いて起動
```

指定したファイルが存在しない場合は、空の文書として開き、保存時に作成します。

## 対応ファイル

文字コードは UTF-8 のみです。Shift_JIS などのファイルは開けません。

記法は拡張子で切り替わります。

| 拡張子 | 記法 |
|---|---|
| `.md` | Markdown（`｜漢字《かんじ》` のルビも有効） |
| それ以外 | [PreTNF](https://www.sylphid.jp/~tabito/Tokimi/PreTNF_Reference.pdf)（なろう、カクヨム、Pixiv のルビ等の記法を含みます） |

PreTNF の半角記号の記法（行頭の `!` や `*`、行内の `"〜"` など）は通常の文章と衝突することがあるため、「表示」メニューの「PreTNFの記号形式」でオフにできます。

## 操作

| キー | 操作 |
|---|---|
| Ctrl+N / Ctrl+O | 新規 / 開く |
| Ctrl+S / Ctrl+Shift+S | 保存 / 名前を付けて保存 |
| Ctrl+Q | 終了 |
| Ctrl+Z / Ctrl+Y（Ctrl+Shift+Z） | 元に戻す / やり直す |
| Ctrl+X / Ctrl+C / Ctrl+V | 切り取り / コピー / 貼り付け |
| Ctrl+A | すべて選択 |
| Ctrl+F | 本文を検索（Enter で次、Shift+Enter で前、Esc で閉じる） |
| Ctrl+Alt+I | コマンドを検索 |
| F5 | ページモードと編集モードの切り替え |

カーソルは縦書きに合わせて動きます。↑↓で1文字、←→で1列移動します。

fcitx5 で変換するときは、候補をウィンドウ内に縦書きで表示します。↑↓で文節を移動し、←で次の候補、→で前の候補を選びます（クリックでも選べます）。hazkey では↑↓がページ送りになります。fcitx5 がない環境では、候補窓とキーの割り当ては IME の標準のままです。

ページモードの操作:

| 操作 | 動作 |
|---|---|
| ←、PageDown、Space、ホイール下 | 次の見開き |
| →、PageUp、ホイール上 | 前の見開き |
| Home / End | 先頭 / 末尾 |
| クリック | その位置の編集モードへ戻る |
| Esc | 編集モードへ戻る |

メニューは KDE Plasma のグローバルメニューにも表示されます。ページ設定と文書情報は「表示」メニューから開きます。

## 既知の制限

- ページモードでは、欧文は回転せず、句読点もぶら下げません
- コードブロックや表は折り返さず、版面からはみ出た分は表示されません
- グローバルメニューの検索欄では IME を使えません（Ctrl+Alt+I のコマンド検索を使ってください）
- ウィンドウ内メニューとグローバルメニューは、当面両方表示されます
- 候補窓はウィンドウの外に出せません。本文のフォントにない字（一部の絵文字など）は豆腐になります
- 非常に長い文書では、入力時に全文のレイアウトし直しによる遅れが出ることがあります

## 開発

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
```

| クレート | 役割 |
|---|---|
| `crates/core` | テキストバッファ、Undo/Redo |
| `crates/layout` | 縦組みレイアウト、ページ組版 |
| `crates/markup` | PreTNF、Markdown のパーサと記法ハイライト |
| `crates/app` | Qt 統合と UI（cxx-qt、QML、Kirigami） |

仕様は [CLAUDE.md](CLAUDE.md) にまとめています。

## ライセンス

[MIT](LICENSE)

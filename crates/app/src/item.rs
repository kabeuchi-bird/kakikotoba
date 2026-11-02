//! 縦書き編集領域のQtアイテム。Qtのイベントを `Editor` の操作へ変換し、描画する。

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;

use cxx::UniquePtr;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QColor, QRectF, QString, QUrl, QVariant};
use kakikotoba_layout::Rect;
use kakikotoba_layout::page::{PageFormat, PageSettings, SpreadMode};

use crate::editor::{Editor, Fonts, VisibleGlyph};
use crate::fcitx::{self, NativeKey, QtKey};

#[cxx_qt::bridge]
pub mod qobject {
    #[namespace = "Qt"]
    #[repr(u32)]
    enum InputMethodQuery {
        ImEnabled = 0x1,
        ImCursorRectangle = 0x2,
        ImCursorPosition = 0x8,
        ImAnchorPosition = 0x80,
        ImHints = 0x100,
    }

    #[namespace = "Qt"]
    unsafe extern "C++" {
        type InputMethodQuery;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qcolor.h");
        type QColor = cxx_qt_lib::QColor;
        include!("cxx-qt-lib/qpainter.h");
        type QPainter = cxx_qt_lib::QPainter;
        include!("cxx-qt-lib/qsizef.h");
        type QSizeF = cxx_qt_lib::QSizeF;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("shim.h");
        type QQuickPaintedItem;
        type QKeyEvent;
        type QMouseEvent;
        type QWheelEvent;
        type QInputMethodEvent;
        type QRawFont;
    }

    #[namespace = "shim"]
    unsafe extern "C++" {
        fn init_item(item: Pin<&mut VerticalEditor>);
        fn focus_item(item: Pin<&mut VerticalEditor>);
        fn update_input_method();
        fn clipboard_text() -> QString;
        fn set_clipboard_text(text: &QString);
        fn new_raw_font(path: &QString, pixel_size: f64) -> UniquePtr<QRawFont>;
        fn draw_glyphs(
            painter: Pin<&mut QPainter>,
            font: &QRawFont,
            color: &QColor,
            ids: &[u32],
            positions: &[f32],
        );
        fn draw_glyphs_rotated(
            painter: Pin<&mut QPainter>,
            font: &QRawFont,
            color: &QColor,
            ids: &[u32],
            positions: &[f32],
        );
        fn fill_rect(painter: Pin<&mut QPainter>, x: f32, y: f32, width: f32, height: f32, color: &QColor);
        fn draw_wave(painter: Pin<&mut QPainter>, x: f32, y: f32, width: f32, height: f32, color: &QColor);
        fn draw_image(
            painter: Pin<&mut QPainter>,
            path: &QString,
            x: f32,
            y: f32,
            width: f32,
            height: f32,
            frame: &QColor,
        );
        fn key_code(event: &QKeyEvent) -> i32;
        fn key_modifiers(event: &QKeyEvent) -> i32;
        fn key_text(event: &QKeyEvent) -> QString;
        fn ignore_key(event: Pin<&mut QKeyEvent>);
        fn key_native_sym(event: &QKeyEvent) -> u32;
        fn key_native_code(event: &QKeyEvent) -> u32;
        fn key_native_state(event: &QKeyEvent) -> u32;
        fn key_time(event: &QKeyEvent) -> u32;
        fn key_is_auto_repeat(event: &QKeyEvent) -> bool;
        fn mouse_x(event: &QMouseEvent) -> f32;
        fn mouse_y(event: &QMouseEvent) -> f32;
        fn mouse_left_pressed(event: &QMouseEvent) -> bool;
        fn mouse_shift(event: &QMouseEvent) -> bool;
        fn wheel_delta(event: &QWheelEvent) -> f32;
        fn im_preedit(event: &QInputMethodEvent) -> QString;
        fn im_commit(event: &QInputMethodEvent) -> QString;
        fn im_replacement_start(event: &QInputMethodEvent) -> i32;
        fn im_replacement_length(event: &QInputMethodEvent) -> i32;
        fn im_cursor(event: &QInputMethodEvent) -> i32;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = QQuickPaintedItem]
        #[qml_element]
        #[qproperty(QColor, text_color, cxx_name = "textColor")]
        #[qproperty(QColor, selection_color, cxx_name = "selectionColor")]
        #[qproperty(QColor, delimiter_color, cxx_name = "delimiterColor")]
        #[qproperty(QColor, tag_color, cxx_name = "tagColor")]
        #[qproperty(QColor, ruby_color, cxx_name = "rubyColor")]
        #[qproperty(QColor, emphasis_color, cxx_name = "emphasisColor")]
        #[qproperty(QColor, sideline_color, cxx_name = "sidelineColor")]
        #[qproperty(QColor, comment_color, cxx_name = "commentColor")]
        #[qproperty(QColor, strong_color, cxx_name = "strongColor")]
        #[qproperty(QColor, code_color, cxx_name = "codeColor")]
        #[qproperty(QColor, paper_color, cxx_name = "paperColor")]
        #[qproperty(bool, page_mode, cxx_name = "pageMode")]
        #[qproperty(QString, page_label, cxx_name = "pageLabel")]
        #[qproperty(bool, can_undo, cxx_name = "canUndo")]
        #[qproperty(bool, can_redo, cxx_name = "canRedo")]
        #[qproperty(bool, has_selection, cxx_name = "hasSelection")]
        #[qproperty(bool, modified)]
        #[qproperty(bool, has_file, cxx_name = "hasFile")]
        #[qproperty(bool, composing)]
        #[qproperty(QString, document_name, cxx_name = "documentName")]
        type VerticalEditor = super::VerticalEditorRust;

        #[inherit]
        fn size(self: &VerticalEditor) -> QSizeF;
        #[inherit]
        fn update(self: Pin<&mut VerticalEditor>);

        #[qinvokable]
        #[cxx_name = "newDocument"]
        fn new_document(self: Pin<&mut VerticalEditor>);
        #[qinvokable]
        fn undo(self: Pin<&mut VerticalEditor>);
        #[qinvokable]
        fn redo(self: Pin<&mut VerticalEditor>);
        #[qinvokable]
        fn cut(self: Pin<&mut VerticalEditor>);
        #[qinvokable]
        fn copy(self: Pin<&mut VerticalEditor>);
        #[qinvokable]
        fn paste(self: Pin<&mut VerticalEditor>);
        #[qinvokable]
        #[cxx_name = "selectAll"]
        fn select_all(self: Pin<&mut VerticalEditor>);
        /// 一致を選択できたらtrue
        #[qinvokable]
        #[cxx_name = "findText"]
        fn find_text(self: Pin<&mut VerticalEditor>, query: &QString, backward: bool) -> bool;
        #[qinvokable]
        #[cxx_name = "selectedText"]
        fn selected_text(self: &VerticalEditor) -> QString;

        // 以下の3つは失敗時にエラーメッセージ、成功時に空文字列を返す
        #[qinvokable]
        fn save(self: Pin<&mut VerticalEditor>) -> QString;
        #[qinvokable]
        #[cxx_name = "saveAs"]
        fn save_as(self: Pin<&mut VerticalEditor>, url: &QUrl) -> QString;
        #[qinvokable]
        fn open(self: Pin<&mut VerticalEditor>, url: &QUrl) -> QString;

        #[qinvokable]
        #[cxx_name = "setHighlightOptions"]
        fn set_highlight_options(self: Pin<&mut VerticalEditor>, enabled: bool, symbol_forms: bool);

        #[qinvokable]
        #[cxx_name = "togglePageMode"]
        fn toggle_page_mode(self: Pin<&mut VerticalEditor>);

        /// `format` は0が擬A6、1が擬B6。`spread` は0が左ページ始まり、1が最初から見開き
        #[qinvokable]
        #[cxx_name = "setPageOptions"]
        fn set_page_options(self: Pin<&mut VerticalEditor>, format: i32, chars: i32, lines: i32, spread: i32);

        #[qinvokable]
        #[cxx_name = "metadataText"]
        fn metadata_text(self: &VerticalEditor) -> QString;

        #[qinvokable]
        #[cxx_name = "setImeFocused"]
        fn set_ime_focused(self: Pin<&mut VerticalEditor>, focused: bool);

        #[cxx_override]
        unsafe fn paint(self: Pin<&mut VerticalEditor>, painter: *mut QPainter);
        #[cxx_override]
        #[cxx_name = "keyPressEvent"]
        unsafe fn key_press_event(self: Pin<&mut VerticalEditor>, event: *mut QKeyEvent);
        #[cxx_override]
        #[cxx_name = "keyReleaseEvent"]
        unsafe fn key_release_event(self: Pin<&mut VerticalEditor>, event: *mut QKeyEvent);
        #[cxx_override]
        #[cxx_name = "mousePressEvent"]
        unsafe fn mouse_press_event(self: Pin<&mut VerticalEditor>, event: *mut QMouseEvent);
        #[cxx_override]
        #[cxx_name = "mouseMoveEvent"]
        unsafe fn mouse_move_event(self: Pin<&mut VerticalEditor>, event: *mut QMouseEvent);
        #[cxx_override]
        #[cxx_name = "wheelEvent"]
        unsafe fn wheel_event(self: Pin<&mut VerticalEditor>, event: *mut QWheelEvent);
        #[cxx_override]
        #[cxx_name = "inputMethodEvent"]
        unsafe fn input_method_event(self: Pin<&mut VerticalEditor>, event: *mut QInputMethodEvent);
        #[cxx_override]
        #[cxx_name = "inputMethodQuery"]
        fn input_method_query(self: &VerticalEditor, query: InputMethodQuery) -> QVariant;
    }

    impl cxx_qt::Initialize for VerticalEditor {}
    impl cxx_qt::Threading for VerticalEditor {}
}

const KEY_BACKSPACE: i32 = 0x0100_0003;
const KEY_RETURN: i32 = 0x0100_0004;
const KEY_ENTER: i32 = 0x0100_0005;
const KEY_DELETE: i32 = 0x0100_0007;
const KEY_HOME: i32 = 0x0100_0010;
const KEY_END: i32 = 0x0100_0011;
const KEY_LEFT: i32 = 0x0100_0012;
const KEY_UP: i32 = 0x0100_0013;
const KEY_RIGHT: i32 = 0x0100_0014;
const KEY_DOWN: i32 = 0x0100_0015;
const KEY_PAGE_UP: i32 = 0x0100_0016;
const KEY_PAGE_DOWN: i32 = 0x0100_0017;
const KEY_SPACE: i32 = 0x20;
const SHIFT: i32 = 0x0200_0000;
const CONTROL: i32 = 0x0400_0000;
const ALT: i32 = 0x0800_0000;

thread_local! {
    static STARTUP_EDITOR: RefCell<Option<Editor>> = const { RefCell::new(None) };
}

/// QMLがアイテムを生成するときに受け取る `Editor` を置く。
pub fn set_startup_editor(editor: Editor) {
    STARTUP_EDITOR.set(Some(editor));
}

/// `set_startup_editor` で置かれた `Editor` を、アイテムの生成時に受け取る。
struct StartupEditor(Editor);

impl Default for StartupEditor {
    fn default() -> Self {
        // ponytail: 1ウィンドウ1文書。複数文書を開くならアイテムのプロパティで渡す
        Self(STARTUP_EDITOR.take().expect("VerticalEditorは1つだけ生成する"))
    }
}

#[derive(Default)]
pub struct VerticalEditorRust {
    text_color: QColor,
    selection_color: QColor,
    delimiter_color: QColor,
    tag_color: QColor,
    ruby_color: QColor,
    emphasis_color: QColor,
    sideline_color: QColor,
    comment_color: QColor,
    strong_color: QColor,
    code_color: QColor,
    paper_color: QColor,
    can_undo: bool,
    can_redo: bool,
    has_selection: bool,
    page_mode: bool,
    page_label: QString,
    modified: bool,
    has_file: bool,
    composing: bool,
    document_name: QString,
    editor: StartupEditor,
    /// `(ゴシックか, 字サイズの1/64px)` ごとのフォント
    raw_fonts: HashMap<(bool, u32), UniquePtr<qobject::QRawFont>>,
    /// ページモードからのクリックで編集モードに戻った後、そのドラッグを選択にしない
    ignore_drag: bool,
    /// fcitx5に接続中だけある。ないときはQtのIM経路を使う
    ime: Option<fcitx::Client>,
    /// QMLのアイテムがフォーカスを持っているか
    has_focus: bool,
    /// fcitx5へFocusInを送った状態か
    ime_focused: bool,
}

impl cxx_qt::Initialize for qobject::VerticalEditor {
    fn initialize(mut self: Pin<&mut Self>) {
        qobject::init_item(self.as_mut());
        self.as_mut().sync_properties();
        let thread = self.qt_thread();
        fcitx::spawn(move |event| {
            // アイテムが破棄された後は届けなくてよい
            let _ = thread.queue(move |item| item.handle_ime_event(event));
        });
    }
}

/// 同じ字サイズと向きのグリフをまとめて描く。
fn draw_visible_glyphs(
    raw_fonts: &mut HashMap<(bool, u32), UniquePtr<qobject::QRawFont>>,
    fonts: &Fonts,
    mut painter: Pin<&mut qobject::QPainter>,
    glyphs: impl Iterator<Item = VisibleGlyph>,
    color_of: impl Fn(u8) -> QColor,
) {
    // 色、字サイズ、向きごとにまとめて描く
    let mut batches: GlyphBatches<(u8, u32, bool)> = HashMap::new();
    for glyph in glyphs {
        let (ids, positions) = batches.entry((glyph.color, glyph.size.to_bits(), glyph.rotated)).or_default();
        ids.push(glyph.id);
        positions.extend([glyph.x, glyph.y]);
    }
    for ((color, size, rotated), (ids, positions)) in &batches {
        let font = raw_font(raw_fonts, fonts, false, f32::from_bits(*size));
        let color = color_of(*color);
        if *rotated {
            qobject::draw_glyphs_rotated(painter.as_mut(), font, &color, ids, positions);
        } else {
            qobject::draw_glyphs(painter.as_mut(), font, &color, ids, positions);
        }
    }
}

/// ページモードの見開きを描く。
fn paint_pages(rust: &mut VerticalEditorRust, mut painter: Pin<&mut qobject::QPainter>, width: f32, height: f32) {
    let view = rust.editor.0.page_view().expect("呼び出し元で確認済み");
    let em = view.em(width, height);
    for placed in view.placed_pages(width, height) {
        let paper = view.page_rect(&placed, em);
        qobject::fill_rect(painter.as_mut(), paper.x, paper.y, paper.width, paper.height, &rust.paper_color);
        let Some(page) = placed.page.map(|page| &view.document.pages[page]) else {
            continue;
        };
        let to_screen = |r: &Rect| (placed.x + r.x * em, placed.y + r.y * em, r.width * em, r.height * em);
        if let Some(name) = &page.image {
            let path = QString::from(view.image_path(name).to_string_lossy().as_ref());
            qobject::draw_image(painter.as_mut(), &path, paper.x, paper.y, paper.width, paper.height, &rust.text_color);
        }
        for line in &page.lines {
            let (x, y, w, h) = to_screen(line);
            qobject::fill_rect(painter.as_mut(), x, y, w.max(1.0), h, &rust.text_color);
        }
        for wave in &page.waves {
            let (x, y, w, h) = to_screen(wave);
            qobject::draw_wave(painter.as_mut(), x, y, w, h, &rust.text_color);
        }
        // 字サイズとフォントごとにまとめて描く
        let mut batches: GlyphBatches<(bool, u32)> = HashMap::new();
        for glyph in &page.glyphs {
            let (ids, positions) = batches.entry((glyph.sans, glyph.scale.to_bits())).or_default();
            ids.push(glyph.id as u32);
            positions.extend([placed.x + glyph.x * em, placed.y + glyph.y * em]);
        }
        for ((sans, scale), (ids, positions)) in &batches {
            let font = raw_font(&mut rust.raw_fonts, &rust.editor.0.fonts, *sans, f32::from_bits(*scale) * em);
            qobject::draw_glyphs(painter.as_mut(), font, &rust.text_color, ids, positions);
        }
    }
    // のど（綴じ目）
    let page_height = view.document.page_height * em;
    let top = (height - page_height) / 2.0;
    qobject::fill_rect(painter.as_mut(), width / 2.0, top, 1.0, page_height, &rust.delimiter_color);
}

/// Qtのキーイベントからfcitx5へ送るキーを作る。合成イベントにはネイティブの情報がない。
fn native_key(event: &qobject::QKeyEvent, composing: bool) -> Option<NativeKey> {
    let sym = qobject::key_native_sym(event);
    (sym != 0).then(|| {
        let native = NativeKey {
            sym,
            code: qobject::key_native_code(event),
            state: qobject::key_native_state(event),
            time: qobject::key_time(event),
        };
        if composing { fcitx::remap_composing_key(native) } else { native }
    })
}

fn local_path(url: &QUrl) -> Result<PathBuf, String> {
    let path = url.to_local_file().ok_or("ローカルファイルではありません")?;
    Ok(PathBuf::from(String::from(&path)))
}

fn utf16_to_chars(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    text.chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= utf16
        })
        .count()
}

const RAW_FONT_CACHE_LIMIT: usize = 64;
/// キーごとのグリフ番号と位置（x, y の交互の並び）
type GlyphBatches<K> = HashMap<K, (Vec<u32>, Vec<f32>)>;

/// 字サイズごとのフォントを返す。ウィンドウの拡縮で増えるので、上限を超えたら捨てる。
fn raw_font<'a>(
    cache: &'a mut HashMap<(bool, u32), UniquePtr<qobject::QRawFont>>,
    fonts: &Fonts,
    sans: bool,
    size: f32,
) -> &'a qobject::QRawFont {
    if cache.len() >= RAW_FONT_CACHE_LIMIT {
        cache.clear();
    }
    let key = (sans, (size * 64.0).round() as u32);
    let font = cache.entry(key).or_insert_with(|| {
        let path = if sans { &fonts.sans_path } else { &fonts.serif_path };
        qobject::new_raw_font(&QString::from(path.as_str()), size as f64)
    });
    font.as_ref().expect("生成済み")
}

impl qobject::VerticalEditor {
    fn editor_mut(self: Pin<&mut Self>) -> &mut Editor {
        &mut self.rust_mut().get_mut().editor.0
    }

    fn sync_properties(mut self: Pin<&mut Self>) {
        let editor = &self.rust().editor.0;
        let (modified, has_file) = (editor.is_modified(), editor.has_file());
        let name = QString::from(editor.document_name().as_str());
        let (can_undo, can_redo, has_selection) = (editor.can_undo(), editor.can_redo(), editor.has_selection());
        let page_mode = editor.page_view().is_some();
        let composing = editor.is_composing();
        let page_label = QString::from(editor.page_view().map_or(String::new(), |view| view.label()).as_str());
        self.as_mut().set_can_undo(can_undo);
        self.as_mut().set_can_redo(can_redo);
        self.as_mut().set_has_selection(has_selection);
        self.as_mut().set_page_mode(page_mode);
        self.as_mut().set_page_label(page_label);
        self.as_mut().set_modified(modified);
        self.as_mut().set_has_file(has_file);
        self.as_mut().set_composing(composing);
        self.as_mut().set_document_name(name);
    }

    /// 操作の結果を画面、QMLのプロパティ、IMEへ反映する。
    fn refresh(mut self: Pin<&mut Self>) {
        self.as_mut().sync_properties();
        self.as_mut().sync_ime_focus();
        self.update();
        qobject::update_input_method();
    }

    /// ページモードは閲覧専用なので、fcitx5から見るとフォーカスを失った状態にする。
    fn sync_ime_focus(self: Pin<&mut Self>) {
        let rust = self.rust_mut().get_mut();
        let wanted = rust.has_focus && rust.editor.0.page_view().is_none();
        if let Some(ime) = rust.ime.as_ref().filter(|_| wanted != rust.ime_focused) {
            if wanted {
                ime.focus_in();
            } else {
                ime.focus_out();
            }
            rust.ime_focused = wanted;
        }
    }

    fn set_ime_focused(mut self: Pin<&mut Self>, focused: bool) {
        self.as_mut().rust_mut().get_mut().has_focus = focused;
        self.refresh();
    }

    fn handle_ime_event(mut self: Pin<&mut Self>, event: fcitx::Event) {
        match event {
            fcitx::Event::Connected(client) => {
                let rust = self.as_mut().rust_mut().get_mut();
                rust.ime = Some(client);
                rust.ime_focused = false;
            }
            fcitx::Event::Disconnected => {
                let rust = self.as_mut().rust_mut().get_mut();
                rust.ime = None;
                rust.editor.0.set_preedit("", None);
                rust.editor.0.set_candidates(Default::default());
            }
            fcitx::Event::Preedit(segments, cursor) => {
                self.as_mut().editor_mut().set_formatted_preedit(&segments, cursor);
            }
            fcitx::Event::Panel(panel) => self.as_mut().editor_mut().set_candidates(panel),
            fcitx::Event::Commit(text) => self.as_mut().editor_mut().commit(0, 0, &text),
            fcitx::Event::Unhandled(key) => {
                self.as_mut().handle_edit_key(&key);
            }
        }
        self.refresh();
    }

    fn set_highlight_options(mut self: Pin<&mut Self>, enabled: bool, symbol_forms: bool) {
        self.as_mut().editor_mut().set_highlight_options(enabled, symbol_forms);
        self.refresh();
    }

    /// 編集できる状態なら操作を実行し、結果を反映する。
    fn edit_command(mut self: Pin<&mut Self>, command: impl FnOnce(&mut Editor)) {
        let editor = self.as_mut().editor_mut();
        if editor.is_editable() {
            command(editor);
        }
        self.refresh();
    }

    fn new_document(mut self: Pin<&mut Self>) {
        self.reset_ime();
        self.as_mut().editor_mut().new_document();
        self.refresh();
    }

    /// 文書を置き換える前に、未確定の文字を捨てる。
    fn reset_ime(&self) {
        if let Some(ime) = &self.rust().ime {
            ime.reset();
        }
    }

    fn undo(self: Pin<&mut Self>) {
        self.edit_command(Editor::undo);
    }

    fn redo(self: Pin<&mut Self>) {
        self.edit_command(Editor::redo);
    }

    fn cut(self: Pin<&mut Self>) {
        self.edit_command(|editor| {
            if let Some(text) = editor.selected_text() {
                qobject::set_clipboard_text(&QString::from(text.as_str()));
                editor.delete();
            }
        });
    }

    fn copy(self: Pin<&mut Self>) {
        self.edit_command(|editor| {
            if let Some(text) = editor.selected_text() {
                qobject::set_clipboard_text(&QString::from(text.as_str()));
            }
        });
    }

    fn paste(self: Pin<&mut Self>) {
        self.edit_command(|editor| {
            let text = String::from(&qobject::clipboard_text());
            if !text.is_empty() {
                editor.insert(&text);
            }
        });
    }

    fn select_all(self: Pin<&mut Self>) {
        self.edit_command(Editor::select_all);
    }

    fn find_text(mut self: Pin<&mut Self>, query: &QString, backward: bool) -> bool {
        let query = String::from(query);
        let mut found = false;
        self.as_mut().edit_command(|editor| found = editor.find(&query, backward));
        found
    }

    fn selected_text(&self) -> QString {
        QString::from(self.rust().editor.0.selected_text().unwrap_or_default().as_str())
    }

    fn toggle_page_mode(mut self: Pin<&mut Self>) {
        self.as_mut().editor_mut().toggle_page_mode();
        self.refresh();
    }

    fn set_page_options(mut self: Pin<&mut Self>, format: i32, chars: i32, lines: i32, spread: i32) {
        let settings = PageSettings {
            format: if format == 1 { PageFormat::B6 } else { PageFormat::A6 },
            chars_per_line: chars.max(1) as usize,
            lines_per_page: lines.max(1) as usize,
            spread: if spread == 1 { SpreadMode::FullSpread } else { SpreadMode::LeftStart },
        };
        self.as_mut().editor_mut().set_page_settings(settings);
        self.refresh();
    }

    fn metadata_text(&self) -> QString {
        QString::from(self.rust().editor.0.metadata_text().as_str())
    }

    /// ファイル操作の結果をQMLへ返す形にする。
    fn finish_file_operation(self: Pin<&mut Self>, result: Result<(), String>) -> QString {
        self.refresh();
        QString::from(result.err().unwrap_or_default().as_str())
    }

    fn save(mut self: Pin<&mut Self>) -> QString {
        let result = self.as_mut().editor_mut().save();
        self.finish_file_operation(result.map_err(|e| format!("保存できません: {e}")))
    }

    fn save_as(mut self: Pin<&mut Self>, url: &QUrl) -> QString {
        let result = local_path(url)
            .and_then(|path| self.as_mut().editor_mut().save_as(path).map_err(|e| e.to_string()))
            .map_err(|e| format!("保存できません: {e}"));
        self.finish_file_operation(result)
    }

    fn open(mut self: Pin<&mut Self>, url: &QUrl) -> QString {
        self.reset_ime();
        let result = local_path(url)
            .and_then(|path| self.as_mut().editor_mut().open(path).map_err(|e| e.to_string()))
            .map_err(|e| format!("開けません: {e}"));
        self.finish_file_operation(result)
    }

    unsafe fn paint(mut self: Pin<&mut Self>, painter: *mut qobject::QPainter) {
        // SAFETY: Qtは描画中有効なQPainterを渡す
        let mut painter = unsafe { Pin::new_unchecked(&mut *painter) };
        let size = self.size();
        let rust = self.as_mut().rust_mut().get_mut();
        let (width, height) = (size.width() as f32, size.height() as f32);
        rust.editor.0.resize(width, height);

        if rust.editor.0.page_view().is_some() {
            paint_pages(rust, painter.as_mut(), width, height);
            return;
        }

        let mut fill = |rect: Rect, color: &QColor| {
            qobject::fill_rect(painter.as_mut(), rect.x, rect.y, rect.width, rect.height, color);
        };
        for rect in rust.editor.0.selection_rects() {
            fill(rect, &rust.selection_color);
        }
        for rect in rust.editor.0.preedit_lines() {
            fill(rect, &rust.text_color);
        }
        fill(rust.editor.0.caret_rect(), &rust.text_color);

        for rect in rust.editor.0.preedit_focus_rects() {
            qobject::fill_rect(painter.as_mut(), rect.x, rect.y, rect.width, rect.height, &rust.selection_color);
        }

        // 色属性の値は `Kind` の並びに1を足したもの。0は既定の文字色
        let palette = [
            &rust.text_color,
            &rust.delimiter_color,
            &rust.tag_color,
            &rust.ruby_color,
            &rust.emphasis_color,
            &rust.sideline_color,
            &rust.comment_color,
            &rust.strong_color,
            &rust.code_color,
        ];
        draw_visible_glyphs(
            &mut rust.raw_fonts,
            &rust.editor.0.fonts,
            painter.as_mut(),
            rust.editor.0.visible_glyphs(),
            |color| palette[color as usize].clone(),
        );

        if let Some(panel) = rust.editor.0.candidate_paint() {
            let frame = panel.background;
            qobject::fill_rect(painter.as_mut(), frame.x, frame.y, frame.width, frame.height, &rust.paper_color);
            for rect in panel.selected {
                qobject::fill_rect(painter.as_mut(), rect.x, rect.y, rect.width, rect.height, &rust.selection_color);
            }
            draw_visible_glyphs(
                &mut rust.raw_fonts,
                &rust.editor.0.fonts,
                painter.as_mut(),
                panel.glyphs.into_iter(),
                |_| rust.text_color.clone(),
            );
        }
    }

    unsafe fn key_press_event(mut self: Pin<&mut Self>, event: *mut qobject::QKeyEvent) {
        // SAFETY: Qtは配信中有効なイベントを渡す
        let mut event = unsafe { Pin::new_unchecked(&mut *event) };
        let key = qobject::key_code(&event);
        let modifiers = qobject::key_modifiers(&event);
        let text = String::from(&qobject::key_text(&event));
        let editor = self.as_mut().editor_mut();

        if let Some(view) = editor.page_view_mut() {
            // 閲覧専用なので、見開きの移動以外は受け付けない
            let handled = match key {
                KEY_LEFT | KEY_PAGE_DOWN | KEY_SPACE => { view.next_spread(); true }
                KEY_RIGHT | KEY_PAGE_UP => { view.previous_spread(); true }
                KEY_HOME => { view.first_spread(); true }
                KEY_END => { view.last_spread(); true }
                _ => false,
            };
            if handled {
                self.refresh();
            } else {
                qobject::ignore_key(event.as_mut());
            }
            return;
        }

        let composing = editor.is_composing();
        let key = QtKey { key, modifiers, text };
        let native = native_key(&event, composing);
        let key = match (&self.rust().ime, native) {
            (Some(ime), Some(native)) => match ime.press(native, key) {
                // 処理されなければ `Event::Unhandled` で戻ってくる
                Ok(()) => return,
                Err(key) => key,
            },
            _ => key,
        };
        if !self.handle_edit_key(&key) {
            qobject::ignore_key(event.as_mut());
        }
    }

    unsafe fn key_release_event(self: Pin<&mut Self>, event: *mut qobject::QKeyEvent) {
        // SAFETY: Qtは配信中有効なイベントを渡す
        let mut event = unsafe { Pin::new_unchecked(&mut *event) };
        let editor = &self.rust().editor.0;
        let native = native_key(&event, editor.is_composing()).filter(|_| editor.page_view().is_none());
        match (&self.rust().ime, native) {
            (Some(ime), Some(native)) if !qobject::key_is_auto_repeat(&event) => ime.release(native),
            _ => qobject::ignore_key(event.as_mut()),
        }
    }

    /// 編集モードのキー操作。処理したらtrue。
    fn handle_edit_key(mut self: Pin<&mut Self>, key: &QtKey) -> bool {
        let QtKey { key, modifiers, text } = key;
        let extend = modifiers & SHIFT != 0;
        let editor = self.as_mut().editor_mut();
        let handled = match *key {
            _ if editor.page_view().is_some() || editor.is_composing() => false,
            KEY_UP => { editor.move_up(extend); true }
            KEY_DOWN => { editor.move_down(extend); true }
            KEY_LEFT => { editor.move_columns(1, extend); true }
            KEY_RIGHT => { editor.move_columns(-1, extend); true }
            KEY_BACKSPACE => { editor.backspace(); true }
            KEY_DELETE => { editor.delete(); true }
            KEY_RETURN | KEY_ENTER => { editor.insert("\n"); true }
            _ if modifiers & (CONTROL | ALT) == 0 && text.chars().all(|c| !c.is_control()) && !text.is_empty() => {
                editor.insert(text);
                true
            }
            _ => false,
        };
        if handled {
            self.refresh();
        }
        handled
    }

    unsafe fn mouse_press_event(mut self: Pin<&mut Self>, event: *mut qobject::QMouseEvent) {
        // SAFETY: Qtは配信中有効なイベントを渡す
        let event = unsafe { &*event };
        if !qobject::mouse_left_pressed(event) {
            return;
        }
        qobject::focus_item(self.as_mut());
        let (x, y) = (qobject::mouse_x(event), qobject::mouse_y(event));
        let rust = self.as_mut().rust_mut().get_mut();
        // ignore_drag は1か所にまとめられるが、各 return の意図が読めるよう分けて書く
        if let Some(ime) = &rust.ime {
            if let Some(candidate) = rust.editor.0.candidate_at(x, y) {
                if let Some(index) = candidate {
                    ime.select_candidate(index);
                }
                rust.ignore_drag = true;
                return;
            }
            if rust.editor.0.is_composing() {
                // FocusOutでfcitx5が確定する
                ime.focus_out();
                ime.focus_in();
                rust.ignore_drag = true;
                return;
            }
        }
        rust.ignore_drag = rust.editor.0.page_view().is_some();
        if rust.ignore_drag {
            rust.editor.0.jump_to_editing(x, y);
        } else {
            rust.editor.0.click(x, y, qobject::mouse_shift(event));
        }
        self.refresh();
    }

    unsafe fn mouse_move_event(mut self: Pin<&mut Self>, event: *mut qobject::QMouseEvent) {
        // SAFETY: Qtは配信中有効なイベントを渡す
        let event = unsafe { &*event };
        if !qobject::mouse_left_pressed(event) || self.rust().ignore_drag {
            return;
        }
        let (x, y) = (qobject::mouse_x(event), qobject::mouse_y(event));
        self.as_mut().editor_mut().click(x, y, true);
        self.refresh();
    }

    unsafe fn wheel_event(mut self: Pin<&mut Self>, event: *mut qobject::QWheelEvent) {
        // SAFETY: Qtは配信中有効なイベントを渡す
        let delta = qobject::wheel_delta(unsafe { &*event });
        let editor = self.as_mut().editor_mut();
        match editor.page_view_mut() {
            Some(view) if delta < 0.0 => view.next_spread(),
            Some(view) if delta > 0.0 => view.previous_spread(),
            Some(_) => {}
            None => editor.scroll_by(delta),
        }
        self.refresh();
    }

    unsafe fn input_method_event(mut self: Pin<&mut Self>, event: *mut qobject::QInputMethodEvent) {
        // SAFETY: Qtは配信中有効なイベントを渡す
        let event = unsafe { &*event };
        let preedit = String::from(&qobject::im_preedit(event));
        let commit = String::from(&qobject::im_commit(event));
        let start = qobject::im_replacement_start(event) as isize;
        let length = qobject::im_replacement_length(event) as usize;
        let cursor = usize::try_from(qobject::im_cursor(event))
            .ok()
            .map(|utf16| utf16_to_chars(&preedit, utf16));

        let editor = self.as_mut().editor_mut();
        // 確定と同時に次のpreeditが来ることがあるので、確定を先に処理する
        if !commit.is_empty() || length > 0 {
            editor.commit(start, length, &commit);
        }
        editor.set_preedit(&preedit, cursor);
        self.refresh();
    }

    fn input_method_query(&self, query: qobject::InputMethodQuery) -> QVariant {
        let editor = &self.rust().editor.0;
        match query {
            // ページモードは閲覧専用。fcitx5へ直接接続している間はQtの経路を止める
            qobject::InputMethodQuery::ImEnabled => QVariant::from(&(editor.page_view().is_none() && self.rust().ime.is_none())),
            qobject::InputMethodQuery::ImCursorRectangle => {
                let r = editor.cursor_rect();
                QVariant::from(&QRectF::new(r.x as f64, r.y as f64, r.width as f64, r.height as f64))
            }
            qobject::InputMethodQuery::ImCursorPosition => QVariant::from(&(editor.caret_index() as i32)),
            qobject::InputMethodQuery::ImAnchorPosition => QVariant::from(&(editor.anchor_position() as i32)),
            qobject::InputMethodQuery::ImHints => QVariant::from(&0i32),
            _ => QVariant::default(),
        }
    }
}

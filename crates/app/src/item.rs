//! 縦書き編集領域のQtアイテム。Qtのイベントを `Editor` の操作へ変換し、描画する。

use std::cell::RefCell;
use std::pin::Pin;

use cxx::UniquePtr;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QColor, QRectF, QString, QVariant};
use kakikotoba_layout::Rect;

use crate::editor::Editor;

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
        fn new_raw_font(path: &QString, pixel_size: f64) -> UniquePtr<QRawFont>;
        fn draw_glyphs(
            painter: Pin<&mut QPainter>,
            font: &QRawFont,
            color: &QColor,
            ids: &[u32],
            positions: &[f32],
        );
        fn fill_rect(painter: Pin<&mut QPainter>, x: f32, y: f32, width: f32, height: f32, color: &QColor);
        fn key_code(event: &QKeyEvent) -> i32;
        fn key_modifiers(event: &QKeyEvent) -> i32;
        fn key_text(event: &QKeyEvent) -> QString;
        fn ignore_key(event: Pin<&mut QKeyEvent>);
        fn mouse_x(event: &QMouseEvent) -> f32;
        fn mouse_y(event: &QMouseEvent) -> f32;
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
        type VerticalEditor = super::VerticalEditorRust;

        #[inherit]
        fn size(self: &VerticalEditor) -> QSizeF;
        #[inherit]
        fn update(self: Pin<&mut VerticalEditor>);

        #[cxx_override]
        unsafe fn paint(self: Pin<&mut VerticalEditor>, painter: *mut QPainter);
        #[cxx_override]
        #[cxx_name = "keyPressEvent"]
        unsafe fn key_press_event(self: Pin<&mut VerticalEditor>, event: *mut QKeyEvent);
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
}

const KEY_BACKSPACE: i32 = 0x0100_0003;
const KEY_RETURN: i32 = 0x0100_0004;
const KEY_ENTER: i32 = 0x0100_0005;
const KEY_DELETE: i32 = 0x0100_0007;
const KEY_LEFT: i32 = 0x0100_0012;
const KEY_UP: i32 = 0x0100_0013;
const KEY_RIGHT: i32 = 0x0100_0014;
const KEY_DOWN: i32 = 0x0100_0015;
const KEY_S: i32 = 0x53;
const KEY_Y: i32 = 0x59;
const KEY_Z: i32 = 0x5A;
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

pub struct VerticalEditorRust {
    text_color: QColor,
    selection_color: QColor,
    editor: Editor,
    raw_font: UniquePtr<qobject::QRawFont>,
    raw_font_size: f32,
}

impl Default for VerticalEditorRust {
    fn default() -> Self {
        Self {
            text_color: QColor::default(),
            selection_color: QColor::default(),
            // ponytail: 1ウィンドウ1文書。複数文書を開くならアイテムのプロパティで渡す
            editor: STARTUP_EDITOR.take().expect("VerticalEditorは1つだけ生成する"),
            raw_font: UniquePtr::null(),
            raw_font_size: 0.0,
        }
    }
}

impl cxx_qt::Initialize for qobject::VerticalEditor {
    fn initialize(self: Pin<&mut Self>) {
        qobject::init_item(self);
    }
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

impl qobject::VerticalEditor {
    fn editor_mut(self: Pin<&mut Self>) -> &mut Editor {
        &mut self.rust_mut().get_mut().editor
    }

    /// 操作の結果を画面とIMEへ反映する。
    fn refresh(self: Pin<&mut Self>) {
        self.update();
        qobject::update_input_method();
    }

    unsafe fn paint(mut self: Pin<&mut Self>, painter: *mut qobject::QPainter) {
        // SAFETY: Qtは描画中有効なQPainterを渡す
        let mut painter = unsafe { Pin::new_unchecked(&mut *painter) };
        let size = self.size();
        let rust = self.as_mut().rust_mut().get_mut();
        rust.editor.resize(size.width() as f32, size.height() as f32);

        let font_size = rust.editor.font_size();
        if rust.raw_font.is_null() || rust.raw_font_size != font_size {
            let path = QString::from(rust.editor.font_path.as_str());
            rust.raw_font = qobject::new_raw_font(&path, font_size as f64);
            rust.raw_font_size = font_size;
        }

        let mut fill = |rect: Rect, color: &QColor| {
            qobject::fill_rect(painter.as_mut(), rect.x, rect.y, rect.width, rect.height, color);
        };
        for rect in rust.editor.selection_rects() {
            fill(rect, &rust.selection_color);
        }
        for rect in rust.editor.preedit_lines() {
            fill(rect, &rust.text_color);
        }
        fill(rust.editor.caret_rect(), &rust.text_color);

        let mut ids = Vec::new();
        let mut positions = Vec::new();
        for (id, x, y) in rust.editor.visible_glyphs() {
            ids.push(id);
            positions.extend([x, y]);
        }
        let font = rust.raw_font.as_ref().expect("直前で生成済み");
        qobject::draw_glyphs(painter.as_mut(), font, &rust.text_color, &ids, &positions);
    }

    unsafe fn key_press_event(mut self: Pin<&mut Self>, event: *mut qobject::QKeyEvent) {
        // SAFETY: Qtは配信中有効なイベントを渡す
        let mut event = unsafe { Pin::new_unchecked(&mut *event) };
        let key = qobject::key_code(&event);
        let modifiers = qobject::key_modifiers(&event);
        let text = String::from(&qobject::key_text(&event));
        let extend = modifiers & SHIFT != 0;
        let control = modifiers & CONTROL != 0;
        let editor = self.as_mut().editor_mut();

        let handled = match key {
            _ if editor.is_composing() => false,
            KEY_UP => { editor.move_up(extend); true }
            KEY_DOWN => { editor.move_down(extend); true }
            KEY_LEFT => { editor.move_columns(1, extend); true }
            KEY_RIGHT => { editor.move_columns(-1, extend); true }
            KEY_BACKSPACE => { editor.backspace(); true }
            KEY_DELETE => { editor.delete(); true }
            KEY_RETURN | KEY_ENTER => { editor.insert("\n"); true }
            KEY_Z if control && extend => { editor.redo(); true }
            KEY_Z if control => { editor.undo(); true }
            KEY_Y if control => { editor.redo(); true }
            KEY_S if control => {
                if let Err(e) = editor.save() {
                    eprintln!("kakikotoba: 保存できません: {e}");
                }
                true
            }
            _ if modifiers & (CONTROL | ALT) == 0 && text.chars().all(|c| !c.is_control()) && !text.is_empty() => {
                editor.insert(&text);
                true
            }
            _ => false,
        };
        if handled {
            self.refresh();
        } else {
            qobject::ignore_key(event.as_mut());
        }
    }

    unsafe fn mouse_press_event(mut self: Pin<&mut Self>, event: *mut qobject::QMouseEvent) {
        // SAFETY: Qtは配信中有効なイベントを渡す
        let event = unsafe { &*event };
        qobject::focus_item(self.as_mut());
        let (x, y) = (qobject::mouse_x(event), qobject::mouse_y(event));
        self.as_mut().editor_mut().click(x, y, qobject::mouse_shift(event));
        self.refresh();
    }

    unsafe fn mouse_move_event(mut self: Pin<&mut Self>, event: *mut qobject::QMouseEvent) {
        // SAFETY: Qtは配信中有効なイベントを渡す
        let event = unsafe { &*event };
        let (x, y) = (qobject::mouse_x(event), qobject::mouse_y(event));
        self.as_mut().editor_mut().click(x, y, true);
        self.refresh();
    }

    unsafe fn wheel_event(mut self: Pin<&mut Self>, event: *mut qobject::QWheelEvent) {
        // SAFETY: Qtは配信中有効なイベントを渡す
        let delta = qobject::wheel_delta(unsafe { &*event });
        self.as_mut().editor_mut().scroll_by(delta);
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
        let editor = &self.rust().editor;
        match query {
            qobject::InputMethodQuery::ImEnabled => QVariant::from(&true),
            qobject::InputMethodQuery::ImCursorRectangle => {
                let r = editor.cursor_rect();
                QVariant::from(&QRectF::new(r.x as f64, r.y as f64, r.width as f64, r.height as f64))
            }
            qobject::InputMethodQuery::ImCursorPosition => QVariant::from(&(editor.cursor_position() as i32)),
            qobject::InputMethodQuery::ImAnchorPosition => QVariant::from(&(editor.anchor_position() as i32)),
            qobject::InputMethodQuery::ImHints => QVariant::from(&0i32),
            _ => QVariant::default(),
        }
    }
}

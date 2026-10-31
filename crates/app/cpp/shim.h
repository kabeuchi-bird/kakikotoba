// cxx-qtが直接扱えないQt APIの薄いラッパー
#pragma once

#include <QClipboard>
#include <QGlyphRun>
#include <QGuiApplication>
#include <QInputMethod>
#include <QInputMethodEvent>
#include <QKeyEvent>
#include <QList>
#include <QMouseEvent>
#include <QPainter>
#include <QPointF>
#include <QRawFont>
#include <QWheelEvent>
#include <QtQuick/QQuickPaintedItem>
#include <memory>

#include "rust/cxx.h"

namespace shim {

template <typename Item>
void init_item(Item& item) {
    item.setFlag(QQuickItem::ItemAcceptsInputMethod);
    item.setAcceptedMouseButtons(Qt::LeftButton);
}

template <typename Item>
void focus_item(Item& item) {
    item.forceActiveFocus(Qt::MouseFocusReason);
}

inline void update_input_method() {
    QGuiApplication::inputMethod()->update(Qt::ImQueryInput);
}

inline QString clipboard_text() { return QGuiApplication::clipboard()->text(); }
inline void set_clipboard_text(const QString& text) { QGuiApplication::clipboard()->setText(text); }

inline std::unique_ptr<QRawFont> new_raw_font(const QString& path, double pixel_size) {
    return std::make_unique<QRawFont>(path, pixel_size, QFont::PreferNoHinting);
}

// positions は x, y の交互の並び
inline void draw_glyphs(QPainter& painter, const QRawFont& font, const QColor& color,
                        rust::Slice<const quint32> ids, rust::Slice<const float> positions) {
    QList<quint32> indexes(ids.begin(), ids.end());
    QList<QPointF> points;
    points.reserve(indexes.size());
    for (size_t i = 0; i + 1 < positions.size(); i += 2) {
        points.append(QPointF(positions[i], positions[i + 1]));
    }
    QGlyphRun run;
    run.setRawFont(font);
    run.setGlyphIndexes(indexes);
    run.setPositions(points);
    painter.setPen(color);
    painter.drawGlyphRun(QPointF(0, 0), run);
}

inline void fill_rect(QPainter& painter, float x, float y, float width, float height,
                      const QColor& color) {
    painter.fillRect(QRectF(x, y, width, height), color);
}

inline qint32 key_code(const QKeyEvent& event) { return event.key(); }
inline qint32 key_modifiers(const QKeyEvent& event) { return event.modifiers().toInt(); }
inline QString key_text(const QKeyEvent& event) { return event.text(); }
inline void ignore_key(QKeyEvent& event) { event.ignore(); }

inline float mouse_x(const QMouseEvent& event) { return event.position().x(); }
inline float mouse_y(const QMouseEvent& event) { return event.position().y(); }
inline bool mouse_shift(const QMouseEvent& event) {
    return event.modifiers().testFlag(Qt::ShiftModifier);
}

inline float wheel_delta(const QWheelEvent& event) { return event.angleDelta().y(); }

inline QString im_preedit(const QInputMethodEvent& event) { return event.preeditString(); }
inline QString im_commit(const QInputMethodEvent& event) { return event.commitString(); }
inline qint32 im_replacement_start(const QInputMethodEvent& event) {
    return event.replacementStart();
}
inline qint32 im_replacement_length(const QInputMethodEvent& event) {
    return event.replacementLength();
}
// preedit内のカーソル位置（UTF-16単位）。指定がなければ-1
inline qint32 im_cursor(const QInputMethodEvent& event) {
    for (const auto& attribute : event.attributes()) {
        if (attribute.type == QInputMethodEvent::Cursor) {
            return attribute.start;
        }
    }
    return -1;
}

} // namespace shim

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
#include <QPainterPath>
#include <QPixmap>
#include <QPixmapCache>
#include <QPointF>
#include <QRawFont>
#include <QWheelEvent>
#include <QtQuick/QQuickPaintedItem>
#include <cmath>
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
    QGuiApplication::inputMethod()->update(Qt::ImQueryAll);
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

// 波線。rectの天地方向に波打つ
inline void draw_wave(QPainter& painter, float x, float y, float width, float height,
                      const QColor& color) {
    const float amplitude = width / 2;
    const float center = x + amplitude;
    const float period = width * 2;
    QPainterPath path(QPointF(center, y));
    for (float offset = 0; offset <= height; offset += 1) {
        path.lineTo(center + amplitude * std::sin(offset / period * 2 * M_PI), y + offset);
    }
    painter.setPen(color);
    painter.setBrush(Qt::NoBrush);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.drawPath(path);
    painter.setRenderHint(QPainter::Antialiasing, false);
}

// 読めない画像は枠だけ描く
inline void draw_image(QPainter& painter, const QString& path, float x, float y, float width,
                       float height, const QColor& frame) {
    const QRectF page(x, y, width, height);
    QPixmap pixmap;
    if (!QPixmapCache::find(path, &pixmap)) {
        if (pixmap.load(path)) {
            QPixmapCache::insert(path, pixmap);
        }
    }
    if (pixmap.isNull()) {
        painter.setPen(frame);
        painter.setBrush(Qt::NoBrush);
        painter.drawRect(page);
        return;
    }
    const QSizeF size = pixmap.size().toSizeF().scaled(page.size(), Qt::KeepAspectRatio);
    const QRectF target(x + (width - size.width()) / 2, y + (height - size.height()) / 2,
                        size.width(), size.height());
    painter.drawPixmap(target, pixmap, QRectF(pixmap.rect()));
}

inline qint32 key_code(const QKeyEvent& event) { return event.key(); }
inline qint32 key_modifiers(const QKeyEvent& event) { return event.modifiers().toInt(); }
inline QString key_text(const QKeyEvent& event) { return event.text(); }
inline void ignore_key(QKeyEvent& event) { event.ignore(); }

inline float mouse_x(const QMouseEvent& event) { return event.position().x(); }
inline float mouse_y(const QMouseEvent& event) { return event.position().y(); }
inline bool mouse_left_pressed(const QMouseEvent& event) {
    return event.buttons().testFlag(Qt::LeftButton);
}
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

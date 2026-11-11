#include "terminalview.h"

#include <QClipboard>
#include <QFontDatabase>
#include <QFontMetricsF>
#include <QGuiApplication>
#include <QInputMethodEvent>
#include <QPainter>
#include <QQmlEngine>
#include <QQuickWindow>
#include <QtEndian>
#include <QtMath>
#include <cstdio>
#include <unistd.h>

namespace {
// Message kinds, see src/proto.rs
enum : quint8 {
    NEW = 1, ATTACH = 2, INPUT = 3, RESIZE = 4, SCROLL = 5, CLOSE = 6, ACK = 8,
    SESSION = 10, FRAME = 11, TITLE = 12, EXIT = 13, BELL = 14, CLIPBOARD = 15,
};
// TermMode bits (alacritty_terminal)
enum : quint32 {
    SHOW_CURSOR = 1, APP_CURSOR = 1 << 1, APP_KEYPAD = 1 << 2, MOUSE_REPORT_CLICK = 1 << 3, BRACKETED_PASTE = 1 << 4,
    SGR_MOUSE = 1 << 5, MOUSE_MOTION = 1 << 6, FOCUS_IN_OUT = 1 << 11, ALT_SCREEN = 1 << 12, MOUSE_DRAG = 1 << 13,
    ALTERNATE_SCROLL = 1 << 15, MOUSE_MODE = MOUSE_REPORT_CLICK | MOUSE_MOTION | MOUSE_DRAG,
};
// Cell flags (alacritty_terminal)
enum : quint16 {
    INVERSE = 1, BOLD = 2, ITALIC = 4, UNDERLINE = 8, WIDE_CHAR = 0x20, WIDE_CHAR_SPACER = 0x40, DIM = 0x80,
    HIDDEN = 0x100, STRIKEOUT = 0x200, LEADING_WIDE_CHAR_SPACER = 0x400, DOUBLE_UNDERLINE = 0x800, UNDERCURL = 0x1000,
    DOTTED_UNDERLINE = 0x2000, DASHED_UNDERLINE = 0x4000,
    ALL_UNDERLINES = UNDERLINE | DOUBLE_UNDERLINE | UNDERCURL | DOTTED_UNDERLINE | DASHED_UNDERLINE,
};
// Named colours past the 16 ANSI ones (alacritty's NamedColor)
enum { FOREGROUND = 256, BACKGROUND = 257, CURSOR = 258, DIM_BLACK = 259, BRIGHT_FOREGROUND = 267, DIM_FOREGROUND = 268 };

// Palette of the foot theme shipped with Filigrane (dark)
const QRgb kAnsi[16] = {0x2d3744, 0xc98f93, 0xa3bfa8, 0xd2bf92, 0x9dbbd6, 0xb3a8c9, 0x95c0c6, 0xa9b8c8,
                        0x5f7894, 0xd9a5a9, 0xb8d0bc, 0xe0d0a8, 0xb9d0e4, 0xc6bdd8, 0xadd2d7, 0xdbe7f4};
const QRgb kForeground = 0xdbe7f4, kBackground = 0x1c222b, kSelection = 0x3c4858;

QRgb dim(QRgb c) { return qRgb(qRed(c) * 2 / 3, qGreen(c) * 2 / 3, qBlue(c) * 2 / 3); }

struct In {
    const char* p;
    const char* end;
    bool ok = true;
    template <typename T> T get() {
        if (end - p < qsizetype(sizeof(T))) { ok = false; return T(); }
        T v = qFromLittleEndian<T>(p);
        p += sizeof(T);
        return v;
    }
};

void put16(QByteArray& b, quint16 v) { char c[2]; qToLittleEndian(v, c); b.append(c, 2); }
void put32(QByteArray& b, quint32 v) { char c[4]; qToLittleEndian(v, c); b.append(c, 4); }
void putString(QByteArray& b, const QString& s) {
    QByteArray u = s.toUtf8().left(65535);
    put16(b, quint16(u.size()));
    b.append(u);
}

QString daemonSocket() {
    QByteArray dir = qgetenv("XDG_RUNTIME_DIR");
    if (dir.isEmpty()) dir = QByteArray("/tmp/fili-term-") + QByteArray::number(getuid());
    return QString::fromLocal8Bit(dir) + QStringLiteral("/fili-term/daemon.sock");
}
}  // namespace

TerminalView::TerminalView(QQuickItem* parent) : QQuickPaintedItem(parent) {
    setFlag(ItemAcceptsInputMethod, true);
    setFlag(ItemIsFocusScope, false);
    setAcceptedMouseButtons(Qt::AllButtons);
    setAcceptHoverEvents(false);
    setOpaquePainting(true);
    setFillColor(QColor::fromRgb(kBackground));
    setAntialiasing(false);
    setActiveFocusOnTab(true);
    m_palette.resize(256);
    for (int i = 0; i < 16; ++i) m_palette[i] = kAnsi[i];
    static const int steps[6] = {0, 95, 135, 175, 215, 255};
    for (int i = 0; i < 216; ++i) m_palette[16 + i] = qRgb(steps[i / 36], steps[(i / 6) % 6], steps[i % 6]);
    for (int i = 0; i < 24; ++i) m_palette[232 + i] = qRgb(8 + 10 * i, 8 + 10 * i, 8 + 10 * i);
    m_font = QFont(QStringLiteral("JetBrains Mono"));
    m_font.setPixelSize(12);
    m_trace = qEnvironmentVariableIsSet("FILI_TERM_TRACE");
    m_clock.start();
    measure();
    connect(&m_sock, &QLocalSocket::readyRead, this, &TerminalView::onReadyRead);
    connect(&m_sock, &QLocalSocket::connected, this, [this] {
        QByteArray b;
        if (m_session > 0) {
            put32(b, quint32(m_session));
            put16(b, quint16(m_wantCols)); put16(b, quint16(m_wantRows));
            put16(b, quint16(m_cw)); put16(b, quint16(m_ch));
            send(ATTACH, b);
        } else {
            put16(b, quint16(m_wantCols)); put16(b, quint16(m_wantRows));
            put16(b, quint16(m_cw)); put16(b, quint16(m_ch));
            putString(b, m_cwd);
            b.append(char(qMin(qsizetype(255), m_command.size())));
            for (int i = 0; i < m_command.size() && i < 255; ++i) putString(b, m_command[i]);
            send(NEW, b);
        }
    });
    connect(&m_sock, &QLocalSocket::disconnected, this, [this] { emit exited(-1); });
    connect(&m_sock, &QLocalSocket::errorOccurred, this, [this](QLocalSocket::LocalSocketError) {
        if (m_sock.state() != QLocalSocket::ConnectedState) emit exited(-1);
    });
}

TerminalView::~TerminalView() = default;

void TerminalView::setSessionId(int id) {
    if (m_session == id) return;
    m_session = id;
    emit sessionIdChanged();
}

void TerminalView::setFontFamily(const QString& f) {
    if (f == m_font.family()) return;
    m_font.setFamily(f);
    measure();
    emit fontChanged();
}

void TerminalView::setFontPixelSize(int px) {
    if (px == m_font.pixelSize() || px < 4) return;
    m_font.setPixelSize(px);
    measure();
    emit fontChanged();
}

void TerminalView::setPadding(int p) {
    if (p == m_pad) return;
    m_pad = p;
    if (computeGrid()) sendSize();
    markAll();
    emit fontChanged();
}

// Cell size from the font: whole pixels (crisp backgrounds), the glyph advance forced to the cell width so a run
// of text drawn at once lines up with the grid
void TerminalView::measure() {
    m_font.setStyleHint(QFont::Monospace);
    m_font.setHintingPreference(QFont::PreferFullHinting);
    m_font.setKerning(false);
    m_font.setLetterSpacing(QFont::AbsoluteSpacing, 0);
    QFontMetricsF fm(m_font);
    qreal advance = fm.horizontalAdvance(QLatin1Char('M'));
    m_cw = qMax<qreal>(1, qRound(advance));
    m_ch = qMax<qreal>(1, qCeil(fm.height()));
    m_ascent = qRound(fm.ascent() + (m_ch - fm.height()) / 2);
    m_font.setLetterSpacing(QFont::AbsoluteSpacing, m_cw - advance);
    m_bold = m_font; m_bold.setWeight(QFont::Bold);
    m_italic = m_font; m_italic.setItalic(true);
    m_boldItalic = m_bold; m_boldItalic.setItalic(true);
    m_inFont.clear();
    if (computeGrid()) sendSize();
    markAll();
}

bool TerminalView::computeGrid() {
    int cols = qMax(2, int((width() - 2 * m_pad) / m_cw));
    int rows = qMax(1, int((height() - 2 * m_pad) / m_ch));
    if (cols == m_wantCols && rows == m_wantRows) return false;
    m_wantCols = cols;
    m_wantRows = rows;
    return true;
}

void TerminalView::componentComplete() {
    QQuickPaintedItem::componentComplete();
    m_complete = true;
    if (width() > 0 && height() > 0) connectDaemon();
}

void TerminalView::geometryChange(const QRectF& g, const QRectF& old) {
    QQuickPaintedItem::geometryChange(g, old);
    if (g.size() == old.size() || g.width() <= 0 || g.height() <= 0) return;
    markAll();
    if (computeGrid()) sendSize();
    if (m_complete) connectDaemon();
}

void TerminalView::itemChange(ItemChange change, const ItemChangeData& value) {
    QQuickPaintedItem::itemChange(change, value);
    if (change != ItemVisibleHasChanged) return;
    if (value.boolValue) {
        // A tab shown again: full-size image back, painted whole, which also acknowledges the frame the daemon waits on
        setTextureSize(QSize());
        markAll();
    } else {
        // A hidden tab keeps no screen-sized image (≈ 7 MB at 1.75 scale)
        setTextureSize(QSize(1, 1));
    }
}

void TerminalView::connectDaemon() {
    if (m_connecting) return;
    m_connecting = true;
    computeGrid();
    m_sock.connectToServer(daemonSocket());
}

void TerminalView::send(quint8 kind, const QByteArray& payload) {
    if (m_sock.state() != QLocalSocket::ConnectedState) return;
    QByteArray b;
    put32(b, quint32(payload.size() + 1));
    b.append(char(kind));
    b.append(payload);
    m_sock.write(b);
}

void TerminalView::sendInput(const QByteArray& bytes) {
    if (bytes.isEmpty()) return;
    if (m_hasSelection) {
        m_hasSelection = false;
        markAll();
    }
    send(INPUT, bytes);
}

void TerminalView::sendSize() {
    QByteArray b;
    put16(b, quint16(m_wantCols)); put16(b, quint16(m_wantRows));
    put16(b, quint16(m_cw)); put16(b, quint16(m_ch));
    send(RESIZE, b);
}

void TerminalView::closeSession() {
    send(CLOSE, {});
    m_sock.flush();
}

void TerminalView::onReadyRead() {
    m_in.append(m_sock.readAll());
    qsizetype at = 0;
    while (m_in.size() - at >= 4) {
        quint32 len = qFromLittleEndian<quint32>(m_in.constData() + at);
        if (m_in.size() - at - 4 < qsizetype(len)) break;
        quint8 kind = quint8(m_in[at + 4]);
        handle(kind, QByteArray::fromRawData(m_in.constData() + at + 5, len - 1));
        at += 4 + len;
    }
    m_in.remove(0, at);
}

void TerminalView::handle(quint8 kind, const QByteArray& payload) {
    In in{payload.constData(), payload.constData() + payload.size()};
    switch (kind) {
    case SESSION:
        setSessionId(int(in.get<quint32>()));
        break;
    case FRAME:
        applyFrame(payload);
        break;
    case TITLE: {
        quint16 n = in.get<quint16>();
        m_title = QString::fromUtf8(in.p, qMin<qsizetype>(n, in.end - in.p));
        emit titleChanged();
        break;
    }
    case CLIPBOARD: {
        quint16 n = in.get<quint16>();
        QGuiApplication::clipboard()->setText(QString::fromUtf8(in.p, qMin<qsizetype>(n, in.end - in.p)));
        break;
    }
    case BELL:
        emit bell();
        break;
    case EXIT: {
        int status = in.get<qint32>();
        m_sock.disconnect(this);
        emit exited(status);
        break;
    }
    default:
        break;
    }
}

void TerminalView::applyFrame(const QByteArray& payload) {
    qint64 t0 = m_trace ? m_clock.nsecsElapsed() : 0;
    In in{payload.constData(), payload.constData() + payload.size()};
    int cols = in.get<quint16>(), rows = in.get<quint16>();
    m_mode = in.get<quint32>();
    int curCol = in.get<quint16>();
    int curLine = in.get<qint16>();
    int shape = in.get<quint8>();
    int offset = int(in.get<quint32>()), history = int(in.get<quint32>());
    in.get<quint8>();  // full
    int count = in.get<quint16>();
    if (!in.ok || cols <= 0 || rows <= 0) return;
    if (cols != m_cols || rows != m_rows) {
        m_cols = cols;
        m_rows = rows;
        m_cells.assign(size_t(cols) * rows, Cell());
        m_extras.clear();
        m_hasSelection = false;
        markAll();
        emit gridChanged();
    }
    if (m_curLine != curLine || m_curCol != curCol || m_curShape != shape) {
        markLine(m_curLine);
        markLine(curLine);
    }
    m_curCol = curCol;
    m_curLine = curLine;
    m_curShape = shape;
    if (offset != m_offset || history != m_history) {
        m_offset = offset;
        m_history = history;
        emit scrollChanged();
    }
    for (int i = 0; i < count && in.ok; ++i) {
        int line = in.get<quint16>();
        bool valid = line < rows;
        for (int col = 0; col < cols && in.ok; ++col) {
            quint32 c = in.get<quint32>();
            Cell cell;
            cell.c = char32_t(c & 0x1fffff);
            cell.fg = in.get<quint32>();
            cell.bg = in.get<quint32>();
            cell.flags = in.get<quint16>();
            int idx = line * cols + col;
            if (c & 0x80000000u) {
                int n = in.get<quint8>();
                QString z;
                for (int k = 0; k < n; ++k) {
                    char32_t ch = in.get<quint32>();
                    z += QString::fromUcs4(&ch, 1);
                }
                cell.extra = true;
                if (valid) m_extras[idx] = z;
            } else if (valid && m_cells[idx].extra) {
                m_extras.erase(idx);
            }
            if (valid) m_cells[idx] = cell;
        }
        if (valid) markLine(line);
    }
    m_needAck = true;
    // Nothing visible changed (title only…): still paint once so the daemon gets its acknowledgement
    markLine(qMax(0, m_curLine));
    if (m_trace)
        fprintf(stderr, "frame %d lines, %lld bytes, parsed in %.2f ms\n", count, (long long)payload.size(),
                (m_clock.nsecsElapsed() - t0) / 1e6);
}

void TerminalView::markLine(int line) {
    if (line < 0 || line >= m_rows) return;
    update(QRect(0, int(m_pad + line * m_ch), int(width()), int(m_ch) + 1));
}

// Explicit rectangle: a bare update() followed by update(line) leaves Qt with the line alone to repaint
void TerminalView::markAll() {
    m_full = true;
    update(QRect(0, 0, qCeil(width()), qCeil(height())));
}

QColor TerminalView::resolve(quint32 c) const {
    quint32 tag = c >> 24, v = c & 0xffffff;
    if (tag == 0) return QColor::fromRgb(QRgb(v));
    if (tag == 2) return QColor::fromRgb(m_palette[v & 0xff]);
    if (v < 16) return QColor::fromRgb(kAnsi[v]);
    switch (v) {
    case FOREGROUND: case CURSOR: case BRIGHT_FOREGROUND: return QColor::fromRgb(kForeground);
    case BACKGROUND: return QColor::fromRgb(kBackground);
    case DIM_FOREGROUND: return QColor::fromRgb(dim(kForeground));
    default:
        if (v >= DIM_BLACK && v < DIM_BLACK + 8) return QColor::fromRgb(dim(kAnsi[v - DIM_BLACK]));
        return QColor::fromRgb(kForeground);
    }
}

// Characters the main font has and draws inside one cell: drawn in runs; others (emoji, CJK, fallback fonts)
// one by one at their cell
bool TerminalView::simpleGlyph(char32_t c) const {
    if (c < 0x7f) return true;
    auto it = m_inFont.find(c);
    if (it != m_inFont.end()) return it->second;
    bool ok = QFontMetricsF(m_font).inFontUcs4(uint(c));
    m_inFont.emplace(c, ok);
    return ok;
}

bool TerminalView::inSelection(int col, int line) const {
    if (!m_hasSelection) return false;
    Pos a = m_selA, b = m_selB;
    if (b.line < a.line || (b.line == a.line && b.col < a.col)) std::swap(a, b);
    if (line < a.line || line > b.line) return false;
    if (line == a.line && col < a.col) return false;
    if (line == b.line && col > b.col) return false;
    return true;
}

void TerminalView::paint(QPainter* p) {
    qint64 t0 = m_trace ? m_clock.nsecsElapsed() : 0;
    QRect clip = p->clipBoundingRect().toAlignedRect();
    // A new image (tab shown again, resize) starts uninitialised: all of it is painted
    if (clip.isEmpty() || m_full) {
        p->setClipping(false);
        clip = QRect(0, 0, qCeil(width()), qCeil(height()));
    }
    m_full = false;
    p->fillRect(clip, QColor::fromRgb(kBackground));
    int first = qMax(0, int((clip.top() - m_pad) / m_ch));
    int last = qMin(m_rows - 1, int((clip.bottom() - m_pad) / m_ch));
    for (int line = first; line <= last; ++line) paintLine(p, line);
    if (m_needAck) {
        m_needAck = false;
        send(ACK, {});
    }
    if (m_trace) {
        qint64 t1 = m_clock.nsecsElapsed();
        fprintf(stderr, "paint lines %d-%d in %.2f ms", first, last, (t1 - t0) / 1e6);
        if (m_keyAt >= 0) {
            fprintf(stderr, ", key→paint %.2f ms", (t1 - m_keyAt) / 1e6);
            m_keyAt = -1;
        }
        fputc('\n', stderr);
    }
}

void TerminalView::paintLine(QPainter* p, int line) {
    if (m_cells.empty()) return;
    const qreal y = m_pad + line * m_ch;
    const Cell* row = &m_cells[size_t(line) * m_cols];
    const QColor defaultBg = QColor::fromRgb(kBackground);

    auto colors = [&](int col, QColor& fg, QColor& bg) {
        const Cell& c = row[col];
        fg = resolve(c.fg);
        bg = resolve(c.bg);
        if (c.flags & DIM) fg = QColor::fromRgb(dim(fg.rgb()));
        if (c.flags & INVERSE) std::swap(fg, bg);
        if (inSelection(col, line)) bg = QColor::fromRgb(kSelection);
        if (c.flags & HIDDEN) fg = bg;
    };

    // Backgrounds, in spans of one colour
    for (int col = 0; col < m_cols;) {
        QColor fg, bg;
        colors(col, fg, bg);
        int end = col + 1;
        for (; end < m_cols; ++end) {
            QColor f2, b2;
            colors(end, f2, b2);
            if (b2 != bg) break;
        }
        if (bg != defaultBg) {
            // The first and last columns' backgrounds reach into the padding, like foot
            qreal x0 = col == 0 ? 0 : m_pad + col * m_cw;
            qreal x1 = end == m_cols ? width() : m_pad + end * m_cw;
            p->fillRect(QRectF(x0, y, x1 - x0, m_ch), bg);
        }
        col = end;
    }

    // Text: runs of cells with the same colour and style, made only of glyphs of the main font
    QString run;
    int runStart = 0;
    QColor runFg;
    quint16 runStyle = 0;
    auto fontFor = [&](quint16 style) -> const QFont& {
        if ((style & (BOLD | ITALIC)) == (BOLD | ITALIC)) return m_boldItalic;
        if (style & BOLD) return m_bold;
        if (style & ITALIC) return m_italic;
        return m_font;
    };
    auto flush = [&] {
        if (run.isEmpty()) return;
        p->setFont(fontFor(runStyle));
        p->setPen(runFg);
        p->drawText(QPointF(m_pad + runStart * m_cw, y + m_ascent), run);
        run.clear();
    };
    for (int col = 0; col < m_cols; ++col) {
        const Cell& c = row[col];
        if (c.flags & (WIDE_CHAR_SPACER | LEADING_WIDE_CHAR_SPACER)) {
            flush();
            continue;
        }
        QColor fg, bg;
        colors(col, fg, bg);
        quint16 style = c.flags & (BOLD | ITALIC);
        bool wide = c.flags & WIDE_CHAR;
        bool simple = !wide && !c.extra && simpleGlyph(c.c);
        if (!simple) {
            flush();
            if (c.c == ' ' && !c.extra) continue;
            QString text = QString::fromUcs4(&c.c, 1);
            if (c.extra) {
                auto it = m_extras.find(line * m_cols + col);
                if (it != m_extras.end()) text += it->second;
            }
            p->setFont(fontFor(style));
            p->setPen(fg);
            qreal w = (wide ? 2 : 1) * m_cw;
            QRectF box(m_pad + col * m_cw, y, w, m_ch);
            p->drawText(box, Qt::AlignCenter | Qt::TextDontClip, text);
            continue;
        }
        if (!run.isEmpty() && (fg != runFg || style != runStyle)) flush();
        if (run.isEmpty()) {
            runStart = col;
            runFg = fg;
            runStyle = style;
        }
        if (c.c == 0) run += QLatin1Char(' ');
        else run += QString::fromUcs4(&c.c, 1);
    }
    flush();

    // Decorations
    for (int col = 0; col < m_cols; ++col) {
        const Cell& c = row[col];
        if (!(c.flags & (ALL_UNDERLINES | STRIKEOUT))) continue;
        QColor fg, bg;
        colors(col, fg, bg);
        qreal x = m_pad + col * m_cw;
        if (c.flags & ALL_UNDERLINES) {
            p->fillRect(QRectF(x, y + m_ascent + 2, m_cw, 1), fg);
            if (c.flags & DOUBLE_UNDERLINE) p->fillRect(QRectF(x, y + m_ascent + 4, m_cw, 1), fg);
        }
        if (c.flags & STRIKEOUT) p->fillRect(QRectF(x, y + m_ch / 2, m_cw, 1), fg);
    }

    // Cursor (hollow while the window is not focused)
    if (line == m_curLine && m_curShape != 0 && m_curCol < m_cols && m_offset == 0) {
        const Cell& c = row[m_curCol];
        qreal w = (c.flags & WIDE_CHAR ? 2 : 1) * m_cw;
        QRectF r(m_pad + m_curCol * m_cw, y, w, m_ch);
        QColor color = QColor::fromRgb(kForeground);
        if (!m_focused || m_curShape == 4) {
            p->setPen(color);
            p->setBrush(Qt::NoBrush);
            p->drawRect(r.adjusted(0.5, 0.5, -0.5, -0.5));
        } else if (m_curShape == 1) {
            p->fillRect(r, color);
            if (c.c != ' ' && c.c != 0) {
                QString text = QString::fromUcs4(&c.c, 1);
                p->setFont(fontFor(c.flags & (BOLD | ITALIC)));
                p->setPen(QColor::fromRgb(kBackground));
                p->drawText(r, Qt::AlignCenter | Qt::TextDontClip, text);
            }
        } else if (m_curShape == 2) {
            p->fillRect(QRectF(r.left(), r.bottom() - 2, w, 2), color);
        } else if (m_curShape == 3) {
            p->fillRect(QRectF(r.left(), r.top(), 2, m_ch), color);
        }
    }

    // Text being composed by the input method, over the cursor
    if (line == m_curLine && !m_preedit.isEmpty()) {
        QFontMetricsF fm(m_font);
        QRectF r(m_pad + m_curCol * m_cw, y, fm.horizontalAdvance(m_preedit), m_ch);
        p->fillRect(r, QColor::fromRgb(kBackground));
        p->setFont(m_font);
        p->setPen(QColor::fromRgb(kForeground));
        p->drawText(QPointF(r.left(), y + m_ascent), m_preedit);
        p->fillRect(QRectF(r.left(), y + m_ascent + 2, r.width(), 1), QColor::fromRgb(kForeground));
    }
}

// ---------------------------------------------------------------- keyboard

void TerminalView::keyPressEvent(QKeyEvent* e) {
    if (m_trace && m_keyAt < 0) m_keyAt = m_clock.nsecsElapsed();
    const auto mods = e->modifiers();
    const bool shift = mods & Qt::ShiftModifier, alt = mods & Qt::AltModifier, ctrl = mods & Qt::ControlModifier;
    const int m = 1 + (shift ? 1 : 0) + (alt ? 2 : 0) + (ctrl ? 4 : 0);
    const bool appCursor = m_mode & APP_CURSOR;

    // Scrolling the history stays in the window
    if (shift && !ctrl && !alt && !(m_mode & ALT_SCREEN)) {
        if (e->key() == Qt::Key_PageUp) { scrollPage(-1); return; }
        if (e->key() == Qt::Key_PageDown) { scrollPage(1); return; }
    }
    if (shift && e->key() == Qt::Key_Insert) { paste(); return; }

    auto csi = [&](const char* final, bool ss3able) -> QByteArray {
        if (m > 1) return QByteArray("\x1b[1;") + QByteArray::number(m) + final;
        return QByteArray(ss3able && appCursor ? "\x1bO" : "\x1b[") + final;
    };
    auto tilde = [&](int n) -> QByteArray {
        if (m > 1) return "\x1b[" + QByteArray::number(n) + ";" + QByteArray::number(m) + "~";
        return "\x1b[" + QByteArray::number(n) + "~";
    };
    QByteArray out;
    switch (e->key()) {
    case Qt::Key_Return: case Qt::Key_Enter: out = alt ? "\x1b\r" : "\r"; break;
    case Qt::Key_Backspace: out = ctrl ? "\x08" : (alt ? "\x1b\x7f" : "\x7f"); break;
    case Qt::Key_Tab: out = "\t"; break;
    case Qt::Key_Backtab: out = "\x1b[Z"; break;
    case Qt::Key_Escape: out = "\x1b"; break;
    case Qt::Key_Up: out = csi("A", true); break;
    case Qt::Key_Down: out = csi("B", true); break;
    case Qt::Key_Right: out = csi("C", true); break;
    case Qt::Key_Left: out = csi("D", true); break;
    case Qt::Key_Home: out = csi("H", true); break;
    case Qt::Key_End: out = csi("F", true); break;
    case Qt::Key_Insert: out = tilde(2); break;
    case Qt::Key_Delete: out = tilde(3); break;
    case Qt::Key_PageUp: out = tilde(5); break;
    case Qt::Key_PageDown: out = tilde(6); break;
    case Qt::Key_F1: out = m > 1 ? csi("P", false) : "\x1bOP"; break;
    case Qt::Key_F2: out = m > 1 ? csi("Q", false) : "\x1bOQ"; break;
    case Qt::Key_F3: out = m > 1 ? csi("R", false) : "\x1bOR"; break;
    case Qt::Key_F4: out = m > 1 ? csi("S", false) : "\x1bOS"; break;
    case Qt::Key_F5: out = tilde(15); break;
    case Qt::Key_F6: out = tilde(17); break;
    case Qt::Key_F7: out = tilde(18); break;
    case Qt::Key_F8: out = tilde(19); break;
    case Qt::Key_F9: out = tilde(20); break;
    case Qt::Key_F10: out = tilde(21); break;
    case Qt::Key_F11: out = tilde(23); break;
    case Qt::Key_F12: out = tilde(24); break;
    default: break;
    }
    if (out.isEmpty() && ctrl) {
        int k = e->key();
        if (k >= Qt::Key_A && k <= Qt::Key_Z) out = QByteArray(1, char(k - Qt::Key_A + 1));
        else if (k == Qt::Key_Space || k == Qt::Key_At || k == Qt::Key_2) out = QByteArray(1, '\0');
        else if (k == Qt::Key_BracketLeft || k == Qt::Key_3) out = "\x1b";
        else if (k == Qt::Key_Backslash || k == Qt::Key_4) out = "\x1c";
        else if (k == Qt::Key_BracketRight || k == Qt::Key_5) out = "\x1d";
        else if (k == Qt::Key_AsciiCircum || k == Qt::Key_6) out = "\x1e";
        else if (k == Qt::Key_Underscore || k == Qt::Key_Slash || k == Qt::Key_7) out = "\x1f";
        if (!out.isEmpty() && alt) out.prepend('\x1b');
    }
    if (out.isEmpty()) {
        QString text = e->text();
        if (text.isEmpty()) { e->ignore(); return; }
        out = text.toUtf8();
        if (alt) out.prepend('\x1b');
    }
    sendInput(out);
    e->accept();
}

void TerminalView::inputMethodEvent(QInputMethodEvent* e) {
    if (!e->commitString().isEmpty()) sendInput(e->commitString().toUtf8());
    if (m_preedit != e->preeditString()) {
        m_preedit = e->preeditString();
        markLine(m_curLine);
    }
    e->accept();
}

QVariant TerminalView::inputMethodQuery(Qt::InputMethodQuery q) const {
    switch (q) {
    case Qt::ImEnabled: return true;
    case Qt::ImCursorRectangle:
        return QRectF(m_pad + m_curCol * m_cw, m_pad + qMax(0, m_curLine) * m_ch, m_cw, m_ch);
    case Qt::ImFont: return m_font;
    case Qt::ImHints: return int(Qt::ImhNoPredictiveText | Qt::ImhNoAutoUppercase);
    default: return QQuickPaintedItem::inputMethodQuery(q);
    }
}

void TerminalView::focusInEvent(QFocusEvent* e) {
    QQuickPaintedItem::focusInEvent(e);
    m_focused = true;
    if (m_mode & FOCUS_IN_OUT) send(INPUT, "\x1b[I");
    markLine(m_curLine);
}

void TerminalView::focusOutEvent(QFocusEvent* e) {
    QQuickPaintedItem::focusOutEvent(e);
    m_focused = false;
    if (m_mode & FOCUS_IN_OUT) send(INPUT, "\x1b[O");
    markLine(m_curLine);
}

// ---------------------------------------------------------------- clipboard and scrolling

void TerminalView::copy() {
    if (m_hasSelection) QGuiApplication::clipboard()->setText(selectionText());
}

void TerminalView::paste() {
    QString text = QGuiApplication::clipboard()->text();
    if (text.isEmpty()) return;
    text.replace(QStringLiteral("\r\n"), QStringLiteral("\r")).replace(QLatin1Char('\n'), QLatin1Char('\r'));
    QByteArray b = text.toUtf8();
    if (m_mode & BRACKETED_PASTE) b = "\x1b[200~" + b.replace("\x1b[201~", "") + "\x1b[201~";
    sendInput(b);
}

void TerminalView::scrollPage(int direction) {
    QByteArray b;
    b.append(char(direction < 0 ? 1 : 2));
    put32(b, 0);
    send(SCROLL, b);
}

// ---------------------------------------------------------------- mouse

TerminalView::Pos TerminalView::cellAt(QPointF pos) const {
    Pos c;
    c.col = qBound(0, int((pos.x() - m_pad) / m_cw), qMax(0, m_cols - 1));
    c.line = qBound(0, int((pos.y() - m_pad) / m_ch), qMax(0, m_rows - 1));
    return c;
}

// Mouse reporting for programs that asked for it (vim, htop…); Shift keeps the mouse for selection
bool TerminalView::reportMouse(QMouseEvent* e, int button, bool press, bool motion) {
    if (!(m_mode & MOUSE_MODE) || (e->modifiers() & Qt::ShiftModifier)) return false;
    if (motion && !(m_mode & (MOUSE_DRAG | MOUSE_MOTION))) return true;
    if (motion && button == 3 && !(m_mode & MOUSE_MOTION)) return true;
    Pos c = cellAt(e->position());
    if (motion && c.col == m_lastMouseCell.col && c.line == m_lastMouseCell.line) return true;
    m_lastMouseCell = c;
    int b = button + (motion ? 32 : 0);
    if (e->modifiers() & Qt::AltModifier) b += 8;
    if (e->modifiers() & Qt::ControlModifier) b += 16;
    QByteArray out;
    if (m_mode & SGR_MOUSE) {
        out = "\x1b[<" + QByteArray::number(b) + ";" + QByteArray::number(c.col + 1) + ";" +
              QByteArray::number(c.line + 1) + (press ? "M" : "m");
    } else {
        if (!press) b = 3 + (b & ~3);
        out = "\x1b[M";
        out.append(char(32 + b)).append(char(qMin(255, 33 + c.col))).append(char(qMin(255, 33 + c.line)));
    }
    send(INPUT, out);
    return true;
}

static int buttonCode(Qt::MouseButton b) {
    switch (b) {
    case Qt::LeftButton: return 0;
    case Qt::MiddleButton: return 1;
    case Qt::RightButton: return 2;
    default: return 3;
    }
}

void TerminalView::selectionChanged(int, int) { markAll(); }

void TerminalView::mousePressEvent(QMouseEvent* e) {
    forceActiveFocus(Qt::MouseFocusReason);
    m_pressedButton = buttonCode(e->button());
    if (reportMouse(e, m_pressedButton, true, false)) return;
    if (e->button() == Qt::LeftButton) {
        m_selA = m_selB = cellAt(e->position());
        m_selecting = true;
        if (m_hasSelection) {
            m_hasSelection = false;
            markAll();
        }
    } else if (e->button() == Qt::MiddleButton) {
        QString text = QGuiApplication::clipboard()->text(QClipboard::Selection);
        if (!text.isEmpty()) {
            text.replace(QLatin1Char('\n'), QLatin1Char('\r'));
            QByteArray b = text.toUtf8();
            if (m_mode & BRACKETED_PASTE) b = "\x1b[200~" + b + "\x1b[201~";
            sendInput(b);
        }
    }
}

void TerminalView::mouseMoveEvent(QMouseEvent* e) {
    int button = (e->buttons() & Qt::LeftButton) ? 0 : (e->buttons() & Qt::MiddleButton) ? 1 : (e->buttons() & Qt::RightButton) ? 2 : 3;
    if (reportMouse(e, button, true, true)) return;
    if (!m_selecting) return;
    Pos c = cellAt(e->position());
    if (c.col == m_selB.col && c.line == m_selB.line) return;
    m_selB = c;
    m_hasSelection = true;
    selectionChanged(0, 0);
}

void TerminalView::mouseReleaseEvent(QMouseEvent* e) {
    if (reportMouse(e, buttonCode(e->button()), false, false)) return;
    if (m_selecting && m_hasSelection) QGuiApplication::clipboard()->setText(selectionText(), QClipboard::Selection);
    m_selecting = false;
}

void TerminalView::mouseDoubleClickEvent(QMouseEvent* e) {
    if (reportMouse(e, buttonCode(e->button()), true, false)) return;
    if (e->button() != Qt::LeftButton || m_cells.empty()) return;
    // Word under the pointer: letters, digits and the characters of paths and URLs
    Pos c = cellAt(e->position());
    auto isWord = [&](int col) {
        char32_t ch = m_cells[size_t(c.line) * m_cols + col].c;
        return ch > ' ' && !QStringLiteral("\"'`()[]{}<>|;,│").contains(QChar(ch < 0x10000 ? ch : 0));
    };
    if (!isWord(c.col)) return;
    int a = c.col, b = c.col;
    while (a > 0 && isWord(a - 1)) --a;
    while (b < m_cols - 1 && isWord(b + 1)) ++b;
    m_selA = {a, c.line};
    m_selB = {b, c.line};
    m_hasSelection = true;
    m_selecting = false;
    QGuiApplication::clipboard()->setText(selectionText(), QClipboard::Selection);
    markAll();
}

void TerminalView::wheelEvent(QWheelEvent* e) {
    int steps = e->angleDelta().y() / 40;  // 3 lines per notch
    if (steps == 0) steps = e->pixelDelta().y() / int(m_ch);
    if (steps == 0) return;
    if ((m_mode & MOUSE_MODE) && !(e->modifiers() & Qt::ShiftModifier)) {
        Pos c = cellAt(e->position());
        int b = steps > 0 ? 64 : 65;
        for (int i = 0; i < qAbs(steps); ++i) {
            QByteArray out = (m_mode & SGR_MOUSE)
                ? "\x1b[<" + QByteArray::number(b) + ";" + QByteArray::number(c.col + 1) + ";" + QByteArray::number(c.line + 1) + "M"
                : QByteArray("\x1b[M") + char(32 + b) + char(33 + c.col) + char(33 + c.line);
            send(INPUT, out);
        }
        return;
    }
    if ((m_mode & ALT_SCREEN) && (m_mode & ALTERNATE_SCROLL)) {
        QByteArray key = (m_mode & APP_CURSOR) ? (steps > 0 ? "\x1bOA" : "\x1bOB") : (steps > 0 ? "\x1b[A" : "\x1b[B");
        for (int i = 0; i < qAbs(steps); ++i) send(INPUT, key);
        return;
    }
    QByteArray b;
    b.append(char(0));
    put32(b, quint32(qint32(steps)));
    send(SCROLL, b);
}

// ---------------------------------------------------------------- text of the screen

QString TerminalView::lineText(int line, int from, int to) const {
    QString s;
    for (int col = from; col <= to && col < m_cols; ++col) {
        const Cell& c = m_cells[size_t(line) * m_cols + col];
        if (c.flags & (WIDE_CHAR_SPACER | LEADING_WIDE_CHAR_SPACER)) continue;
        s += c.c ? QString::fromUcs4(&c.c, 1) : QStringLiteral(" ");
        if (c.extra) {
            auto it = m_extras.find(line * m_cols + col);
            if (it != m_extras.end()) s += it->second;
        }
    }
    return s;
}

QString TerminalView::selectionText() const {
    if (!m_hasSelection || m_cells.empty()) return {};
    Pos a = m_selA, b = m_selB;
    if (b.line < a.line || (b.line == a.line && b.col < a.col)) std::swap(a, b);
    QStringList lines;
    for (int line = a.line; line <= b.line; ++line) {
        int from = line == a.line ? a.col : 0;
        int to = line == b.line ? b.col : m_cols - 1;
        QString t = lineText(line, from, to);
        // A line the program wrapped continues on the next one without a newline
        bool wrapped = m_cells[size_t(line) * m_cols + m_cols - 1].flags & 0x10;
        QString out;
        if (wrapped && line != b.line) {
            out = t;
        } else {
            while (t.endsWith(QLatin1Char(' '))) t.chop(1);
            out = t + (line == b.line ? QString() : QStringLiteral("\n"));
        }
        lines << out;
    }
    return lines.join(QString());
}

extern "C" void fili_term_register_types() {
    // The font travels with the binary: the terminal looks the same on any system
    QFontDatabase::addApplicationFont(QStringLiteral(":/qt/qml/Fili/Term/fonts/JetBrainsMono-Variable.ttf"));
    qmlRegisterType<TerminalView>("Fili.TermView", 1, 0, "TerminalView");
}

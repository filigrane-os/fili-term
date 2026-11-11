// TerminalView (QML: import Fili.TermView; TerminalView {}): one terminal session drawn with QPainter.
// The session lives in the daemon (src/daemon.rs); this item only receives the damaged lines of the screen over the
// daemon's socket, paints those lines, and sends keys, mouse reports, size and scroll requests back.
#pragma once
#include <QElapsedTimer>
#include <QFont>
#include <QLocalSocket>
#include <QQuickPaintedItem>
#include <QStringList>
#include <unordered_map>
#include <vector>

class TerminalView : public QQuickPaintedItem {
    Q_OBJECT
    // Session to attach to (0: start a new one with `cwd` and `command`)
    Q_PROPERTY(int sessionId READ sessionId WRITE setSessionId NOTIFY sessionIdChanged)
    Q_PROPERTY(QString cwd MEMBER m_cwd)
    Q_PROPERTY(QStringList command MEMBER m_command)
    Q_PROPERTY(QString fontFamily READ fontFamily WRITE setFontFamily NOTIFY fontChanged)
    Q_PROPERTY(int fontPixelSize READ fontPixelSize WRITE setFontPixelSize NOTIFY fontChanged)
    Q_PROPERTY(int padding READ padding WRITE setPadding NOTIFY fontChanged)
    Q_PROPERTY(QString title READ title NOTIFY titleChanged)
    Q_PROPERTY(int scrollOffset READ scrollOffset NOTIFY scrollChanged)
    Q_PROPERTY(int historySize READ historySize NOTIFY scrollChanged)
    Q_PROPERTY(int columns READ columns NOTIFY gridChanged)
    Q_PROPERTY(int rows READ rows NOTIFY gridChanged)

public:
    explicit TerminalView(QQuickItem* parent = nullptr);
    ~TerminalView() override;

    int sessionId() const { return m_session; }
    void setSessionId(int id);
    QString fontFamily() const { return m_font.family(); }
    void setFontFamily(const QString& f);
    int fontPixelSize() const { return m_font.pixelSize(); }
    void setFontPixelSize(int px);
    int padding() const { return m_pad; }
    void setPadding(int p);
    QString title() const { return m_title; }
    int scrollOffset() const { return m_offset; }
    int historySize() const { return m_history; }
    int columns() const { return m_cols; }
    int rows() const { return m_rows; }

    Q_INVOKABLE void copy();
    Q_INVOKABLE void paste();
    Q_INVOKABLE void scrollPage(int direction);
    // Hang the session up (the tab or window closes)
    Q_INVOKABLE void closeSession();

    void paint(QPainter* painter) override;

signals:
    void sessionIdChanged();
    void fontChanged();
    void titleChanged();
    void scrollChanged();
    void gridChanged();
    void exited(int status);
    void bell();

protected:
    void componentComplete() override;
    void geometryChange(const QRectF& newGeometry, const QRectF& oldGeometry) override;
    void itemChange(ItemChange change, const ItemChangeData& value) override;
    void keyPressEvent(QKeyEvent* event) override;
    void inputMethodEvent(QInputMethodEvent* event) override;
    QVariant inputMethodQuery(Qt::InputMethodQuery query) const override;
    void focusInEvent(QFocusEvent* event) override;
    void focusOutEvent(QFocusEvent* event) override;
    void mousePressEvent(QMouseEvent* event) override;
    void mouseMoveEvent(QMouseEvent* event) override;
    void mouseReleaseEvent(QMouseEvent* event) override;
    void mouseDoubleClickEvent(QMouseEvent* event) override;
    void wheelEvent(QWheelEvent* event) override;

private:
    struct Cell {
        char32_t c = ' ';
        bool extra = false;
        quint16 flags = 0;
        quint32 fg = 0x01000100;  // named foreground
        quint32 bg = 0x01000101;  // named background
    };
    struct Pos {
        int col = 0, line = 0;
    };

    void connectDaemon();
    void onReadyRead();
    void handle(quint8 kind, const QByteArray& payload);
    void applyFrame(const QByteArray& payload);
    void send(quint8 kind, const QByteArray& payload);
    void sendInput(const QByteArray& bytes);
    void sendSize();
    void measure();
    bool computeGrid();
    void markLine(int line);
    void markAll();
    QColor resolve(quint32 color) const;
    bool simpleGlyph(char32_t c) const;
    void paintLine(QPainter* p, int line);
    Pos cellAt(QPointF pos) const;
    bool inSelection(int col, int line) const;
    QString selectionText() const;
    QString lineText(int line, int from, int to) const;
    bool reportMouse(QMouseEvent* event, int button, bool press, bool motion);
    void selectionChanged(int fromLine, int toLine);

    QLocalSocket m_sock;
    QByteArray m_in;
    int m_session = 0;
    QString m_cwd;
    QStringList m_command;
    bool m_complete = false;
    bool m_connecting = false;
    bool m_needAck = false;
    bool m_full = true;  // next paint covers the whole item

    QFont m_font, m_bold, m_italic, m_boldItalic;
    qreal m_cw = 7, m_ch = 15, m_ascent = 11;
    int m_pad = 10;

    int m_cols = 0, m_rows = 0;          // grid as sent by the daemon
    int m_wantCols = 0, m_wantRows = 0;  // grid that fits the item
    std::vector<Cell> m_cells;
    std::unordered_map<int, QString> m_extras;  // index → zero-width characters (combining, VS16…)
    quint32 m_mode = 0;
    int m_curCol = 0, m_curLine = -1, m_curShape = 1;
    int m_offset = 0, m_history = 0;
    QString m_title;
    bool m_focused = false;
    QString m_preedit;

    // Selection in screen cells (anchor → head), PRIMARY on release
    bool m_selecting = false, m_hasSelection = false;
    Pos m_selA, m_selB;
    int m_pressedButton = -1;
    Pos m_lastMouseCell{-1, -1};

    mutable std::unordered_map<char32_t, bool> m_inFont;
    QVector<QRgb> m_palette;

    // FILI_TERM_TRACE=1: keystroke → paint latency and frame sizes on stderr
    bool m_trace = false;
    QElapsedTimer m_clock;
    qint64 m_keyAt = -1;
};

extern "C" void fili_term_register_types();

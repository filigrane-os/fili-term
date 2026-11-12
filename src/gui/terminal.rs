//! TerminalView (QML: `TerminalView {}` in Fili.Term): one terminal session painted with QPainter.
//!
//! The session lives in the daemon (daemon/src/session.rs); this item receives the damaged lines of the screen over
//! the daemon's socket, paints those lines, and sends keys, mouse reports, size and scroll requests back. A reader
//! thread parses the daemon's messages and hands them to the Qt thread; frames are acknowledged once painted, so the
//! daemon never sends faster than the window paints.

use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::pin::Pin;
use std::time::Instant;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{
    KeyboardModifiers, MouseButton, MouseButtons, QColor, QFont, QFontHintingPreference, QFontSpacingType, QFontStyleHint,
    QFontStyleStrategy, QPen, QPoint, QPointF, QRect, QRectF, QSize, QString, QStringList, QVariant,
};

use crate::proto::{self, Reader};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("cxx-qt-lib/qrect.h");
        type QRect = cxx_qt_lib::QRect;
        include!("cxx-qt-lib/qrectf.h");
        type QRectF = cxx_qt_lib::QRectF;
        include!("cxx-qt-lib/qpoint.h");
        type QPoint = cxx_qt_lib::QPoint;
        include!("cxx-qt-lib/qpointf.h");
        type QPointF = cxx_qt_lib::QPointF;
        include!("cxx-qt-lib/qsize.h");
        type QSize = cxx_qt_lib::QSize;
        include!("cxx-qt-lib/qcolor.h");
        type QColor = cxx_qt_lib::QColor;
        include!("cxx-qt-lib/qfont.h");
        type QFont = cxx_qt_lib::QFont;
        include!("cxx-qt-lib/qpainter.h");
        type QPainter = cxx_qt_lib::QPainter;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    #[namespace = "Qt"]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qt.h");
        type KeyboardModifiers = cxx_qt_lib::KeyboardModifiers;
        type MouseButtons = cxx_qt_lib::MouseButtons;
        type MouseButton = cxx_qt_lib::MouseButton;
    }

    // Qt classes cxx-qt-lib does not wrap: declared here, bound straight to Qt's own members
    unsafe extern "C++" {
        include!(<QtGui/QKeyEvent>);
        type QKeyEvent;
        type QMouseEvent;
        type QWheelEvent;
        type QInputMethodEvent;
        type QFocusEvent;

        fn key(self: &QKeyEvent) -> i32;
        fn modifiers(self: &QKeyEvent) -> KeyboardModifiers;
        fn text(self: &QKeyEvent) -> QString;
        fn accept(self: Pin<&mut QKeyEvent>);
        fn ignore(self: Pin<&mut QKeyEvent>);

        fn position(self: &QMouseEvent) -> QPointF;
        fn button(self: &QMouseEvent) -> MouseButton;
        fn buttons(self: &QMouseEvent) -> MouseButtons;
        fn modifiers(self: &QMouseEvent) -> KeyboardModifiers;

        #[cxx_name = "angleDelta"]
        fn angle_delta(self: &QWheelEvent) -> QPoint;
        #[cxx_name = "pixelDelta"]
        fn pixel_delta(self: &QWheelEvent) -> QPoint;
        fn position(self: &QWheelEvent) -> QPointF;
        fn modifiers(self: &QWheelEvent) -> KeyboardModifiers;

        #[cxx_name = "commitString"]
        fn commit_string(self: &QInputMethodEvent) -> &QString;
        #[cxx_name = "preeditString"]
        fn preedit_string(self: &QInputMethodEvent) -> &QString;
        fn accept(self: Pin<&mut QInputMethodEvent>);

        include!(<QtGui/QFontMetricsF>);
        type QFontMetricsF;
        #[cxx_name = "horizontalAdvance"]
        fn horizontal_advance(self: &QFontMetricsF, text: &QString, length: i32) -> f64;
        fn ascent(self: &QFontMetricsF) -> f64;
        fn height(self: &QFontMetricsF) -> f64;
        #[cxx_name = "inFontUcs4"]
        fn in_font_ucs4(self: &QFontMetricsF, ucs4: u32) -> bool;

        include!(<QtGui/QClipboard>);
        type QClipboard;
        #[cxx_name = "setText"]
        fn set_text(self: Pin<&mut QClipboard>, text: &QString, mode: ClipboardMode);
        fn text(self: &QClipboard, mode: ClipboardMode) -> QString;
        include!(<QtGui/QGuiApplication>);
        #[cxx_name = "QGuiApplication"]
        type GuiApplication;
        #[Self = "GuiApplication"]
        fn clipboard() -> *mut QClipboard;
    }

    #[namespace = "rust::cxxqtlib1"]
    unsafe extern "C++" {
        include!("cxx-qt-lib/common.h");
        #[cxx_name = "make_unique"]
        fn font_metrics_new(font: &QFont) -> UniquePtr<QFontMetricsF>;
    }

    /// Qt::InputMethodQuery (the values the terminal answers)
    #[namespace = "Qt"]
    #[repr(i32)]
    enum InputMethodQuery {
        ImEnabled = 0x1,
        ImCursorRectangle = 0x2,
        ImFont = 0x4,
        ImHints = 0x100,
    }

    unsafe extern "C++" {
        #[namespace = "Qt"]
        type InputMethodQuery;
    }

    #[namespace = "QClipboard"]
    unsafe extern "C++" {
        #[cxx_name = "Mode"]
        type ClipboardMode = super::ClipboardMode;
    }

    unsafe extern "C++Qt" {
        include!(<QtQuick/QQuickPaintedItem>);
        #[qobject]
        type QQuickPaintedItem;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickPaintedItem]
        #[qproperty(i32, session_id, cxx_name = "sessionId")]
        #[qproperty(QString, cwd)]
        #[qproperty(QStringList, command)]
        #[qproperty(QString, font_family, cxx_name = "fontFamily")]
        #[qproperty(i32, font_pixel_size, cxx_name = "fontPixelSize")]
        #[qproperty(i32, padding)]
        #[qproperty(QString, title)]
        #[qproperty(i32, scroll_offset, cxx_name = "scrollOffset")]
        #[qproperty(i32, history_size, cxx_name = "historySize")]
        /// Columns and rows of the grid
        #[qproperty(i32, columns)]
        #[qproperty(i32, lines)]
        /// "fg bg selection cursor c0 … c15" (#rrggbb), from the theme
        #[qproperty(QString, colors)]
        type TerminalView = super::TerminalViewRust;

        /// The shell ended (status), or the daemon went away (-1)
        #[qsignal]
        fn exited(self: Pin<&mut TerminalView>, status: i32);
        #[qsignal]
        fn bell(self: Pin<&mut TerminalView>);

        #[qinvokable]
        fn copy(self: Pin<&mut TerminalView>);
        #[qinvokable]
        fn paste(self: Pin<&mut TerminalView>);
        #[qinvokable]
        #[cxx_name = "scrollPage"]
        /// -2 top of the history, -1 page up, 1 page down, 0 or 2 back to the bottom
        fn scroll_page(self: Pin<&mut TerminalView>, direction: i32);
        /// Hang the session up (its tab or window closes)
        #[qinvokable]
        #[cxx_name = "closeSession"]
        fn close_session(self: Pin<&mut TerminalView>);
        /// The tab is shown or hidden (a hidden tab keeps no screen-sized image)
        #[qinvokable]
        #[cxx_name = "setShown"]
        fn set_shown(self: Pin<&mut TerminalView>, shown: bool);
    }

    unsafe extern "RustQt" {
        #[cxx_override]
        unsafe fn paint(self: Pin<&mut TerminalView>, painter: *mut QPainter);
        #[cxx_override]
        #[cxx_name = "componentComplete"]
        fn component_complete(self: Pin<&mut TerminalView>);
        #[cxx_override]
        #[cxx_name = "geometryChange"]
        fn geometry_change(self: Pin<&mut TerminalView>, new_geometry: &QRectF, old_geometry: &QRectF);
        #[cxx_override]
        #[cxx_name = "keyPressEvent"]
        unsafe fn key_press_event(self: Pin<&mut TerminalView>, event: *mut QKeyEvent);
        #[cxx_override]
        #[cxx_name = "inputMethodEvent"]
        unsafe fn input_method_event(self: Pin<&mut TerminalView>, event: *mut QInputMethodEvent);
        #[cxx_override]
        #[cxx_name = "inputMethodQuery"]
        fn input_method_query(self: &TerminalView, query: InputMethodQuery) -> QVariant;
        #[cxx_override]
        #[cxx_name = "focusInEvent"]
        unsafe fn focus_in_event(self: Pin<&mut TerminalView>, event: *mut QFocusEvent);
        #[cxx_override]
        #[cxx_name = "focusOutEvent"]
        unsafe fn focus_out_event(self: Pin<&mut TerminalView>, event: *mut QFocusEvent);
        #[cxx_override]
        #[cxx_name = "mousePressEvent"]
        unsafe fn mouse_press_event(self: Pin<&mut TerminalView>, event: *mut QMouseEvent);
        #[cxx_override]
        #[cxx_name = "mouseMoveEvent"]
        unsafe fn mouse_move_event(self: Pin<&mut TerminalView>, event: *mut QMouseEvent);
        #[cxx_override]
        #[cxx_name = "mouseReleaseEvent"]
        unsafe fn mouse_release_event(self: Pin<&mut TerminalView>, event: *mut QMouseEvent);
        #[cxx_override]
        #[cxx_name = "mouseDoubleClickEvent"]
        unsafe fn mouse_double_click_event(self: Pin<&mut TerminalView>, event: *mut QMouseEvent);
        #[cxx_override]
        #[cxx_name = "wheelEvent"]
        unsafe fn wheel_event(self: Pin<&mut TerminalView>, event: *mut QWheelEvent);

        #[inherit]
        #[cxx_name = "componentComplete"]
        fn component_complete_super(self: Pin<&mut TerminalView>);
        #[inherit]
        #[cxx_name = "geometryChange"]
        fn geometry_change_super(self: Pin<&mut TerminalView>, new_geometry: &QRectF, old_geometry: &QRectF);
        #[inherit]
        #[cxx_name = "inputMethodQuery"]
        fn input_method_query_super(self: &TerminalView, query: InputMethodQuery) -> QVariant;
        #[inherit]
        fn update(self: Pin<&mut TerminalView>, rect: &QRect);
        #[inherit]
        fn width(self: &TerminalView) -> f64;
        #[inherit]
        fn height(self: &TerminalView) -> f64;
        #[inherit]
        #[cxx_name = "setAcceptedMouseButtons"]
        fn set_accepted_mouse_buttons(self: Pin<&mut TerminalView>, buttons: MouseButtons);
        #[inherit]
        #[cxx_name = "setOpaquePainting"]
        fn set_opaque_painting(self: Pin<&mut TerminalView>, opaque: bool);
        #[inherit]
        #[cxx_name = "setFillColor"]
        fn set_fill_color(self: Pin<&mut TerminalView>, color: &QColor);
        #[inherit]
        #[cxx_name = "setTextureSize"]
        fn set_texture_size(self: Pin<&mut TerminalView>, size: &QSize);
        #[inherit]
        #[cxx_name = "setActiveFocusOnTab"]
        fn set_active_focus_on_tab(self: Pin<&mut TerminalView>, on: bool);
        #[inherit]
        #[cxx_name = "forceActiveFocus"]
        fn force_active_focus(self: Pin<&mut TerminalView>);
    }

    impl cxx_qt::Threading for TerminalView {}
    impl cxx_qt::Constructor<()> for TerminalView {}
}

use qobject::InputMethodQuery;

/// QClipboard::Mode, a class-nested enum cxx cannot declare itself: same layout (an int), named by its C++ path
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct ClipboardMode(i32);

impl ClipboardMode {
    #[allow(non_upper_case_globals)]
    pub const Clipboard: Self = Self(0);
    #[allow(non_upper_case_globals)]
    pub const Selection: Self = Self(1);
}

// SAFETY: QClipboard::Mode is a plain enum of int size, trivially copyable
unsafe impl cxx::ExternType for ClipboardMode {
    type Id = cxx::type_id!("QClipboard::Mode");
    type Kind = cxx::kind::Trivial;
}

// Message kinds and bits shared with the daemon (alacritty_terminal's TermMode and cell Flags)
mod mode {
    pub const APP_CURSOR: u32 = 1 << 1;
    pub const MOUSE_REPORT_CLICK: u32 = 1 << 3;
    pub const BRACKETED_PASTE: u32 = 1 << 4;
    pub const SGR_MOUSE: u32 = 1 << 5;
    pub const MOUSE_MOTION: u32 = 1 << 6;
    pub const FOCUS_IN_OUT: u32 = 1 << 11;
    pub const ALT_SCREEN: u32 = 1 << 12;
    pub const MOUSE_DRAG: u32 = 1 << 13;
    pub const ALTERNATE_SCROLL: u32 = 1 << 15;
    pub const MOUSE_MODE: u32 = MOUSE_REPORT_CLICK | MOUSE_MOTION | MOUSE_DRAG;
}
mod flags {
    pub const INVERSE: u16 = 1;
    pub const BOLD: u16 = 2;
    pub const ITALIC: u16 = 4;
    pub const UNDERLINE: u16 = 8;
    pub const WRAPLINE: u16 = 0x10;
    pub const WIDE_CHAR: u16 = 0x20;
    pub const WIDE_CHAR_SPACER: u16 = 0x40;
    pub const DIM: u16 = 0x80;
    pub const HIDDEN: u16 = 0x100;
    pub const STRIKEOUT: u16 = 0x200;
    pub const LEADING_WIDE_CHAR_SPACER: u16 = 0x400;
    pub const DOUBLE_UNDERLINE: u16 = 0x800;
    pub const ALL_UNDERLINES: u16 = UNDERLINE | DOUBLE_UNDERLINE | 0x1000 | 0x2000 | 0x4000;
    pub const SPACERS: u16 = WIDE_CHAR_SPACER | LEADING_WIDE_CHAR_SPACER;
}

/// Colours of the terminal: the 16 ANSI colours, text, background, selection and cursor. Set by QML (`colors`, from
/// the theme: FiliTheme.terminalColors); the defaults are the dark palette of Filigrane's foot theme.
#[derive(Clone, Copy)]
struct Pal {
    ansi: [u32; 16],
    fg: u32,
    bg: u32,
    selection: u32,
    cursor: u32,
}

impl Default for Pal {
    fn default() -> Self {
        Self {
            ansi: [
                0x2d3744, 0xc98f93, 0xa3bfa8, 0xd2bf92, 0x9dbbd6, 0xb3a8c9, 0x95c0c6, 0xa9b8c8, 0x5f7894, 0xd9a5a9, 0xb8d0bc,
                0xe0d0a8, 0xb9d0e4, 0xc6bdd8, 0xadd2d7, 0xdbe7f4,
            ],
            fg: 0xdbe7f4,
            bg: 0x1c222b,
            selection: 0x3c4858,
            cursor: 0xdbe7f4,
        }
    }
}

impl Pal {
    /// "fg bg selection cursor c0 … c15"; None unless all 20 colours are valid
    fn parse(text: &str) -> Option<Self> {
        // #rgb, #rrggbb or #aarrggbb (alpha dropped: the terminal is opaque)
        let hex = |c: &str| -> Option<u32> {
            let h = c.strip_prefix('#')?;
            let v = u32::from_str_radix(h, 16).ok()?;
            match h.len() {
                3 => Some(((v >> 8 & 0xf) * 0x11) << 16 | ((v >> 4 & 0xf) * 0x11) << 8 | (v & 0xf) * 0x11),
                6 | 8 => Some(v & 0xff_ffff),
                _ => None,
            }
        };
        let v: Vec<u32> = text.split_whitespace().map(hex).collect::<Option<_>>()?;
        if v.len() != 20 {
            return None;
        }
        let mut ansi = [0; 16];
        ansi.copy_from_slice(&v[4..]);
        Some(Self { ansi, fg: v[0], bg: v[1], selection: v[2], cursor: v[3] })
    }
}

// Qt::Key values and modifier bits
mod key {
    pub const ESCAPE: i32 = 0x0100_0000;
    pub const TAB: i32 = 0x0100_0001;
    pub const BACKTAB: i32 = 0x0100_0002;
    pub const BACKSPACE: i32 = 0x0100_0003;
    pub const RETURN: i32 = 0x0100_0004;
    pub const ENTER: i32 = 0x0100_0005;
    pub const INSERT: i32 = 0x0100_0006;
    pub const DELETE: i32 = 0x0100_0007;
    pub const HOME: i32 = 0x0100_0010;
    pub const END: i32 = 0x0100_0011;
    pub const LEFT: i32 = 0x0100_0012;
    pub const UP: i32 = 0x0100_0013;
    pub const RIGHT: i32 = 0x0100_0014;
    pub const DOWN: i32 = 0x0100_0015;
    pub const PAGE_UP: i32 = 0x0100_0016;
    pub const PAGE_DOWN: i32 = 0x0100_0017;
    pub const F1: i32 = 0x0100_0030;
    pub const F12: i32 = 0x0100_003b;
    pub const SHIFT: u32 = 0x0200_0000;
    pub const CONTROL: u32 = 0x0400_0000;
    pub const ALT: u32 = 0x0800_0000;
}

fn rgb(v: u32) -> QColor {
    QColor::from_rgb((v >> 16 & 0xff) as i32, (v >> 8 & 0xff) as i32, (v & 0xff) as i32)
}

fn dim(v: u32) -> u32 {
    let c = |s: u32| ((v >> s & 0xff) * 2 / 3) << s;
    c(16) | c(8) | c(0)
}

/// Colour of the 256-colour palette
fn palette(i: u32, pal: &Pal) -> u32 {
    match i {
        0..=15 => pal.ansi[i as usize],
        16..=231 => {
            const STEPS: [u32; 6] = [0, 95, 135, 175, 215, 255];
            let i = i - 16;
            STEPS[(i / 36) as usize] << 16 | STEPS[(i / 6 % 6) as usize] << 8 | STEPS[(i % 6) as usize]
        }
        _ => {
            let g = 8 + 10 * (i.min(255) - 232);
            g << 16 | g << 8 | g
        }
    }
}

/// Wire colour (see daemon `color`) → 0xRRGGBB
fn resolve(c: u32, pal: &Pal) -> u32 {
    let (tag, v) = (c >> 24, c & 0xff_ffff);
    match tag {
        0 => v,
        2 => palette(v & 0xff, pal),
        _ => match v {
            0..=15 => pal.ansi[v as usize],
            257 => pal.bg,
            259..=266 => dim(pal.ansi[(v - 259) as usize]),
            268 => dim(pal.fg),
            _ => pal.fg,
        },
    }
}

#[derive(Clone, Copy)]
struct Cell {
    c: char,
    fg: u32,
    bg: u32,
    flags: u16,
    extra: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self { c: ' ', fg: 0x0100_0100, bg: 0x0100_0101, flags: 0, extra: false }
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
struct Pos {
    col: i32,
    line: i32,
}

/// A FRAME message decoded by the reader thread
struct Frame {
    cols: usize,
    rows: usize,
    mode: u32,
    cursor_col: i32,
    cursor_line: i32,
    cursor_shape: u8,
    offset: i32,
    history: i32,
    lines: Vec<(usize, Vec<Cell>, Vec<(usize, String)>)>,
}

enum Message {
    Session(u32),
    Frame(Frame),
    Title(String),
    Clipboard(String),
    Bell,
    Exit(i32),
    Gone,
}

/// Socket to the daemon; shutting it down ends the reader thread
struct Conn(UnixStream);

impl Drop for Conn {
    fn drop(&mut self) {
        let _ = self.0.shutdown(std::net::Shutdown::Both);
    }
}

/// Regular, bold, italic, bold italic (index: bold + 2 × italic), and the metrics of the regular face
pub struct Fonts {
    faces: [QFont; 4],
    metrics: cxx::UniquePtr<qobject::QFontMetricsF>,
}

pub struct TerminalViewRust {
    session_id: i32,
    cwd: QString,
    command: QStringList,
    font_family: QString,
    font_pixel_size: i32,
    padding: i32,
    title: QString,
    scroll_offset: i32,
    history_size: i32,
    columns: i32,
    lines: i32,
    colors: QString,
    pal: Pal,

    conn: Option<Conn>,
    complete: bool,
    need_ack: bool,
    full: bool,
    fonts: Option<Fonts>,
    cw: i32,
    ch: i32,
    ascent: i32,
    cols: usize,
    rows: usize,
    want_cols: usize,
    want_rows: usize,
    cells: Vec<Cell>,
    extras: HashMap<usize, String>,
    mode: u32,
    cur_col: i32,
    cur_line: i32,
    cur_shape: u8,
    focused: bool,
    preedit: String,
    selecting: bool,
    has_selection: bool,
    sel_a: Pos,
    sel_b: Pos,
    last_mouse: Pos,
    in_font: HashMap<char, bool>,
    // FILI_TERM_TRACE=1: keystroke → paint latency and frame sizes on stderr
    trace: bool,
    key_at: Option<Instant>,
}

impl Default for TerminalViewRust {
    fn default() -> Self {
        Self {
            session_id: 0,
            cwd: QString::default(),
            command: QStringList::default(),
            font_family: QString::from("JetBrains Mono"),
            font_pixel_size: 12,
            padding: 10,
            title: QString::default(),
            scroll_offset: 0,
            history_size: 0,
            columns: 0,
            lines: 0,
            colors: QString::default(),
            pal: Pal::default(),
            conn: None,
            complete: false,
            need_ack: false,
            full: true,
            fonts: None,
            cw: 7,
            ch: 15,
            ascent: 11,
            cols: 0,
            rows: 0,
            want_cols: 0,
            want_rows: 0,
            cells: Vec::new(),
            extras: HashMap::new(),
            mode: 0,
            cur_col: 0,
            cur_line: -1,
            cur_shape: 1,
            focused: false,
            preedit: String::new(),
            selecting: false,
            has_selection: false,
            sel_a: Pos::default(),
            sel_b: Pos::default(),
            last_mouse: Pos { col: -1, line: -1 },
            in_font: HashMap::new(),
            trace: std::env::var_os("FILI_TERM_TRACE").is_some(),
            key_at: None,
        }
    }
}

impl cxx_qt::Constructor<()> for qobject::TerminalView {
    type BaseArguments = ();
    type NewArguments = ();
    type InitializeArguments = ();

    fn route_arguments(_: ()) -> ((), (), ()) {
        ((), (), ())
    }

    fn new(_: ()) -> TerminalViewRust {
        TerminalViewRust::default()
    }

    fn initialize(mut self: Pin<&mut Self>, _: ()) {
        // Input method: enabled by the ImEnabled answer of input_method_query (QQuickItem::Flag is out of cxx's reach)
        let mut all = MouseButtons::from(MouseButton::LeftButton);
        all.set_flag(MouseButton::RightButton, true).set_flag(MouseButton::MiddleButton, true);
        self.as_mut().set_accepted_mouse_buttons(all);
        self.as_mut().set_opaque_painting(true);
        self.as_mut().set_fill_color(&rgb(Pal::default().bg));
        self.as_mut()
            .on_colors_changed(|mut v| {
                if let Some(pal) = Pal::parse(&v.colors.to_string()) {
                    v.as_mut().rust_mut().pal = pal;
                    v.as_mut().set_fill_color(&rgb(pal.bg));
                    v.mark_all();
                }
            })
            .release();
        self.as_mut().set_active_focus_on_tab(true);
        self.as_mut().measure();
        self.as_mut().on_font_family_changed(|v| v.measure()).release();
        self.as_mut().on_font_pixel_size_changed(|v| v.measure()).release();
        self.as_mut()
            .on_padding_changed(|mut v| {
                if v.as_mut().compute_grid() {
                    v.as_mut().send_size();
                }
                v.mark_all();
            })
            .release();
    }
}

// ---------------------------------------------------------------- connection

fn put16(b: &mut Vec<u8>, v: usize) {
    proto::put_u16(b, v.min(u16::MAX as usize) as u16);
}

/// Reader thread: daemon messages → Qt thread
fn read_loop(mut stream: UnixStream, thread: cxx_qt::CxxQtThread<qobject::TerminalView>) {
    loop {
        let msg = match proto::read_msg(&mut stream) {
            Ok((kind, payload)) => match decode(kind, &payload) {
                Some(m) => m,
                None => continue,
            },
            Err(_) => Message::Gone,
        };
        let gone = matches!(msg, Message::Gone | Message::Exit(_));
        if thread.queue(move |view| view.handle(msg)).is_err() || gone {
            return;
        }
    }
}

fn decode(kind: u8, payload: &[u8]) -> Option<Message> {
    let mut r = Reader::new(payload);
    Some(match kind {
        proto::SESSION => Message::Session(r.u32().ok()?),
        proto::FRAME => Message::Frame(decode_frame(&mut r).ok()?),
        proto::TITLE => Message::Title(r.string().ok()?),
        proto::CLIPBOARD => Message::Clipboard(r.string().ok()?),
        proto::BELL => Message::Bell,
        proto::EXIT => Message::Exit(r.i32().ok()?),
        _ => return None,
    })
}

fn decode_frame(r: &mut Reader<'_>) -> std::io::Result<Frame> {
    let cols = r.u16()? as usize;
    let rows = r.u16()? as usize;
    let mode = r.u32()?;
    let cursor_col = r.u16()? as i32;
    let cursor_line = r.u16()? as i16 as i32;
    let cursor_shape = r.u8()?;
    let offset = r.u32()? as i32;
    let history = r.u32()? as i32;
    r.u8()?; // full
    let count = r.u16()? as usize;
    let mut lines = Vec::with_capacity(count);
    for _ in 0..count {
        let line = r.u16()? as usize;
        let mut cells = Vec::with_capacity(cols);
        let mut extras = Vec::new();
        for col in 0..cols {
            let c = r.u32()?;
            let fg = r.u32()?;
            let bg = r.u32()?;
            let flags = r.u16()?;
            let extra = c & 0x8000_0000 != 0;
            if extra {
                let n = r.u8()?;
                let mut z = String::new();
                for _ in 0..n {
                    z.extend(char::from_u32(r.u32()?));
                }
                extras.push((col, z));
            }
            cells.push(Cell { c: char::from_u32(c & 0x1f_ffff).unwrap_or(' '), fg, bg, flags, extra });
        }
        lines.push((line, cells, extras));
    }
    Ok(Frame { cols, rows, mode, cursor_col, cursor_line, cursor_shape, offset, history, lines })
}

fn sgr_or_x10(mode: u32, button: i32, col: i32, line: i32, press: bool) -> Vec<u8> {
    if mode & mode::SGR_MOUSE != 0 {
        format!("\x1b[<{button};{};{}{}", col + 1, line + 1, if press { 'M' } else { 'm' }).into_bytes()
    } else {
        let b = if press { button } else { 3 + (button & !3) };
        vec![0x1b, b'[', b'M', (32 + b).min(255) as u8, (33 + col).min(255) as u8, (33 + line).min(255) as u8]
    }
}

fn clipboard_text(mode: ClipboardMode) -> String {
    let cb = qobject::GuiApplication::clipboard();
    if cb.is_null() {
        return String::new();
    }
    // SAFETY: the application's clipboard lives as long as the application
    unsafe { (*cb).text(mode) }.to_string()
}

/// A finished selection goes to the primary selection and to the clipboard (copy on select)
fn publish_selection(text: &str) {
    set_clipboard_text(text, ClipboardMode::Selection);
    set_clipboard_text(text, ClipboardMode::Clipboard);
}

fn set_clipboard_text(text: &str, mode: ClipboardMode) {
    let cb = qobject::GuiApplication::clipboard();
    if cb.is_null() {
        return;
    }
    // SAFETY: as above; Qt thread
    unsafe { Pin::new_unchecked(&mut *cb) }.set_text(&QString::from(text), mode);
}

impl qobject::TerminalView {
    fn send(&self, kind: u8, payload: &[u8]) {
        if let Some(conn) = &self.conn {
            let _ = proto::write_msg(&mut &conn.0, kind, payload);
        }
    }

    fn send_input(mut self: Pin<&mut Self>, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        if self.has_selection {
            self.as_mut().rust_mut().has_selection = false;
            self.as_mut().mark_all();
        }
        self.send(proto::INPUT, bytes);
    }

    fn size_payload(&self) -> Vec<u8> {
        let mut b = Vec::new();
        put16(&mut b, self.want_cols);
        put16(&mut b, self.want_rows);
        put16(&mut b, self.cw as usize);
        put16(&mut b, self.ch as usize);
        b
    }

    fn send_size(self: Pin<&mut Self>) {
        let b = self.size_payload();
        self.send(proto::RESIZE, &b);
    }

    fn connect_daemon(mut self: Pin<&mut Self>) {
        if self.conn.is_some() {
            return;
        }
        self.as_mut().compute_grid();
        let Ok(stream) = UnixStream::connect(proto::daemon_socket()) else {
            self.exited(-1);
            return;
        };
        let mut b = Vec::new();
        if self.session_id > 0 {
            proto::put_u32(&mut b, self.session_id as u32);
            b.extend(self.size_payload());
            let _ = proto::write_msg(&mut &stream, proto::ATTACH, &b);
        } else {
            b.extend(self.size_payload());
            proto::put_string(&mut b, &self.cwd.to_string());
            let argv: Vec<String> = self.command.iter().map(|s| s.to_string()).take(255).collect();
            b.push(argv.len() as u8);
            for a in &argv {
                proto::put_string(&mut b, a);
            }
            let _ = proto::write_msg(&mut &stream, proto::NEW, &b);
        }
        let Ok(reader) = stream.try_clone() else { return };
        let thread = self.qt_thread();
        std::thread::spawn(move || read_loop(reader, thread));
        self.as_mut().rust_mut().conn = Some(Conn(stream));
    }

    fn handle(mut self: Pin<&mut Self>, msg: Message) {
        match msg {
            Message::Session(id) => self.set_session_id(id as i32),
            Message::Frame(f) => self.apply_frame(f),
            Message::Title(t) => self.set_title(QString::from(&t)),
            Message::Clipboard(t) => set_clipboard_text(&t, ClipboardMode::Clipboard),
            Message::Bell => self.bell(),
            Message::Exit(status) => {
                self.as_mut().rust_mut().conn = None;
                self.exited(status);
            }
            Message::Gone => {
                if self.conn.is_some() {
                    self.as_mut().rust_mut().conn = None;
                    self.exited(-1);
                }
            }
        }
    }

    fn apply_frame(mut self: Pin<&mut Self>, f: Frame) {
        if f.cols == 0 || f.rows == 0 {
            return;
        }
        if self.trace {
            eprintln!("frame {} lines", f.lines.len());
        }
        if f.cols != self.cols || f.rows != self.rows {
            let mut r = self.as_mut().rust_mut();
            r.cols = f.cols;
            r.rows = f.rows;
            r.cells = vec![Cell::default(); f.cols * f.rows];
            r.extras.clear();
            r.has_selection = false;
            self.as_mut().mark_all();
        }
        if self.cur_line != f.cursor_line || self.cur_col != f.cursor_col || self.cur_shape != f.cursor_shape {
            let old = self.cur_line;
            self.as_mut().mark_line(old);
            self.as_mut().mark_line(f.cursor_line);
        }
        {
            let mut r = self.as_mut().rust_mut();
            r.mode = f.mode;
            r.cur_col = f.cursor_col;
            r.cur_line = f.cursor_line;
            r.cur_shape = f.cursor_shape;
        }
        if f.offset != self.scroll_offset {
            self.as_mut().set_scroll_offset(f.offset);
        }
        if f.history != self.history_size {
            self.as_mut().set_history_size(f.history);
        }
        let cols = self.cols;
        let mut dirty = Vec::with_capacity(f.lines.len());
        {
            let mut r = self.as_mut().rust_mut();
            for (line, cells, extras) in f.lines {
                if line >= r.rows || cells.len() != cols {
                    continue;
                }
                let base = line * cols;
                for col in 0..cols {
                    if r.cells[base + col].extra {
                        r.extras.remove(&(base + col));
                    }
                }
                r.cells[base..base + cols].copy_from_slice(&cells);
                for (col, z) in extras {
                    r.extras.insert(base + col, z);
                }
                dirty.push(line as i32);
            }
            r.need_ack = true;
        }
        for line in dirty {
            self.as_mut().mark_line(line);
        }
        // Nothing visible changed (title only…): still paint once so the daemon gets its acknowledgement
        let line = self.cur_line.max(0);
        self.mark_line(line);
    }

    // ------------------------------------------------------------ geometry

    /// Cell size from the font: whole pixels (crisp backgrounds), the glyph advance forced to the cell width so a
    /// run of text drawn at once lines up with the grid
    fn measure(mut self: Pin<&mut Self>) {
        let mut font = QFont::default();
        font.set_family(&self.font_family);
        font.set_pixel_size(self.font_pixel_size.max(4));
        font.set_style_hint(QFontStyleHint::Monospace, QFontStyleStrategy::PreferDefault);
        font.set_hinting_preference(QFontHintingPreference::PreferFullHinting);
        font.set_kerning(false);
        let (advance, height, ascent) = {
            let fm = qobject::font_metrics_new(&font);
            (fm.horizontal_advance(&QString::from("M"), -1), fm.height(), fm.ascent())
        };
        let cw = advance.round().max(1.0) as i32;
        let ch = height.ceil().max(1.0) as i32;
        font.set_letter_spacing(QFontSpacingType::AbsoluteSpacing, cw as f64 - advance);
        let mut bold = font.clone();
        bold.set_bold(true);
        let mut italic = font.clone();
        italic.set_italic(true);
        let mut bold_italic = bold.clone();
        bold_italic.set_italic(true);
        {
            let mut r = self.as_mut().rust_mut();
            r.cw = cw;
            r.ch = ch;
            r.ascent = (ascent + (ch as f64 - height) / 2.0).round() as i32;
            let metrics = qobject::font_metrics_new(&font);
            r.fonts = Some(Fonts { faces: [font, bold, italic, bold_italic], metrics });
            r.in_font.clear();
        }
        if self.as_mut().compute_grid() {
            self.as_mut().send_size();
        }
        self.mark_all();
    }

    fn compute_grid(mut self: Pin<&mut Self>) -> bool {
        let pad = self.padding as f64;
        let cols = (((self.width() - 2.0 * pad) / self.cw as f64) as usize).max(2);
        let rows = (((self.height() - 2.0 * pad) / self.ch as f64) as usize).max(1);
        if cols == self.want_cols && rows == self.want_rows {
            return false;
        }
        let mut r = self.as_mut().rust_mut();
        r.want_cols = cols;
        r.want_rows = rows;
        self.as_mut().set_columns(cols as i32);
        self.as_mut().set_lines(rows as i32);
        true
    }

    fn mark_line(self: Pin<&mut Self>, line: i32) {
        if line < 0 || line as usize >= self.rows {
            return;
        }
        let y = self.padding + line * self.ch;
        let w = self.width().ceil() as i32;
        let h = self.ch + 1;
        self.update(&QRect::new(0, y, w, h));
    }

    // Explicit rectangle: a bare update() followed by update(line) leaves Qt with the line alone to repaint
    fn mark_all(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().full = true;
        let (w, h) = (self.width().ceil() as i32, self.height().ceil() as i32);
        self.update(&QRect::new(0, 0, w, h));
    }

    // ------------------------------------------------------------ painting

    fn simple_glyph(mut self: Pin<&mut Self>, c: char) -> bool {
        if (c as u32) < 0x7f {
            return true;
        }
        if let Some(&ok) = self.in_font.get(&c) {
            return ok;
        }
        let ok = self.fonts.as_ref().is_some_and(|f| f.metrics.in_font_ucs4(c as u32));
        self.as_mut().rust_mut().in_font.insert(c, ok);
        ok
    }

    fn in_selection(&self, col: i32, line: i32) -> bool {
        if !self.has_selection {
            return false;
        }
        let (a, b) = ordered(self.sel_a, self.sel_b);
        if line < a.line || line > b.line {
            return false;
        }
        !(line == a.line && col < a.col || line == b.line && col > b.col)
    }

    /// Effective (fg, bg) of a cell
    fn cell_colors(&self, line: i32, col: usize) -> (u32, u32) {
        let c = &self.cells[line as usize * self.cols + col];
        let mut fg = resolve(c.fg, &self.pal);
        let mut bg = resolve(c.bg, &self.pal);
        if c.flags & flags::DIM != 0 {
            fg = dim(fg);
        }
        if c.flags & flags::INVERSE != 0 {
            std::mem::swap(&mut fg, &mut bg);
        }
        if self.in_selection(col as i32, line) {
            bg = self.pal.selection;
        }
        if c.flags & flags::HIDDEN != 0 {
            fg = bg;
        }
        (fg, bg)
    }

    fn paint_line(mut self: Pin<&mut Self>, p: &mut Pin<&mut cxx_qt_lib::QPainter>, line: i32) {
        if self.cells.is_empty() || self.fonts.is_none() {
            return;
        }
        // The fonts leave the object while it paints (glyph checks below need it mutable)
        let fonts = self.as_mut().rust_mut().fonts.take().unwrap();
        self.as_mut().paint_line_with(p, line, &fonts);
        self.as_mut().rust_mut().fonts = Some(fonts);
    }

    fn paint_line_with(mut self: Pin<&mut Self>, p: &mut Pin<&mut cxx_qt_lib::QPainter>, line: i32, fonts: &Fonts) {
        let (pad, cw, ch, ascent, cols) = (self.padding, self.cw, self.ch, self.ascent, self.cols);
        let y = pad + line * ch;
        let width = self.width();
        let base = line as usize * cols;

        // Backgrounds, in spans of one colour; the first and last columns reach into the padding, like foot
        let mut col = 0;
        while col < cols {
            let (_, bg) = self.cell_colors(line, col);
            let mut end = col + 1;
            while end < cols && self.cell_colors(line, end).1 == bg {
                end += 1;
            }
            if bg != self.pal.bg {
                let x0 = if col == 0 { 0.0 } else { (pad + col as i32 * cw) as f64 };
                let x1 = if end == cols { width } else { (pad + end as i32 * cw) as f64 };
                p.as_mut().fill_rect(&QRectF::new(x0, y as f64, x1 - x0, ch as f64), &rgb(bg));
            }
            col = end;
        }

        // Text: runs of cells with the same colour and style, made only of glyphs of the main font
        let mut run = String::new();
        let mut run_start = 0usize;
        let mut run_fg = 0u32;
        let mut run_style = 0u16;
        let font_for = |style: u16| -> &QFont {
            &fonts.faces[(style & flags::BOLD != 0) as usize + 2 * (style & flags::ITALIC != 0) as usize]
        };
        let flush = |p: &mut Pin<&mut cxx_qt_lib::QPainter>, run: &mut String, start: usize, fg: u32, style: u16| {
            if run.is_empty() {
                return;
            }
            p.as_mut().set_font(font_for(style));
            p.as_mut().set_pen(&QPen::from(&rgb(fg)));
            p.as_mut().draw_text(&QPoint::new(pad + start as i32 * cw, y + ascent), &QString::from(run.as_str()));
            run.clear();
        };
        for col in 0..cols {
            let c = self.cells[base + col];
            if c.flags & flags::SPACERS != 0 {
                flush(p, &mut run, run_start, run_fg, run_style);
                continue;
            }
            let (fg, _) = self.cell_colors(line, col);
            let style = c.flags & (flags::BOLD | flags::ITALIC);
            let wide = c.flags & flags::WIDE_CHAR != 0;
            let simple = !wide && !c.extra && self.as_mut().simple_glyph(c.c);
            if !simple {
                flush(p, &mut run, run_start, run_fg, run_style);
                if c.c == ' ' && !c.extra {
                    continue;
                }
                let mut text = c.c.to_string();
                if let Some(z) = self.extras.get(&(base + col)) {
                    text.push_str(z);
                }
                let text = QString::from(text.as_str());
                let font = font_for(style);
                let advance = fonts.metrics.horizontal_advance(&text, -1);
                let w = if wide { 2 * cw } else { cw } as f64;
                let x = pad + col as i32 * cw + ((w - advance) / 2.0).round() as i32;
                p.as_mut().set_font(font);
                p.as_mut().set_pen(&QPen::from(&rgb(fg)));
                p.as_mut().draw_text(&QPoint::new(x, y + ascent), &text);
                continue;
            }
            if !run.is_empty() && (fg != run_fg || style != run_style) {
                flush(p, &mut run, run_start, run_fg, run_style);
            }
            if run.is_empty() {
                run_start = col;
                run_fg = fg;
                run_style = style;
            }
            run.push(if c.c == '\0' { ' ' } else { c.c });
        }
        flush(p, &mut run, run_start, run_fg, run_style);

        // Decorations
        for col in 0..cols {
            let c = self.cells[base + col];
            if c.flags & (flags::ALL_UNDERLINES | flags::STRIKEOUT) == 0 {
                continue;
            }
            let fg = rgb(self.cell_colors(line, col).0);
            let x = (pad + col as i32 * cw) as f64;
            if c.flags & flags::ALL_UNDERLINES != 0 {
                p.as_mut().fill_rect(&QRectF::new(x, (y + ascent + 2) as f64, cw as f64, 1.0), &fg);
                if c.flags & flags::DOUBLE_UNDERLINE != 0 {
                    p.as_mut().fill_rect(&QRectF::new(x, (y + ascent + 4) as f64, cw as f64, 1.0), &fg);
                }
            }
            if c.flags & flags::STRIKEOUT != 0 {
                p.as_mut().fill_rect(&QRectF::new(x, (y + ch / 2) as f64, cw as f64, 1.0), &fg);
            }
        }

        // Cursor (hollow while the window is not focused)
        let cur_col = self.cur_col as usize;
        if line == self.cur_line && self.cur_shape != 0 && cur_col < cols && self.scroll_offset == 0 {
            let c = self.cells[base + cur_col];
            let w = if c.flags & flags::WIDE_CHAR != 0 { 2 * cw } else { cw };
            let x = pad + cur_col as i32 * cw;
            let fg = rgb(self.pal.cursor);
            if !self.focused || self.cur_shape == 4 {
                p.as_mut().set_pen(&QPen::from(&fg));
                p.as_mut().draw_rect_f(&QRectF::new(x as f64 + 0.5, y as f64 + 0.5, (w - 1) as f64, (ch - 1) as f64));
            } else if self.cur_shape == 1 {
                p.as_mut().fill_rect(&QRectF::new(x as f64, y as f64, w as f64, ch as f64), &fg);
                if c.c != ' ' && c.c != '\0' {
                    p.as_mut().set_font(font_for(c.flags & (flags::BOLD | flags::ITALIC)));
                    p.as_mut().set_pen(&QPen::from(&rgb(self.pal.bg)));
                    p.as_mut().draw_text(&QPoint::new(x, y + ascent), &QString::from(c.c.to_string().as_str()));
                }
            } else if self.cur_shape == 2 {
                p.as_mut().fill_rect(&QRectF::new(x as f64, (y + ch - 2) as f64, w as f64, 2.0), &fg);
            } else if self.cur_shape == 3 {
                p.as_mut().fill_rect(&QRectF::new(x as f64, y as f64, 2.0, ch as f64), &fg);
            }
        }

        // Text being composed by the input method, over the cursor
        if line == self.cur_line && !self.preedit.is_empty() {
            let text = QString::from(self.preedit.as_str());
            let w = fonts.metrics.horizontal_advance(&text, -1);
            let x = (pad + self.cur_col * cw) as f64;
            p.as_mut().fill_rect(&QRectF::new(x, y as f64, w, ch as f64), &rgb(self.pal.bg));
            p.as_mut().set_font(&fonts.faces[0]);
            p.as_mut().set_pen(&QPen::from(&rgb(self.pal.fg)));
            p.as_mut().draw_text(&QPoint::new(x as i32, y + ascent), &text);
            p.as_mut().fill_rect(&QRectF::new(x, (y + ascent + 2) as f64, w, 1.0), &rgb(self.pal.fg));
        }
    }

    // ------------------------------------------------------------ text of the screen

    fn line_text(&self, line: i32, from: i32, to: i32) -> String {
        let mut s = String::new();
        for col in from.max(0)..=to.min(self.cols as i32 - 1) {
            let idx = line as usize * self.cols + col as usize;
            let c = &self.cells[idx];
            if c.flags & flags::SPACERS != 0 {
                continue;
            }
            s.push(if c.c == '\0' { ' ' } else { c.c });
            if let Some(z) = self.extras.get(&idx) {
                s.push_str(z);
            }
        }
        s
    }

    fn selection_text(&self) -> String {
        if !self.has_selection || self.cells.is_empty() {
            return String::new();
        }
        let (a, b) = ordered(self.sel_a, self.sel_b);
        let mut out = String::new();
        for line in a.line..=b.line {
            let from = if line == a.line { a.col } else { 0 };
            let to = if line == b.line { b.col } else { self.cols as i32 - 1 };
            let t = self.line_text(line, from, to);
            // A line the program wrapped continues on the next one without a newline
            let wrapped = self.cells[line as usize * self.cols + self.cols - 1].flags & flags::WRAPLINE != 0;
            if wrapped && line != b.line {
                out.push_str(&t);
            } else {
                out.push_str(t.trim_end_matches(' '));
                if line != b.line {
                    out.push('\n');
                }
            }
        }
        out
    }

    fn cell_at(&self, pos: &QPointF) -> Pos {
        let pad = self.padding as f64;
        Pos {
            col: (((pos.x() - pad) / self.cw as f64) as i32).clamp(0, (self.cols as i32 - 1).max(0)),
            line: (((pos.y() - pad) / self.ch as f64) as i32).clamp(0, (self.rows as i32 - 1).max(0)),
        }
    }

    /// Mouse reporting for programs that asked for it (vim, htop…); Shift keeps the mouse for selection
    fn report_mouse(mut self: Pin<&mut Self>, mods: u32, pos: &QPointF, button: i32, press: bool, motion: bool) -> bool {
        let m = self.mode;
        if m & mode::MOUSE_MODE == 0 || mods & key::SHIFT != 0 {
            return false;
        }
        if motion && m & (mode::MOUSE_DRAG | mode::MOUSE_MOTION) == 0 {
            return true;
        }
        if motion && button == 3 && m & mode::MOUSE_MOTION == 0 {
            return true;
        }
        let c = self.cell_at(pos);
        if motion && c == self.last_mouse {
            return true;
        }
        self.as_mut().rust_mut().last_mouse = c;
        let mut b = button + if motion { 32 } else { 0 };
        if mods & key::ALT != 0 {
            b += 8;
        }
        if mods & key::CONTROL != 0 {
            b += 16;
        }
        let out = sgr_or_x10(m, b, c.col, c.line, press);
        self.send(proto::INPUT, &out);
        true
    }
}

fn ordered(a: Pos, b: Pos) -> (Pos, Pos) {
    if b.line < a.line || (b.line == a.line && b.col < a.col) { (b, a) } else { (a, b) }
}

fn button_code(b: MouseButton) -> i32 {
    match b {
        MouseButton::LeftButton => 0,
        MouseButton::MiddleButton => 1,
        MouseButton::RightButton => 2,
        _ => 3,
    }
}

fn mods_of(m: KeyboardModifiers) -> u32 {
    m.to_int() as u32
}

/// Bytes a key sends to the program
fn key_bytes(k: i32, mods: u32, text: &str, app_cursor: bool) -> Vec<u8> {
    let (shift, alt, ctrl) = (mods & key::SHIFT != 0, mods & key::ALT != 0, mods & key::CONTROL != 0);
    let m = 1 + shift as i32 + 2 * alt as i32 + 4 * ctrl as i32;
    let csi = |fin: &str, ss3: bool| -> Vec<u8> {
        if m > 1 {
            format!("\x1b[1;{m}{fin}").into_bytes()
        } else if ss3 && app_cursor {
            format!("\x1bO{fin}").into_bytes()
        } else {
            format!("\x1b[{fin}").into_bytes()
        }
    };
    let tilde =
        |n: i32| -> Vec<u8> { if m > 1 { format!("\x1b[{n};{m}~").into_bytes() } else { format!("\x1b[{n}~").into_bytes() } };
    let out: Vec<u8> = match k {
        key::RETURN | key::ENTER => {
            if alt {
                b"\x1b\r".to_vec()
            } else {
                b"\r".to_vec()
            }
        }
        key::BACKSPACE => {
            if ctrl {
                b"\x08".to_vec()
            } else if alt {
                b"\x1b\x7f".to_vec()
            } else {
                b"\x7f".to_vec()
            }
        }
        key::TAB => b"\t".to_vec(),
        key::BACKTAB => b"\x1b[Z".to_vec(),
        key::ESCAPE => b"\x1b".to_vec(),
        key::UP => csi("A", true),
        key::DOWN => csi("B", true),
        key::RIGHT => csi("C", true),
        key::LEFT => csi("D", true),
        key::HOME => csi("H", true),
        key::END => csi("F", true),
        key::INSERT => tilde(2),
        key::DELETE => tilde(3),
        key::PAGE_UP => tilde(5),
        key::PAGE_DOWN => tilde(6),
        k if (key::F1..key::F1 + 4).contains(&k) => {
            let fin = ["P", "Q", "R", "S"][(k - key::F1) as usize];
            if m > 1 { csi(fin, false) } else { format!("\x1bO{fin}").into_bytes() }
        }
        k if (key::F1 + 4..=key::F12).contains(&k) => tilde([15, 17, 18, 19, 20, 21, 23, 24][(k - key::F1 - 4) as usize]),
        _ => Vec::new(),
    };
    if !out.is_empty() {
        return out;
    }
    if ctrl {
        let c: Option<u8> = match k {
            0x41..=0x5a => Some((k - 0x41 + 1) as u8),
            0x20 | 0x40 | 0x32 => Some(0),
            0x5b | 0x33 => Some(0x1b),
            0x5c | 0x34 => Some(0x1c),
            0x5d | 0x35 => Some(0x1d),
            0x5e | 0x36 => Some(0x1e),
            0x5f | 0x2f | 0x37 => Some(0x1f),
            _ => None,
        };
        if let Some(c) = c {
            return if alt { vec![0x1b, c] } else { vec![c] };
        }
    }
    let mut out = text.as_bytes().to_vec();
    if alt && !out.is_empty() {
        out.insert(0, 0x1b);
    }
    out
}

fn bracketed(text: &str, mode: u32) -> Vec<u8> {
    let text = text.replace("\r\n", "\r").replace('\n', "\r");
    if mode & mode::BRACKETED_PASTE != 0 {
        format!("\x1b[200~{}\x1b[201~", text.replace("\x1b[201~", "")).into_bytes()
    } else {
        text.into_bytes()
    }
}

// ---------------------------------------------------------------- Qt entry points

impl qobject::TerminalView {
    unsafe fn paint(mut self: Pin<&mut Self>, painter: *mut cxx_qt_lib::QPainter) {
        let t0 = Instant::now();
        // SAFETY: Qt hands a live painter for the duration of the call
        let mut p = unsafe { Pin::new_unchecked(&mut *painter) };
        let (w, h) = (self.width(), self.height());
        let mut clip = p.clip_bounding_rect().unwrap_or_else(|| QRectF::new(0.0, 0.0, w, h));
        // A new image (tab shown again, resize) starts uninitialised: all of it is painted
        if self.full || clip.width() <= 0.0 {
            p.as_mut().set_clipping(false);
            clip = QRectF::new(0.0, 0.0, w, h);
        }
        self.as_mut().rust_mut().full = false;
        let bg = rgb(self.pal.bg);
        p.as_mut().fill_rect(&clip, &bg);
        let (pad, ch) = (self.padding as f64, self.ch as f64);
        let first = (((clip.y() - pad) / ch).floor() as i32).max(0);
        let last = (((clip.y() + clip.height() - pad) / ch).floor() as i32).min(self.rows as i32 - 1);
        for line in first..=last {
            self.as_mut().paint_line(&mut p, line);
        }
        if self.need_ack {
            self.as_mut().rust_mut().need_ack = false;
            self.send(proto::ACK, &[]);
        }
        if self.trace {
            let mut s = format!("paint lines {first}-{last} in {:.2} ms", t0.elapsed().as_secs_f64() * 1e3);
            if let Some(k) = self.as_mut().rust_mut().key_at.take() {
                s.push_str(&format!(", key→paint {:.2} ms", k.elapsed().as_secs_f64() * 1e3));
            }
            eprintln!("{s}");
        }
    }

    fn component_complete(mut self: Pin<&mut Self>) {
        self.as_mut().component_complete_super();
        self.as_mut().rust_mut().complete = true;
        if self.width() > 0.0 && self.height() > 0.0 {
            self.connect_daemon();
        }
    }

    fn geometry_change(mut self: Pin<&mut Self>, new_geometry: &QRectF, old_geometry: &QRectF) {
        self.as_mut().geometry_change_super(new_geometry, old_geometry);
        let same = new_geometry.width() == old_geometry.width() && new_geometry.height() == old_geometry.height();
        if same || new_geometry.width() <= 0.0 || new_geometry.height() <= 0.0 {
            return;
        }
        self.as_mut().mark_all();
        if self.as_mut().compute_grid() {
            self.as_mut().send_size();
        }
        if self.complete {
            self.connect_daemon();
        }
    }

    unsafe fn key_press_event(mut self: Pin<&mut Self>, event: *mut qobject::QKeyEvent) {
        // SAFETY: Qt hands a live event for the duration of the call
        let mut e = unsafe { Pin::new_unchecked(&mut *event) };
        if self.trace && self.key_at.is_none() {
            self.as_mut().rust_mut().key_at = Some(Instant::now());
        }
        let k = e.key();
        let mods = mods_of(e.modifiers());
        let shift_only = mods & (key::SHIFT | key::CONTROL | key::ALT) == key::SHIFT;
        // Scrolling the history stays in the window
        if shift_only && self.mode & mode::ALT_SCREEN == 0 && (k == key::PAGE_UP || k == key::PAGE_DOWN) {
            self.scroll_page(if k == key::PAGE_UP { -1 } else { 1 });
            return;
        }
        if mods & key::SHIFT != 0 && k == key::INSERT {
            self.paste();
            return;
        }
        let out = key_bytes(k, mods, &e.text().to_string(), self.mode & mode::APP_CURSOR != 0);
        if out.is_empty() {
            e.as_mut().ignore();
            return;
        }
        self.send_input(&out);
        e.accept();
    }

    unsafe fn input_method_event(mut self: Pin<&mut Self>, event: *mut qobject::QInputMethodEvent) {
        // SAFETY: as above
        let mut e = unsafe { Pin::new_unchecked(&mut *event) };
        let commit = e.commit_string().to_string();
        let preedit = e.preedit_string().to_string();
        if !commit.is_empty() {
            self.as_mut().send_input(commit.as_bytes());
        }
        if self.preedit != preedit {
            self.as_mut().rust_mut().preedit = preedit;
            let line = self.cur_line;
            self.as_mut().mark_line(line);
        }
        e.as_mut().accept();
    }

    fn input_method_query(&self, query: InputMethodQuery) -> QVariant {
        match query {
            InputMethodQuery::ImEnabled => QVariant::from(&true),
            InputMethodQuery::ImCursorRectangle => QVariant::from(&QRectF::new(
                (self.padding + self.cur_col * self.cw) as f64,
                (self.padding + self.cur_line.max(0) * self.ch) as f64,
                self.cw as f64,
                self.ch as f64,
            )),
            // Qt::ImhNoPredictiveText | Qt::ImhNoAutoUppercase
            InputMethodQuery::ImHints => QVariant::from(&(0x40i32 | 0x2)),
            _ => self.input_method_query_super(query),
        }
    }

    unsafe fn focus_in_event(mut self: Pin<&mut Self>, _event: *mut qobject::QFocusEvent) {
        self.as_mut().rust_mut().focused = true;
        if self.mode & mode::FOCUS_IN_OUT != 0 {
            self.send(proto::INPUT, b"\x1b[I");
        }
        let line = self.cur_line;
        self.mark_line(line);
    }

    unsafe fn focus_out_event(mut self: Pin<&mut Self>, _event: *mut qobject::QFocusEvent) {
        self.as_mut().rust_mut().focused = false;
        if self.mode & mode::FOCUS_IN_OUT != 0 {
            self.send(proto::INPUT, b"\x1b[O");
        }
        let line = self.cur_line;
        self.mark_line(line);
    }

    unsafe fn mouse_press_event(mut self: Pin<&mut Self>, event: *mut qobject::QMouseEvent) {
        // SAFETY: as above
        let e = unsafe { &*event };
        self.as_mut().force_active_focus();
        let (pos, mods, button) = (e.position(), mods_of(e.modifiers()), e.button());
        if self.as_mut().report_mouse(mods, &pos, button_code(button), true, false) {
            return;
        }
        if button == MouseButton::LeftButton {
            let c = self.cell_at(&pos);
            let had = self.has_selection;
            {
                let mut r = self.as_mut().rust_mut();
                r.sel_a = c;
                r.sel_b = c;
                r.selecting = true;
                r.has_selection = false;
            }
            if had {
                self.mark_all();
            }
        } else if button == MouseButton::MiddleButton {
            let text = clipboard_text(ClipboardMode::Selection);
            if !text.is_empty() {
                let b = bracketed(&text, self.mode);
                self.send_input(&b);
            }
        }
    }

    unsafe fn mouse_move_event(mut self: Pin<&mut Self>, event: *mut qobject::QMouseEvent) {
        // SAFETY: as above
        let e = unsafe { &*event };
        let buttons = e.buttons();
        let button = if buttons.test_flag(MouseButton::LeftButton) {
            0
        } else if buttons.test_flag(MouseButton::MiddleButton) {
            1
        } else if buttons.test_flag(MouseButton::RightButton) {
            2
        } else {
            3
        };
        let pos = e.position();
        if self.as_mut().report_mouse(mods_of(e.modifiers()), &pos, button, true, true) {
            return;
        }
        if !self.selecting {
            return;
        }
        let c = self.cell_at(&pos);
        if c == self.sel_b {
            return;
        }
        {
            let mut r = self.as_mut().rust_mut();
            r.sel_b = c;
            r.has_selection = true;
        }
        self.mark_all();
    }

    unsafe fn mouse_release_event(mut self: Pin<&mut Self>, event: *mut qobject::QMouseEvent) {
        // SAFETY: as above
        let e = unsafe { &*event };
        if self.as_mut().report_mouse(mods_of(e.modifiers()), &e.position(), button_code(e.button()), false, false) {
            return;
        }
        if self.selecting && self.has_selection {
            publish_selection(&self.selection_text());
        }
        self.as_mut().rust_mut().selecting = false;
    }

    unsafe fn mouse_double_click_event(mut self: Pin<&mut Self>, event: *mut qobject::QMouseEvent) {
        // SAFETY: as above
        let e = unsafe { &*event };
        if self.as_mut().report_mouse(mods_of(e.modifiers()), &e.position(), button_code(e.button()), true, false) {
            return;
        }
        if e.button() != MouseButton::LeftButton || self.cells.is_empty() {
            return;
        }
        // Word under the pointer: letters, digits and the characters of paths and URLs
        let c = self.cell_at(&e.position());
        let base = c.line as usize * self.cols;
        let is_word = |col: i32| {
            let ch = self.cells[base + col as usize].c;
            ch > ' ' && !"\"'`()[]{}<>|;,│".contains(ch)
        };
        if !is_word(c.col) {
            return;
        }
        let (mut a, mut b) = (c.col, c.col);
        while a > 0 && is_word(a - 1) {
            a -= 1;
        }
        while b < self.cols as i32 - 1 && is_word(b + 1) {
            b += 1;
        }
        {
            let mut r = self.as_mut().rust_mut();
            r.sel_a = Pos { col: a, line: c.line };
            r.sel_b = Pos { col: b, line: c.line };
            r.has_selection = true;
            r.selecting = false;
        }
        publish_selection(&self.selection_text());
        self.mark_all();
    }

    unsafe fn wheel_event(self: Pin<&mut Self>, event: *mut qobject::QWheelEvent) {
        // SAFETY: as above
        let e = unsafe { &*event };
        let mut steps = e.angle_delta().y() / 40; // 3 lines per notch
        if steps == 0 {
            steps = e.pixel_delta().y() / self.ch.max(1);
        }
        if steps == 0 {
            return;
        }
        let m = self.mode;
        if m & mode::MOUSE_MODE != 0 && mods_of(e.modifiers()) & key::SHIFT == 0 {
            let c = self.cell_at(&e.position());
            let b = if steps > 0 { 64 } else { 65 };
            for _ in 0..steps.abs() {
                self.send(proto::INPUT, &sgr_or_x10(m, b, c.col, c.line, true));
            }
            return;
        }
        if m & mode::ALT_SCREEN != 0 && m & mode::ALTERNATE_SCROLL != 0 {
            let app = m & mode::APP_CURSOR != 0;
            let k: &[u8] = match (steps > 0, app) {
                (true, true) => b"\x1bOA",
                (false, true) => b"\x1bOB",
                (true, false) => b"\x1b[A",
                (false, false) => b"\x1b[B",
            };
            for _ in 0..steps.abs() {
                self.send(proto::INPUT, k);
            }
            return;
        }
        let mut b = vec![0u8];
        proto::put_u32(&mut b, steps as u32);
        self.send(proto::SCROLL, &b);
    }

    // ------------------------------------------------------------ invokables

    fn copy(self: Pin<&mut Self>) {
        if self.has_selection {
            set_clipboard_text(&self.selection_text(), ClipboardMode::Clipboard);
        }
    }

    fn paste(self: Pin<&mut Self>) {
        let text = clipboard_text(ClipboardMode::Clipboard);
        if !text.is_empty() {
            let b = bracketed(&text, self.mode);
            self.send_input(&b);
        }
    }

    fn scroll_page(self: Pin<&mut Self>, direction: i32) {
        let kind = match direction {
            ..=-2 => 3u8,
            -1 => 1,
            1 => 2,
            _ => 4,
        };
        let mut b = vec![kind];
        proto::put_u32(&mut b, 0);
        self.send(proto::SCROLL, &b);
    }

    fn close_session(self: Pin<&mut Self>) {
        self.send(proto::CLOSE, &[]);
    }

    fn set_shown(mut self: Pin<&mut Self>, shown: bool) {
        if shown {
            // Full-size image back, painted whole, which also acknowledges the frame the daemon waits on
            self.as_mut().set_texture_size(&QSize::default());
            self.mark_all();
        } else {
            self.set_texture_size(&QSize::new(1, 1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Pal;

    #[test]
    fn colours_of_the_theme() {
        let mut text = String::from("#dbe7f4 #1c222b #3c4858 #fff");
        for _ in 0..16 {
            text.push_str(" #ff102030");
        }
        let pal = Pal::parse(&text).unwrap();
        assert_eq!((pal.fg, pal.bg, pal.selection, pal.cursor), (0xdbe7f4, 0x1c222b, 0x3c4858, 0xffffff));
        assert_eq!(pal.ansi[15], 0x102030);
        // Missing or invalid colours keep the palette as it is
        assert!(Pal::parse("#dbe7f4 #1c222b").is_none());
        assert!(Pal::parse(&text.replace("#fff", "white")).is_none());
    }
}

use std::fmt::Write;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags, Hyperlink};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor};

#[derive(Clone, PartialEq)]
struct Pen {
    fg: Color,
    bg: Color,
    flags: Flags,
    underline: Option<Color>,
}

const STYLE: Flags = Flags::BOLD
    .union(Flags::DIM)
    .union(Flags::ITALIC)
    .union(Flags::ALL_UNDERLINES)
    .union(Flags::INVERSE)
    .union(Flags::HIDDEN)
    .union(Flags::STRIKEOUT);

impl Pen {
    fn plain() -> Pen {
        Pen {
            fg: Color::Named(NamedColor::Foreground),
            bg: Color::Named(NamedColor::Background),
            flags: Flags::empty(),
            underline: None,
        }
    }

    fn of(cell: &Cell) -> Pen {
        Pen { fg: cell.fg, bg: cell.bg, flags: cell.flags & STYLE, underline: cell.underline_color() }
    }

    fn sgr(&self, out: &mut String) {
        out.push_str("\x1b[0");
        let flags = [
            (Flags::BOLD, "1"),
            (Flags::DIM, "2"),
            (Flags::ITALIC, "3"),
            (Flags::UNDERLINE, "4"),
            (Flags::DOUBLE_UNDERLINE, "4:2"),
            (Flags::UNDERCURL, "4:3"),
            (Flags::DOTTED_UNDERLINE, "4:4"),
            (Flags::DASHED_UNDERLINE, "4:5"),
            (Flags::INVERSE, "7"),
            (Flags::HIDDEN, "8"),
            (Flags::STRIKEOUT, "9"),
        ];
        for (flag, code) in flags {
            if self.flags.contains(flag) {
                let _ = write!(out, ";{code}");
            }
        }
        color(out, self.fg, 30, 90, "38", 39);
        color(out, self.bg, 40, 100, "48", 49);
        if let Some(underline) = self.underline {
            color(out, underline, 0, 0, "58", 59);
        }
        out.push('m');
    }
}

fn color(out: &mut String, color: Color, base: u8, bright: u8, extended: &str, default: u8) {
    match color {
        Color::Spec(rgb) => {
            let _ = write!(out, ";{extended};2;{};{};{}", rgb.r, rgb.g, rgb.b);
        }
        Color::Indexed(index) => {
            let _ = write!(out, ";{extended};5;{index}");
        }
        Color::Named(named) => {
            let index = named as usize;
            if base > 0 && index < 8 {
                let _ = write!(out, ";{}", base as usize + index);
            } else if base > 0 && index < 16 {
                let _ = write!(out, ";{}", bright as usize + index - 8);
            } else if index < 16 {
                let _ = write!(out, ";{extended};5;{index}");
            } else if default != 39 && default != 49 {
                let _ = write!(out, ";{default}");
            }
        }
    }
}

fn link(out: &mut String, link: Option<&Hyperlink>) {
    match link {
        Some(link) => {
            let _ = write!(out, "\x1b]8;id={};{}\x1b\\", link.id(), link.uri());
        }
        None => out.push_str("\x1b]8;;\x1b\\"),
    }
}

fn significant(cell: &Cell) -> bool {
    cell.c != ' '
        || cell.bg != Color::Named(NamedColor::Background)
        || cell.flags.intersects(Flags::INVERSE | Flags::ALL_UNDERLINES | Flags::STRIKEOUT)
        || cell.hyperlink().is_some()
}

const MODES: [(TermMode, &str); 12] = [
    (TermMode::APP_CURSOR, "\x1b[?1h"),
    (TermMode::APP_KEYPAD, "\x1b="),
    (TermMode::MOUSE_REPORT_CLICK, "\x1b[?1000h"),
    (TermMode::MOUSE_DRAG, "\x1b[?1002h"),
    (TermMode::MOUSE_MOTION, "\x1b[?1003h"),
    (TermMode::FOCUS_IN_OUT, "\x1b[?1004h"),
    (TermMode::UTF8_MOUSE, "\x1b[?1005h"),
    (TermMode::SGR_MOUSE, "\x1b[?1006h"),
    (TermMode::BRACKETED_PASTE, "\x1b[?2004h"),
    (TermMode::INSERT, "\x1b[4h"),
    (TermMode::LINE_FEED_NEW_LINE, "\x1b[20h"),
    (TermMode::ORIGIN, "\x1b[?6h"),
];

const CLEARED: [(TermMode, &str); 4] = [
    (TermMode::SHOW_CURSOR, "\x1b[?25l"),
    (TermMode::LINE_WRAP, "\x1b[?7l"),
    (TermMode::ALTERNATE_SCROLL, "\x1b[?1007l"),
    (TermMode::URGENCY_HINTS, "\x1b[?1042l"),
];

const KITTY: [(TermMode, u8); 5] = [
    (TermMode::DISAMBIGUATE_ESC_CODES, 1),
    (TermMode::REPORT_EVENT_TYPES, 2),
    (TermMode::REPORT_ALTERNATE_KEYS, 4),
    (TermMode::REPORT_ALL_KEYS_AS_ESC, 8),
    (TermMode::REPORT_ASSOCIATED_TEXT, 16),
];

pub fn snapshot<T>(term: &Term<T>) -> Vec<u8> {
    let mode = *term.mode();
    let alternate = mode.contains(TermMode::ALT_SCREEN);
    let grid = term.grid();
    let (rows, cols) = (grid.screen_lines() as i32, grid.columns());
    let top = if alternate { 0 } else { -(grid.history_size() as i32) };
    let mut out = String::new();
    if alternate {
        out.push_str("\x1b[?1049h");
    }
    let mut pen = Pen::plain();
    let mut current_link: Option<Hyperlink> = None;
    let mut continued = false;
    for line in top..rows {
        let row = &grid[Line(line)];
        let wrapped = row[Column(cols - 1)].flags.contains(Flags::WRAPLINE);
        let end = if wrapped {
            cols
        } else {
            (0..cols).rev().find(|&col| significant(&row[Column(col)])).map_or(0, |col| col + 1)
        };
        let end = if continued && end == 0 { 1 } else { end };
        for col in 0..end {
            let cell = &row[Column(col)];
            if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                continue;
            }
            let cell_pen = Pen::of(cell);
            if cell_pen != pen {
                cell_pen.sgr(&mut out);
                pen = cell_pen;
            }
            let cell_link = cell.hyperlink();
            if cell_link != current_link {
                link(&mut out, cell_link.as_ref());
                current_link = cell_link;
            }
            out.push(if cell.c == '\0' { ' ' } else { cell.c });
            if let Some(extra) = cell.zerowidth() {
                out.extend(extra);
            }
        }
        continued = wrapped;
        if line < rows - 1 && !wrapped {
            if current_link.take().is_some() {
                link(&mut out, None);
            }
            if pen != Pen::plain() {
                out.push_str("\x1b[0m");
                pen = Pen::plain();
            }
            out.push_str("\r\n");
        }
    }
    if current_link.is_some() {
        link(&mut out, None);
    }
    for (flag, sequence) in MODES {
        if mode.contains(flag) {
            out.push_str(sequence);
        }
    }
    for (flag, sequence) in CLEARED {
        if !mode.contains(flag) {
            out.push_str(sequence);
        }
    }
    let kitty: u8 = KITTY.iter().filter(|(flag, _)| mode.contains(*flag)).map(|(_, bit)| bit).sum();
    if kitty > 0 {
        let _ = write!(out, "\x1b[>{kitty}u");
    }
    let cursor = &grid.cursor;
    let _ = write!(out, "\x1b[{};{}H", cursor.point.line.0 + 1, cursor.point.column.0 + 1);
    Pen::of(&cursor.template).sgr(&mut out);
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

    struct Size(usize, usize);

    impl Dimensions for Size {
        fn total_lines(&self) -> usize {
            self.1
        }
        fn screen_lines(&self) -> usize {
            self.1
        }
        fn columns(&self) -> usize {
            self.0
        }
    }

    fn term(cols: usize, rows: usize) -> Term<VoidListener> {
        let config = Config { scrolling_history: 1000, kitty_keyboard: true, ..Config::default() };
        Term::new(config, &Size(cols, rows), VoidListener)
    }

    fn feed(term: &mut Term<VoidListener>, bytes: &[u8]) {
        Processor::<StdSyncHandler>::new().advance(term, bytes);
    }

    fn assert_same(a: &Term<VoidListener>, b: &Term<VoidListener>) {
        let (ga, gb) = (a.grid(), b.grid());
        assert_eq!(ga.history_size(), gb.history_size(), "history");
        assert_eq!(ga.cursor.point, gb.cursor.point, "cursor");
        assert_eq!(*a.mode(), *b.mode(), "mode");
        let top = -(ga.history_size() as i32);
        for line in top..ga.screen_lines() as i32 {
            for col in 0..ga.columns() {
                let (ca, cb) = (&ga[Line(line)][Column(col)], &gb[Line(line)][Column(col)]);
                assert_eq!(ca.c, cb.c, "char at {line},{col}");
                assert_eq!(ca.fg, cb.fg, "fg at {line},{col}");
                assert_eq!(ca.bg, cb.bg, "bg at {line},{col}");
                assert_eq!(ca.flags, cb.flags, "flags at {line},{col}");
                assert_eq!(ca.hyperlink(), cb.hyperlink(), "link at {line},{col}");
                assert_eq!(ca.zerowidth(), cb.zerowidth(), "zerowidth at {line},{col}");
            }
        }
        let after = b"\x1b[1mnext\x1b[0m done\r\n";
        let (mut a, mut b) = (clone(a), clone(b));
        feed(&mut a, after);
        feed(&mut b, after);
        assert_eq!(a.grid().cursor.point, b.grid().cursor.point, "cursor after more output");
    }

    fn clone(term: &Term<VoidListener>) -> Term<VoidListener> {
        let mut copy = self::term(term.columns(), term.screen_lines());
        feed(&mut copy, &snapshot(term));
        copy
    }

    fn round_trip(cols: usize, rows: usize, bytes: &[u8]) {
        let mut source = term(cols, rows);
        feed(&mut source, bytes);
        let copy = clone(&source);
        assert_same(&source, &copy);
    }

    #[test]
    fn copies_plain_output_and_history() {
        let mut text = String::new();
        for index in 0..60 {
            text.push_str(&format!("line {index}\r\n"));
        }
        text.push_str("$ prompt");
        round_trip(20, 10, text.as_bytes());
    }

    #[test]
    fn copies_colors_styles_and_links() {
        round_trip(
            30,
            6,
            "\x1b[31mred\x1b[0m \x1b[1;4;38;5;123mbold\x1b[0m \x1b[48;2;10;20;30m bg \x1b[0m\r\n\
             \x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\ \x1b[7minv\x1b[27m \x1b[9mstrike\x1b[0m\r\n\
             wide 你好 e\u{301} \x1b[4:3;58;2;1;2;3mcurl\x1b[0m\r\n\x1b[92mbright\x1b[0m"
                .as_bytes(),
        );
    }

    #[test]
    fn copies_wrapped_lines_and_wide_chars_at_the_edge() {
        round_trip(10, 5, "abcdefghijklmnopqrstuvwxyz\r\n123456789你\r\n0123456789\r\nx".as_bytes());
    }

    #[test]
    fn copies_cursor_position_and_modes() {
        round_trip(40, 10, b"\x1b[?2004h\x1b[?1h\x1b[?1000h\x1b[?1006h\x1b[?25l\x1b[>1u\x1b[3;5Hmid\x1b[1;33m");
    }

    #[test]
    fn copies_the_alternate_screen() {
        round_trip(20, 6, b"shell output\r\n\x1b[?1049h\x1b[2J\x1b[Hfull screen\x1b[5;3Hhere");
    }

    #[test]
    fn copies_lines_cleared_in_the_middle() {
        round_trip(20, 6, b"one\r\ntwo\r\nthree\x1b[2;1H\x1b[2K\x1b[6;1Hlast");
    }
}

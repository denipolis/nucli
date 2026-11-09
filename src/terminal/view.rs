use std::ops::Range;
use std::rc::Rc;

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point as GridPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb};
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, ExternalPaths, Font, FontFeatures, FontStyle, FontWeight, GlobalElementId, Hsla,
    InspectorElementId, InteractiveElement, IntoElement, KeyDownEvent, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Render, Rgba, ScrollDelta, ScrollWheelEvent,
    ShapedLine, SharedString, Size, StrikethroughStyle, Style, Styled, TextRun, UTF16Selection, UnderlineStyle, Window,
    div, fill, outline, point, px, relative, size,
};

use crate::commands;
use crate::session::{Session, SessionEvent};
use crate::settings::{self, FONT};
use crate::terminal::find::Find;
use crate::terminal::links::{self, Link, Target};
use crate::terminal::{boxes, keys};
use crate::theme::Theme;
use crate::ui;

#[derive(Default)]
pub struct ViewState {
    pub layout: Option<(Point<Pixels>, Size<Pixels>)>,
    pub marked: Option<String>,
    pub selecting: bool,
    pub scroll: f32,
    pub theme: Option<Rc<Theme>>,
    pub mouse_button: Option<u8>,
    pub find: Option<Find>,
    pub hover: Option<Link>,
    pub cursor_hidden: bool,
    pub focused: bool,
    pub observing_focus: bool,
    pub link_clicked: bool,
}

pub fn color_table(theme: &Theme) -> Vec<Rgb> {
    let rgb = |color: Rgba| Rgb { r: (color.r * 255.0) as u8, g: (color.g * 255.0) as u8, b: (color.b * 255.0) as u8 };
    let mut table: Vec<Rgb> = theme.ansi.iter().map(|color| rgb(*color)).collect();
    let steps = [0u8, 95, 135, 175, 215, 255];
    for index in 0..216 {
        table.push(Rgb { r: steps[index / 36], g: steps[(index / 6) % 6], b: steps[index % 6] });
    }
    for index in 0..24 {
        let level = 8 + index as u8 * 10;
        table.push(Rgb { r: level, g: level, b: level });
    }
    table.push(rgb(theme.foreground_rgb));
    table.push(rgb(theme.background_rgb));
    table.push(rgb(theme.accent.to_rgb()));
    table
}

fn to_hsla(color: Rgb) -> Hsla {
    Rgba { r: color.r as f32 / 255.0, g: color.g as f32 / 255.0, b: color.b as f32 / 255.0, a: 1.0 }.into()
}

fn resolve(color: Color, overrides: &alacritty_terminal::term::color::Colors, table: &[Rgb]) -> Rgb {
    let lookup = |index: usize| overrides[index].unwrap_or(table[index.min(table.len() - 1)]);
    match color {
        Color::Spec(rgb) => rgb,
        Color::Indexed(index) => lookup(index as usize),
        Color::Named(named) => {
            let index = named as usize;
            if index <= NamedColor::Cursor as usize {
                lookup(index)
            } else if index == NamedColor::BrightForeground as usize {
                lookup(NamedColor::Foreground as usize)
            } else if index == NamedColor::DimForeground as usize {
                dim(lookup(NamedColor::Foreground as usize), lookup(NamedColor::Background as usize))
            } else {
                let base = index - NamedColor::DimBlack as usize;
                dim(lookup(base), lookup(NamedColor::Background as usize))
            }
        }
    }
}

fn dim(color: Rgb, background: Rgb) -> Rgb {
    let mix = |a: u8, b: u8| (a as f32 * 0.66 + b as f32 * 0.34) as u8;
    Rgb { r: mix(color.r, background.r), g: mix(color.g, background.g), b: mix(color.b, background.b) }
}

pub fn font(family: &str, bold: bool, italic: bool, ligatures: bool) -> Font {
    Font {
        family: family.to_string().into(),
        features: if ligatures { FontFeatures::default() } else { FontFeatures::disable_ligatures() },
        fallbacks: (family != FONT).then(|| gpui::FontFallbacks::from_fonts(vec![FONT.into()])),
        weight: if bold { FontWeight::BOLD } else { FontWeight::NORMAL },
        style: if italic { FontStyle::Italic } else { FontStyle::Normal },
    }
}

pub fn cell_size(window: &Window, cx: &App) -> (Pixels, Size<Pixels>) {
    let settings = settings::get(cx);
    let font_size = px(settings.font_size());
    let font_id = window.text_system().resolve_font(&font(&settings.font, false, false, true));
    let width = window.text_system().advance(font_id, font_size, 'm').map(|size| size.width).unwrap_or(font_size * 0.6);
    let height = (font_size * settings.line_height()).round();
    (font_size, size(width, height))
}

struct Run {
    column: usize,
    text: String,
    color: Hsla,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
}

pub struct Frame {
    rects: Vec<(Bounds<Pixels>, Hsla)>,
    lines: Vec<(Point<Pixels>, ShapedLine)>,
    cursor: Vec<gpui::PaintQuad>,
    cell: Size<Pixels>,
}

pub struct TerminalElement {
    session: Entity<Session>,
    focused: bool,
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = Frame;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Frame {
        let (font_size, cell) = cell_size(window, cx);
        let cols = (f32::from(bounds.size.width) / f32::from(cell.width)).floor().max(2.0) as usize;
        let rows = (f32::from(bounds.size.height) / f32::from(cell.height)).floor().max(1.0) as usize;
        let grid =
            super::GridSize { cols, rows, cell_width: f32::from(cell.width), cell_height: f32::from(cell.height) };
        let (family, ligatures) = (settings::get(cx).font.clone(), settings::get(cx).ligatures);
        let global_theme = ui::theme(cx);
        let lift = self.session.update(cx, |session, _| {
            session.terminal.resize(grid);
            cell.height * session.terminal.prompt_lift() as f32
        });
        let bounds = Bounds::new(point(bounds.origin.x, bounds.origin.y + lift), bounds.size);
        let (theme, marked, term, hover, cursor_hidden, found, current) = self.session.update(cx, |session, _| {
            session.view.layout = Some((bounds.origin, cell));
            let (found, current) = match &session.view.find {
                Some(find) => (find.matches.clone(), find.current),
                None => (Vec::new(), 0),
            };
            (
                session.view.theme.clone().unwrap_or(global_theme),
                session.view.marked.clone(),
                session.terminal.term.clone(),
                session.view.hover.clone(),
                session.view.cursor_hidden,
                found,
                current,
            )
        });
        let table = color_table(&theme);
        let background = Rgb {
            r: (theme.background_rgb.r * 255.0) as u8,
            g: (theme.background_rgb.g * 255.0) as u8,
            b: (theme.background_rgb.b * 255.0) as u8,
        };
        let mut frame = Frame { rects: Vec::new(), lines: Vec::new(), cursor: Vec::new(), cell };
        let mut rows_runs: Vec<Vec<Run>> = (0..rows).map(|_| Vec::new()).collect();
        let term = term.lock();
        let content = term.renderable_content();
        let offset = content.display_offset as i32;
        let selection = content.selection;
        let cursor = content.cursor;
        let colors = content.colors;
        let top_line = -offset;
        let bottom_line = rows as i32 - 1 - offset;
        let visible: Vec<(GridPoint, GridPoint, bool)> = found
            .iter()
            .enumerate()
            .filter(|(_, (start, end))| end.line.0 >= top_line && start.line.0 <= bottom_line)
            .map(|(index, (start, end))| (*start, *end, index == current))
            .collect();
        for indexed in content.display_iter {
            let row = indexed.point.line.0 + offset;
            if row < 0 || row as usize >= rows {
                continue;
            }
            let cell_data = indexed.cell;
            if cell_data.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                continue;
            }
            let column = indexed.point.column.0;
            let mut fg = resolve(cell_data.fg, colors, &table);
            let mut bg = resolve(cell_data.bg, colors, &table);
            if cell_data.flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            if cell_data.flags.contains(Flags::DIM) {
                fg = dim(fg, bg);
            }
            let selected = selection.is_some_and(|range| range.contains(indexed.point));
            let width = if cell_data.flags.contains(Flags::WIDE_CHAR) { 2.0 } else { 1.0 };
            let origin =
                point(bounds.origin.x + cell.width * column as f32, bounds.origin.y + cell.height * row as f32);
            let cell_bounds = Bounds::new(origin, size(cell.width * width, cell.height));
            let matched = visible
                .iter()
                .find(|(start, end, _)| *start <= indexed.point && indexed.point <= *end)
                .map(|(_, _, active)| *active);
            let hovered = hover
                .as_ref()
                .is_some_and(|link| link.line == indexed.point.line.0 && (link.start..=link.end).contains(&column));
            if let Some(active) = matched {
                frame.rects.push((cell_bounds, if active { theme.orange } else { theme.yellow }));
                fg = Rgb { r: 0, g: 0, b: 0 };
            } else if selected {
                frame.rects.push((cell_bounds, theme.selection));
                fg = table[NamedColor::Foreground as usize];
            } else if bg != background {
                frame.rects.push((cell_bounds, to_hsla(bg)));
            }
            let character = cell_data.c;
            if character == ' ' || character == '\0' || cell_data.flags.contains(Flags::HIDDEN) {
                continue;
            }
            if boxes::is_drawn(character) {
                frame.rects.extend(boxes::rects(character, origin, cell, to_hsla(fg)));
                continue;
            }
            let mut text = String::new();
            text.push(character);
            if let Some(extra) = cell_data.zerowidth() {
                text.extend(extra);
            }
            let run = Run {
                column,
                text,
                color: to_hsla(fg),
                bold: cell_data.flags.contains(Flags::BOLD),
                italic: cell_data.flags.contains(Flags::ITALIC),
                underline: cell_data.flags.intersects(Flags::ALL_UNDERLINES) || hovered,
                strike: cell_data.flags.contains(Flags::STRIKEOUT),
            };
            let runs = &mut rows_runs[row as usize];
            match runs.last_mut() {
                Some(last)
                    if character.is_ascii()
                        && last.text.is_ascii()
                        && last.column + last.text.len() == column
                        && last.color == run.color
                        && last.bold == run.bold
                        && last.italic == run.italic
                        && last.underline == run.underline
                        && last.strike == run.strike =>
                {
                    last.text.push(character);
                }
                _ => runs.push(run),
            }
        }
        let cursor_row = cursor.point.line.0 + offset;
        let show_cursor = cursor.shape != CursorShape::Hidden && cursor_row >= 0 && (cursor_row as usize) < rows;
        drop(term);
        if show_cursor {
            let origin = point(
                bounds.origin.x + cell.width * cursor.point.column.0 as f32,
                bounds.origin.y + cell.height * cursor_row as f32,
            );
            let color = theme.accent;
            let block = Bounds::new(origin, cell);
            if !self.focused {
                frame.cursor.push(outline(block, color, gpui::BorderStyle::Solid));
            } else if !cursor_hidden {
                match cursor.shape {
                    CursorShape::Beam => {
                        frame.cursor.push(fill(Bounds::new(origin, size(px(2.0), cell.height)), color))
                    }
                    CursorShape::Underline => frame.cursor.push(fill(
                        Bounds::new(point(origin.x, origin.y + cell.height - px(2.0)), size(cell.width, px(2.0))),
                        color,
                    )),
                    _ => {
                        frame.rects.push((block, color));
                        if let Some(run) = rows_runs[cursor_row as usize].iter_mut().find(|run| {
                            run.column <= cursor.point.column.0
                                && cursor.point.column.0 < run.column + run.text.chars().count()
                        }) {
                            let index = cursor.point.column.0 - run.column;
                            if run.text.is_ascii() && run.text.len() > 1 {
                                let split: Vec<Run> = split_run(run, index, theme.on_accent);
                                let position = rows_runs[cursor_row as usize]
                                    .iter()
                                    .position(|candidate| candidate.column == split[0].column)
                                    .unwrap_or(0);
                                rows_runs[cursor_row as usize].splice(position..=position, split);
                            } else {
                                run.color = theme.on_accent;
                            }
                        }
                    }
                }
            }
            if let Some(text) = marked.filter(|text| !text.is_empty()) {
                let run = TextRun {
                    len: text.len(),
                    font: font(&family, false, false, ligatures),
                    color: theme.text,
                    background_color: Some(theme.elevated),
                    underline: Some(UnderlineStyle { thickness: px(1.0), color: Some(theme.accent), wavy: false }),
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(text.into(), font_size, &[run], None);
                frame.lines.push((origin, shaped));
            }
        }
        for (row, runs) in rows_runs.into_iter().enumerate() {
            for run in runs {
                let origin =
                    point(bounds.origin.x + cell.width * run.column as f32, bounds.origin.y + cell.height * row as f32);
                let text_run = TextRun {
                    len: run.text.len(),
                    font: font(&family, run.bold, run.italic, ligatures),
                    color: run.color,
                    background_color: None,
                    underline: run.underline.then_some(UnderlineStyle { thickness: px(1.0), color: None, wavy: false }),
                    strikethrough: run.strike.then_some(StrikethroughStyle { thickness: px(1.0), color: None }),
                };
                let shaped =
                    window.text_system().shape_line(SharedString::from(run.text), font_size, &[text_run], None);
                frame.lines.push((origin, shaped));
            }
        }
        frame
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _layout: &mut (),
        frame: &mut Frame,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.session.read(cx).focus.clone();
        window.handle_input(&focus, ElementInputHandler::new(bounds, self.session.clone()), cx);
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            for (rect, color) in frame.rects.drain(..) {
                window.paint_quad(fill(rect, color));
            }
            for quad in frame.cursor.drain(..) {
                window.paint_quad(quad);
            }
            for (origin, line) in frame.lines.drain(..) {
                let _ = line.paint(origin, frame.cell.height, window, cx);
            }
        });
    }
}

fn split_run(run: &Run, index: usize, cursor_color: Hsla) -> Vec<Run> {
    let mut parts = Vec::new();
    let text = run.text.as_str();
    let make = |column: usize, text: &str, color: Hsla| Run {
        column,
        text: text.to_string(),
        color,
        bold: run.bold,
        italic: run.italic,
        underline: run.underline,
        strike: run.strike,
    };
    if index > 0 {
        parts.push(make(run.column, &text[..index], run.color));
    }
    parts.push(make(run.column + index, &text[index..index + 1], cursor_color));
    if index + 1 < text.len() {
        parts.push(make(run.column + index + 1, &text[index + 1..], run.color));
    }
    parts
}

fn grid_point(session: &Session, position: Point<Pixels>) -> Option<(GridPoint, Side)> {
    let (origin, cell) = session.view.layout?;
    let size = session.terminal.size;
    let x = f32::from(position.x - origin.x) / f32::from(cell.width);
    let y = f32::from(position.y - origin.y) / f32::from(cell.height);
    let column = (x.max(0.0) as usize).min(size.cols - 1);
    let row = (y.max(0.0) as usize).min(size.rows - 1);
    let offset = session.terminal.term.lock().grid().display_offset() as i32;
    let side = if x.fract() < 0.5 { Side::Left } else { Side::Right };
    Some((GridPoint::new(Line(row as i32 - offset), Column(column)), side))
}

fn mouse_report(
    session: &Session,
    button: u8,
    position: Point<Pixels>,
    pressed: bool,
    mods: &gpui::Modifiers,
) -> Option<Vec<u8>> {
    let (column, row) = session.cell_at(position)?;
    let size = session.terminal.size;
    let column = column.min(size.cols - 1) + 1;
    let row = row.min(size.rows - 1) + 1;
    let code =
        button + if mods.shift { 4 } else { 0 } + if mods.alt { 8 } else { 0 } + if mods.control { 16 } else { 0 };
    let mode = session.terminal.mode();
    if mode.contains(TermMode::SGR_MOUSE) {
        Some(format!("\x1b[<{code};{column};{row}{}", if pressed { 'M' } else { 'm' }).into_bytes())
    } else {
        let code = if pressed { code } else { 3 + (code & !3) };
        if column > 223 || row > 223 {
            return None;
        }
        Some(vec![0x1b, b'[', b'M', 32 + code, 32 + column as u8, 32 + row as u8])
    }
}

fn reports_mouse(session: &Session, mods: &gpui::Modifiers) -> bool {
    !mods.shift && session.terminal.mode().intersects(TermMode::MOUSE_MODE)
}

fn button_code(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Middle => Some(1),
        MouseButton::Right => Some(2),
        _ => None,
    }
}

impl Session {
    pub fn input(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if self.exit.is_some() {
            return;
        }
        {
            let mut term = self.terminal.term.lock();
            term.scroll_display(Scroll::Bottom);
            term.selection = None;
        }
        self.view.cursor_hidden = false;
        self.terminal.write(bytes);
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if keys::CLIPBOARD_KEYS {
            if keys::plain_control(&event.keystroke, "c")
                && let Some(text) = self.terminal.selection_text()
            {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.terminal.term.lock().selection = None;
                cx.stop_propagation();
                cx.notify();
                return;
            }
            if keys::plain_control(&event.keystroke, "v") {
                cx.stop_propagation();
                self.paste_clipboard(cx);
                return;
            }
        }
        let mode = self.terminal.mode();
        if let Some(bytes) = keys::encode(&event.keystroke, mode, true) {
            cx.stop_propagation();
            self.input(&bytes, cx);
        }
    }

    fn copy(&mut self, _: &commands::Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.terminal.selection_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn paste(&mut self, _: &commands::Paste, _window: &mut Window, cx: &mut Context<Self>) {
        self.paste_clipboard(cx);
    }

    fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let bytes = keys::paste(&text, self.terminal.mode());
            self.input(&bytes, cx);
        }
    }

    fn select_all(&mut self, _: &commands::SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        let mut term = self.terminal.term.lock();
        let top = -(term.grid().history_size() as i32);
        let bottom = term.grid().screen_lines() as i32 - 1;
        let last = term.grid().columns() - 1;
        let mut selection = Selection::new(SelectionType::Simple, GridPoint::new(Line(top), Column(0)), Side::Left);
        selection.update(GridPoint::new(Line(bottom), Column(last)), Side::Right);
        term.selection = Some(selection);
        drop(term);
        cx.notify();
    }

    fn clear(&mut self, _: &commands::Clear, _window: &mut Window, cx: &mut Context<Self>) {
        self.terminal.clear();
        cx.notify();
    }

    fn link_at(&self, position: Point<Pixels>) -> Option<Link> {
        let (point, _) = grid_point(self, position)?;
        let term = self.terminal.term.lock();
        let grid = term.grid();
        let row = &grid[point.line];
        if let Some(link) = row[point.column].hyperlink() {
            let same = |column: usize| row[Column(column)].hyperlink().is_some_and(|other| other.id() == link.id());
            let mut start = point.column.0;
            while start > 0 && same(start - 1) {
                start -= 1;
            }
            let mut end = point.column.0;
            while end + 1 < grid.columns() && same(end + 1) {
                end += 1;
            }
            let uri = link.uri().to_string();
            let target = match uri.strip_prefix("file://") {
                Some(path) => links::classify(path, self.working_dir(), &self.project),
                None => Some(Target::Url(uri)),
            }?;
            return Some(Link { line: point.line.0, start, end, target });
        }
        let columns = grid.columns();
        let wraps = |line: i32| grid[Line(line)][Column(columns - 1)].flags.contains(Flags::WRAPLINE);
        let top = -(grid.history_size() as i32);
        let bottom = grid.screen_lines() as i32 - 1;
        let mut first = point.line.0;
        while first > top && wraps(first - 1) {
            first -= 1;
        }
        let mut last = point.line.0;
        while last < bottom && wraps(last) {
            last += 1;
        }
        let text: Vec<char> = (first..=last)
            .flat_map(|line| (0..columns).map(move |column| (line, column)))
            .map(|(line, column)| grid[Line(line)][Column(column)].c)
            .collect();
        let offset = (point.line.0 - first) as usize * columns;
        let (start, end, token) = links::token_at(&text, offset + point.column.0)?;
        drop(term);
        let target = links::classify(&token, self.working_dir(), &self.project)?;
        let (row_start, row_end) = (offset, offset + columns - 1);
        Some(Link {
            line: point.line.0,
            start: start.max(row_start) - row_start,
            end: end.min(row_end) - row_start,
            target,
        })
    }

    fn update_hover(&mut self, position: Point<Pixels>, modifiers: &gpui::Modifiers, cx: &mut Context<Self>) {
        let hover = if modifiers.secondary() { self.link_at(position) } else { None };
        if hover != self.view.hover {
            self.view.hover = hover;
            cx.notify();
        }
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        if event.button == MouseButton::Left
            && event.modifiers.secondary()
            && let Some(link) = self.link_at(event.position)
        {
            self.view.link_clicked = true;
            match link.target {
                Target::Url(url) => cx.open_url(&url),
                Target::File(path, line) => cx.emit(SessionEvent::OpenPath(path, line)),
            }
            return;
        }
        if reports_mouse(self, &event.modifiers) {
            if let Some(code) = button_code(event.button)
                && let Some(bytes) = mouse_report(self, code, event.position, true, &event.modifiers)
            {
                self.view.mouse_button = Some(code);
                self.terminal.write(&bytes);
            }
            return;
        }
        if event.button != MouseButton::Left {
            return;
        }
        let Some((point, side)) = grid_point(self, event.position) else { return };
        let kind = match event.click_count {
            2 => SelectionType::Semantic,
            3.. => SelectionType::Lines,
            _ => SelectionType::Simple,
        };
        let mut term = self.terminal.term.lock();
        if event.modifiers.shift
            && let Some(selection) = term.selection.as_mut()
        {
            selection.update(point, side);
        } else {
            term.selection = (kind != SelectionType::Simple).then(|| Selection::new(kind, point, side));
            if kind == SelectionType::Simple {
                term.selection = Some(Selection::new(kind, point, side));
            }
        }
        drop(term);
        self.view.selecting = true;
        cx.notify();
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.update_hover(event.position, &event.modifiers, cx);
        if let Some(code) = self.view.mouse_button {
            if self.terminal.mode().intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION)
                && let Some(bytes) = mouse_report(self, code + 32, event.position, true, &event.modifiers)
            {
                self.terminal.write(&bytes);
            }
            return;
        }
        if !self.view.selecting || event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let Some((point, side)) = grid_point(self, event.position) else { return };
        if let Some(selection) = self.terminal.term.lock().selection.as_mut() {
            selection.update(point, side);
        }
        cx.notify();
    }

    fn mouse_up(&mut self, event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.view.link_clicked) {
            return;
        }
        if let Some(code) = self.view.mouse_button.take() {
            if let Some(bytes) = mouse_report(self, code, event.position, false, &event.modifiers) {
                self.terminal.write(&bytes);
            }
            return;
        }
        self.view.selecting = false;
        let empty = self.terminal.term.lock().selection.as_ref().is_some_and(|selection| selection.is_empty());
        if empty {
            self.terminal.term.lock().selection = None;
        }
        cx.notify();
    }

    fn focus_changed(&mut self, focused: bool, cx: &mut Context<Self>) {
        self.view.focused = focused;
        self.view.cursor_hidden = false;
        if self.terminal.mode().contains(TermMode::FOCUS_IN_OUT) {
            self.terminal.write(if focused { b"\x1b[I" } else { b"\x1b[O" });
        }
        cx.notify();
    }

    fn scroll_wheel(&mut self, event: &ScrollWheelEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(cell) = self.cell_size() else { return };
        let lines = match event.delta {
            ScrollDelta::Pixels(delta) => f32::from(delta.y) / f32::from(cell.height),
            ScrollDelta::Lines(delta) => delta.y,
        };
        self.view.scroll += lines;
        let whole = self.view.scroll.trunc() as i32;
        if whole == 0 {
            return;
        }
        self.view.scroll -= whole as f32;
        let mode = self.terminal.mode();
        if reports_mouse(self, &event.modifiers) {
            let code = if whole > 0 { 64 } else { 65 };
            for _ in 0..whole.unsigned_abs().min(10) {
                if let Some(bytes) = mouse_report(self, code, event.position, true, &event.modifiers) {
                    self.terminal.write(&bytes);
                }
            }
        } else if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL) {
            let key: &[u8] = match (whole > 0, mode.contains(TermMode::APP_CURSOR)) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1b[A",
                (false, true) => b"\x1bOB",
                (false, false) => b"\x1b[B",
            };
            self.terminal.write(&key.repeat(whole.unsigned_abs() as usize));
        } else {
            self.terminal.scroll(Scroll::Delta(whole));
            cx.notify();
        }
    }

    fn drop_paths(&mut self, paths: &ExternalPaths, _window: &mut Window, cx: &mut Context<Self>) {
        let text: Vec<String> =
            paths.paths().iter().map(|path| crate::agents::quote(&path.to_string_lossy())).collect();
        if !text.is_empty() {
            let bytes = keys::paste(&format!("{} ", text.join(" ")), self.terminal.mode());
            self.input(&bytes, cx);
        }
    }
}

impl Render for Session {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.view.theme.clone().unwrap_or_else(|| ui::theme(cx));
        let focused = self.focus.is_focused(window);
        if !self.view.observing_focus {
            self.view.observing_focus = true;
            cx.on_focus(&self.focus, window, |session, _, cx| session.focus_changed(true, cx)).detach();
            cx.on_blur(&self.focus, window, |session, _, cx| session.focus_changed(false, cx)).detach();
        }
        let bar = self.view.find.as_ref().map(|find| self.find_bar(find, cx));
        let pointer = if self.view.hover.is_some() { CursorStyle::PointingHand } else { CursorStyle::IBeam };
        div()
            .relative()
            .size_full()
            .on_action(cx.listener(|session, _: &commands::Find, window, cx| session.open_find(window, cx)))
            .on_action(cx.listener(|session, _: &commands::FindNext, _, cx| session.find_step(1, cx)))
            .on_action(cx.listener(|session, _: &commands::FindPrevious, _, cx| session.find_step(-1, cx)))
            .child(
                div()
                    .id(ElementId::Name(SharedString::from(format!("terminal-{}", self.id))))
                    .key_context("Terminal")
                    .track_focus(&self.focus)
                    .size_full()
                    .bg(theme.background)
                    .px(px(10.0))
                    .py(px(6.0))
                    .cursor(pointer)
                    .on_key_down(cx.listener(Self::key_down))
                    .on_modifiers_changed(cx.listener(|session, event: &gpui::ModifiersChangedEvent, window, cx| {
                        session.update_hover(window.mouse_position(), &event.modifiers, cx);
                    }))
                    .on_action(cx.listener(Self::copy))
                    .on_action(cx.listener(Self::paste))
                    .on_action(cx.listener(Self::select_all))
                    .on_action(cx.listener(Self::clear))
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
                    .on_mouse_down(MouseButton::Middle, cx.listener(Self::mouse_down))
                    .on_mouse_down(MouseButton::Right, cx.listener(Self::mouse_down))
                    .on_mouse_move(cx.listener(Self::mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
                    .on_mouse_up(MouseButton::Middle, cx.listener(Self::mouse_up))
                    .on_mouse_up(MouseButton::Right, cx.listener(Self::mouse_up))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
                    .on_scroll_wheel(cx.listener(Self::scroll_wheel))
                    .on_drop(cx.listener(Self::drop_paths))
                    .child(TerminalElement { session: cx.entity(), focused }),
            )
            .children(bar)
    }
}

impl EntityInputHandler for Session {
    fn text_for_range(
        &mut self,
        _range: Range<usize>,
        _actual: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: 0..0, reversed: false })
    }

    fn marked_text_range(&self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<Range<usize>> {
        self.view.marked.as_ref().map(|text| 0..text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.view.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.view.marked = None;
        if !text.is_empty() {
            self.input(text.as_bytes(), cx);
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _selected: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.view.marked = Some(text.to_string()).filter(|text| !text.is_empty());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let (origin, cell) = self.view.layout?;
        let cursor = self.terminal.cursor_line();
        let offset = self.terminal.term.lock().grid().display_offset() as i32;
        let position = point(
            origin.x + cell.width * cursor.column.0 as f32,
            origin.y + cell.height * (cursor.line.0 + offset) as f32,
        );
        Some(Bounds::new(position, cell))
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

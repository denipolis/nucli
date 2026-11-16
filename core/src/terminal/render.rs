use std::sync::Arc;

use alacritty_terminal::event::EventListener;
use alacritty_terminal::index::Point as GridPoint;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb};
use gpui::{
    App, Bounds, Font, FontFeatures, FontStyle, FontWeight, Hsla, Pixels, Point, Rgba, ShapedLine, SharedString, Size,
    StrikethroughStyle, TextRun, UnderlineStyle, Window, fill, outline, point, px, size,
};

use super::boxes;
use crate::fonts::FAMILY;
use crate::theme::Theme;

pub struct Look<'a> {
    pub theme: &'a Theme,
    pub family: &'a str,
    pub ligatures: bool,
    pub font_size: Pixels,
    pub cell: Size<Pixels>,
    pub focused: bool,
    pub cursor_hidden: bool,
    pub marked: Option<String>,
    pub hover: Option<(GridPoint, GridPoint)>,
    pub found: &'a [(GridPoint, GridPoint)],
    pub current: usize,
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
        fallbacks: fallbacks(family),
        weight: if bold { FontWeight::BOLD } else { FontWeight::NORMAL },
        style: if italic { FontStyle::Italic } else { FontStyle::Normal },
    }
}

fn fallbacks(family: &str) -> Option<gpui::FontFallbacks> {
    let fonts: Vec<String> =
        (family != FAMILY).then(|| FAMILY.to_string()).into_iter().chain(symbol_fonts().map(String::from)).collect();
    (!fonts.is_empty()).then(|| gpui::FontFallbacks::from_fonts(fonts))
}

#[cfg(target_os = "ios")]
fn symbol_fonts() -> impl Iterator<Item = &'static str> {
    ["STIX Two Math", "Zapf Dingbats"].into_iter()
}

#[cfg(not(target_os = "ios"))]
fn symbol_fonts() -> impl Iterator<Item = &'static str> {
    std::iter::empty()
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

pub fn layout<T: EventListener>(
    term: &Arc<FairMutex<Term<T>>>,
    rows: usize,
    bounds: Bounds<Pixels>,
    look: Look,
    window: &mut Window,
) -> Frame {
    let Look { theme, family, ligatures, font_size, cell, cursor_hidden, marked, found, current, .. } = look;
    let table = color_table(theme);
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
        let origin = point(bounds.origin.x + cell.width * column as f32, bounds.origin.y + cell.height * row as f32);
        let cell_bounds = Bounds::new(origin, size(cell.width * width, cell.height));
        let matched = visible
            .iter()
            .find(|(start, end, _)| *start <= indexed.point && indexed.point <= *end)
            .map(|(_, _, active)| *active);
        let hovered = look.hover.is_some_and(|(start, end)| start <= indexed.point && indexed.point <= end);
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
        if !look.focused {
            frame.cursor.push(outline(block, color, gpui::BorderStyle::Solid));
        } else if !cursor_hidden {
            match cursor.shape {
                CursorShape::Beam => frame.cursor.push(fill(Bounds::new(origin, size(px(2.0), cell.height)), color)),
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
                font: font(family, false, false, ligatures),
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
                font: font(family, run.bold, run.italic, ligatures),
                color: run.color,
                background_color: None,
                underline: run.underline.then_some(UnderlineStyle { thickness: px(1.0), color: None, wavy: false }),
                strikethrough: run.strike.then_some(StrikethroughStyle { thickness: px(1.0), color: None }),
            };
            let shaped = window.text_system().shape_line(SharedString::from(run.text), font_size, &[text_run], None);
            frame.lines.push((origin, shaped));
        }
    }
    frame
}

pub fn paint(frame: &mut Frame, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
        for (rect, color) in frame.rects.drain(..) {
            window.paint_quad(fill(rect, color));
        }
        for quad in frame.cursor.drain(..) {
            window.paint_quad(quad);
        }
        for (origin, line) in frame.lines.drain(..) {
            let _ = line.paint(origin, frame.cell.height, gpui::TextAlign::Left, None, window, cx);
        }
    });
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

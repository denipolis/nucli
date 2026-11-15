use gpui::prelude::FluentBuilder;
use gpui::{Div, Hsla, ParentElement, Styled, div, px};

use crate::theme::Theme;

pub const TEXT: &str = "█▄ █ █ █ █▀▀ █   █\n█ ▀█ █▄█ █▄▄ █▄▄ █";
pub const BLINK_MS: u128 = 530;
const ROWS: usize = 4;
const COLUMN_MS: u128 = 50;

fn logo_columns() -> Vec<[bool; ROWS]> {
    let lines: Vec<Vec<char>> = TEXT.lines().map(|line| line.chars().collect()).collect();
    let width = lines.iter().map(Vec::len).max().unwrap_or(0);
    (0..width)
        .map(|column| {
            std::array::from_fn(|row| {
                let cell = lines.get(row / 2).and_then(|line| line.get(column)).copied().unwrap_or(' ');
                if row % 2 == 0 { matches!(cell, '█' | '▀') } else { matches!(cell, '█' | '▄') }
            })
        })
        .collect()
}

pub fn pixels(time: u128, pixel: f32, theme: &Theme) -> Div {
    let columns = logo_columns();
    let revealed = (time.saturating_sub(150) / COLUMN_MS) as usize;
    let blink = revealed >= columns.len() && (time / BLINK_MS).is_multiple_of(2);
    let column = |cells: [Option<Hsla>; ROWS]| {
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .children(cells.map(|color| div().size(px(pixel)).when_some(color, Div::bg)))
    };
    let mut row = div().flex().gap(px(2.0));
    for (index, cells) in columns.iter().enumerate() {
        let age = time.saturating_sub(150 + index as u128 * COLUMN_MS);
        let color = if age < 200 { theme.text } else { theme.accent };
        row = row.child(column(match index.cmp(&revealed) {
            std::cmp::Ordering::Less => cells.map(|on| on.then_some(color)),
            std::cmp::Ordering::Equal => [Some(theme.accent); ROWS],
            std::cmp::Ordering::Greater => [None; ROWS],
        }));
    }
    let mut underline = [None; ROWS];
    underline[ROWS - 1] = blink.then_some(theme.accent);
    row.child(column([None; ROWS])).child(column(underline)).child(column(underline))
}

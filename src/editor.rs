use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, Bounds, ClipboardItem, Context, ElementInputHandler, EntityInputHandler, FocusHandle, Hsla,
    InteractiveElement, IntoElement, KeyBinding, ListHorizontalSizingBehavior, MouseButton, ParentElement, Pixels,
    Point, Render, ScrollStrategy, SharedString, Styled, StyledText, TextRun, UTF16Selection, UniformListScrollHandle,
    Window, actions, canvas, div, point, px, size, uniform_list,
};

use crate::commands::{self, NextChange, PreviousChange, Save};
use crate::find::Hit;
use crate::git::{self, LineKind};
use crate::highlight::{self, Token};
use crate::settings;
use crate::terminal::view::{cell_size, font};
use crate::text_input::TextInput;
use crate::theme::Theme;
use crate::ui;

mod find_bar;

actions!(
    editor,
    [
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        LineStart,
        LineEnd,
        SelectLineStart,
        SelectLineEnd,
        DocumentStart,
        DocumentEnd,
        SelectDocumentStart,
        SelectDocumentEnd,
        PageUp,
        PageDown,
        Backspace,
        DeleteForward,
        DeleteWordLeft,
        DeleteLineLeft,
        Newline,
        Indent,
    ]
);

pub fn bindings() -> Vec<KeyBinding> {
    let c = Some("FileView");
    let mac = cfg!(target_os = "macos");
    let primary = if mac { "cmd" } else { "ctrl" };
    let clipboard = [
        KeyBinding::new(&format!("{primary}-c"), commands::Copy, c),
        KeyBinding::new(&format!("{primary}-v"), commands::Paste, c),
        KeyBinding::new(&format!("{primary}-a"), commands::SelectAll, c),
    ];
    let word = if mac { "alt" } else { "ctrl" };
    let mut keys = vec![
        KeyBinding::new("left", MoveLeft, c),
        KeyBinding::new("right", MoveRight, c),
        KeyBinding::new("up", MoveUp, c),
        KeyBinding::new("down", MoveDown, c),
        KeyBinding::new("shift-left", SelectLeft, c),
        KeyBinding::new("shift-right", SelectRight, c),
        KeyBinding::new("shift-up", SelectUp, c),
        KeyBinding::new("shift-down", SelectDown, c),
        KeyBinding::new(&format!("{word}-left"), WordLeft, c),
        KeyBinding::new(&format!("{word}-right"), WordRight, c),
        KeyBinding::new(&format!("{word}-shift-left"), SelectWordLeft, c),
        KeyBinding::new(&format!("{word}-shift-right"), SelectWordRight, c),
        KeyBinding::new("home", LineStart, c),
        KeyBinding::new("end", LineEnd, c),
        KeyBinding::new("shift-home", SelectLineStart, c),
        KeyBinding::new("shift-end", SelectLineEnd, c),
        KeyBinding::new("pageup", PageUp, c),
        KeyBinding::new("pagedown", PageDown, c),
        KeyBinding::new("backspace", Backspace, c),
        KeyBinding::new("delete", DeleteForward, c),
        KeyBinding::new(&format!("{word}-backspace"), DeleteWordLeft, c),
        KeyBinding::new("enter", Newline, c),
        KeyBinding::new("tab", Indent, c),
    ];
    keys.extend(clipboard);
    if mac {
        keys.extend([
            KeyBinding::new("cmd-left", LineStart, c),
            KeyBinding::new("cmd-right", LineEnd, c),
            KeyBinding::new("cmd-shift-left", SelectLineStart, c),
            KeyBinding::new("cmd-shift-right", SelectLineEnd, c),
            KeyBinding::new("cmd-up", DocumentStart, c),
            KeyBinding::new("cmd-down", DocumentEnd, c),
            KeyBinding::new("cmd-shift-up", SelectDocumentStart, c),
            KeyBinding::new("cmd-shift-down", SelectDocumentEnd, c),
            KeyBinding::new("cmd-backspace", DeleteLineLeft, c),
        ]);
    } else {
        keys.extend([
            KeyBinding::new("ctrl-home", DocumentStart, c),
            KeyBinding::new("ctrl-end", DocumentEnd, c),
            KeyBinding::new("ctrl-shift-home", SelectDocumentStart, c),
            KeyBinding::new("ctrl-shift-end", SelectDocumentEnd, c),
            KeyBinding::new("ctrl-y", commands::Redo, c),
        ]);
    }
    keys
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub kind: LineKind,
    pub text: String,
    pub old: Option<usize>,
    pub new: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Pos {
    pub row: usize,
    pub col: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Loading,
    Text,
    Image,
    Binary(u64),
    Missing,
}

#[derive(Clone)]
struct Snapshot {
    rows: Vec<Row>,
    cursor: Pos,
    anchor: Option<Pos>,
}

pub struct Loaded {
    pub content: Content,
    pub rows: Vec<Row>,
    pub text: String,
    pub trailing_newline: bool,
    pub modified: Option<SystemTime>,
}

const IMAGES: [&str; 8] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico"];

pub fn load(project: &Path, path: &str, commit: Option<&str>) -> Loaded {
    let full = if Path::new(path).is_absolute() { PathBuf::from(path) } else { project.join(path) };
    let meta = std::fs::metadata(&full).ok();
    let modified = meta.as_ref().and_then(|meta| meta.modified().ok());
    let extension = full.extension().map(|ext| ext.to_string_lossy().to_lowercase()).unwrap_or_default();
    let empty =
        |content: Content| Loaded { content, rows: Vec::new(), text: String::new(), trailing_newline: false, modified };
    if commit.is_none() && IMAGES.contains(&extension.as_str()) && full.is_file() {
        return empty(Content::Image);
    }
    let lines = if Path::new(path).is_absolute() { None } else { git::file_diff(project, path, commit) };
    let rows: Vec<Row> = match lines {
        Some(lines) => lines
            .into_iter()
            .map(|line| Row { kind: line.kind, text: line.text, old: line.old, new: line.new })
            .collect(),
        None => {
            let Some(meta) = meta.filter(|meta| meta.is_file()) else { return empty(Content::Missing) };
            let Ok(bytes) = std::fs::read(&full) else { return empty(Content::Binary(meta.len())) };
            if bytes.len() > 4000000 || bytes.contains(&0) {
                return empty(Content::Binary(meta.len()));
            }
            let text = String::from_utf8_lossy(&bytes);
            text.lines()
                .enumerate()
                .map(|(index, line)| Row {
                    kind: LineKind::Context,
                    text: line.trim_end_matches('\r').to_string(),
                    old: Some(index + 1),
                    new: Some(index + 1),
                })
                .collect()
        }
    };
    let disk = std::fs::read(&full).ok().map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default();
    Loaded { content: Content::Text, trailing_newline: disk.ends_with('\n'), text: disk, rows, modified }
}

pub fn file_text(rows: &[Row], trailing_newline: bool) -> String {
    let mut text = rows
        .iter()
        .filter(|row| row.kind != LineKind::Removed)
        .map(|row| row.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if trailing_newline {
        text.push('\n');
    }
    text
}

fn highlights(language: Option<&str>, rows: &[Row]) -> Vec<Vec<(Range<usize>, Token)>> {
    let mut result: Vec<Vec<(Range<usize>, Token)>> = vec![Vec::new(); rows.len()];
    for removed_side in [false, true] {
        let skip = if removed_side { LineKind::Added } else { LineKind::Removed };
        let mut offsets = Vec::new();
        let mut text = String::new();
        for (index, row) in rows.iter().enumerate() {
            if row.kind != skip {
                offsets.push((text.len(), index));
                text.push_str(&row.text);
                text.push('\n');
            }
        }
        let Some(tokens) = language.and_then(|language| highlight::highlight(language, &text)) else { continue };
        for (range, token) in tokens {
            let mut slot = offsets.partition_point(|(start, _)| *start <= range.start).saturating_sub(1);
            let mut start = range.start;
            while start < range.end && slot < offsets.len() {
                let (line_start, row) = offsets[slot];
                let line_end = line_start + rows[row].text.len();
                let end = range.end.min(line_end);
                let wanted = (rows[row].kind == LineKind::Removed) == removed_side;
                if start < end && wanted {
                    result[row].push((start - line_start..end - line_start, token));
                }
                slot += 1;
                start = offsets.get(slot).map(|(next, _)| *next).unwrap_or(range.end);
            }
        }
    }
    result
}

fn token_color(token: Token, theme: &Theme) -> Hsla {
    let syntax = &theme.syntax;
    match token {
        Token::Keyword => syntax.keyword,
        Token::String => syntax.string,
        Token::Number => syntax.number,
        Token::Comment => syntax.comment,
        Token::Type => syntax.kind,
        Token::Function => syntax.function,
        Token::Constant => syntax.constant,
        Token::Attribute => syntax.attribute,
        Token::Punctuation => syntax.punctuation,
        Token::Property | Token::Tag | Token::Variable => syntax.attribute,
    }
}

pub struct FindState {
    input: gpui::Entity<TextInput>,
    replacement: gpui::Entity<TextInput>,
    show_replace: bool,
    hits: Vec<Hit>,
    by_row: HashMap<usize, Vec<(usize, Range<usize>)>>,
    current: usize,
    invalid: bool,
    scope: Option<(Pos, Pos)>,
    notice: Option<String>,
}

pub struct Editor {
    pub find: Option<FindState>,
    pub project: PathBuf,
    pub path: String,
    pub commit: Option<String>,
    pub full: PathBuf,
    pub focus: FocusHandle,
    pub content: Content,
    pub rows: Vec<Row>,
    tokens: Vec<Vec<(Range<usize>, Token)>>,
    pub cursor: Pos,
    pub anchor: Option<Pos>,
    original: String,
    trailing_newline: bool,
    modified: Option<SystemTime>,
    pub dirty: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_edit: Option<Instant>,
    pub scroll: UniformListScrollHandle,
    selecting: bool,
    code_origin: Point<Pixels>,
    marked: Option<String>,
    pending_line: Option<usize>,
    first_load: bool,
    pub added: usize,
    pub removed: usize,
    pub language: Option<&'static str>,
}

impl Editor {
    pub fn new(
        project: PathBuf,
        path: String,
        commit: Option<String>,
        language: Option<&'static str>,
        cx: &mut Context<Self>,
    ) -> Self {
        let full = if Path::new(&path).is_absolute() { PathBuf::from(&path) } else { project.join(&path) };
        let mut editor = Editor {
            find: None,
            project,
            path,
            commit,
            full,
            focus: cx.focus_handle(),
            content: Content::Loading,
            rows: Vec::new(),
            tokens: Vec::new(),
            cursor: Pos::default(),
            anchor: None,
            original: String::new(),
            trailing_newline: true,
            modified: None,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            scroll: UniformListScrollHandle::new(),
            selecting: false,
            code_origin: point(px(0.0), px(0.0)),
            marked: None,
            pending_line: None,
            first_load: true,
            added: 0,
            removed: 0,
            language,
        };
        editor.reload(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(1000)).await;
                let alive = this.update(cx, |editor, cx| editor.check_disk(cx));
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        editor
    }

    fn check_disk(&mut self, cx: &mut Context<Self>) {
        if self.dirty || self.commit.is_some() {
            return;
        }
        let modified = std::fs::metadata(&self.full).ok().and_then(|meta| meta.modified().ok());
        if modified != self.modified {
            self.reload(cx);
        }
    }

    fn current_language(&self) -> Option<&'static str> {
        self.language.or_else(|| highlight::language_name(&self.full))
    }

    pub fn language_label(&self) -> &'static str {
        self.current_language().unwrap_or(highlight::PLAIN_TEXT)
    }

    pub fn set_language(&mut self, language: Option<&'static str>, cx: &mut Context<Self>) {
        self.language = language;
        self.refresh_tokens(cx);
        cx.notify();
    }

    fn refresh_tokens(&mut self, cx: &mut Context<Self>) {
        let rows = self.rows.clone();
        let language = self.current_language();
        cx.spawn(async move |this, cx| {
            let tokens = cx.background_executor().spawn(async move { highlights(language, &rows) }).await;
            let _ = this.update(cx, |editor, cx| {
                if tokens.len() == editor.rows.len() {
                    editor.tokens = tokens;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn is_editable(&self) -> bool {
        self.commit.is_none() && self.content == Content::Text
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        if self.dirty {
            return;
        }
        let (project, path, commit) = (self.project.clone(), self.path.clone(), self.commit.clone());
        let language = self.current_language();
        cx.spawn(async move |this, cx| {
            let (loaded, tokens) = cx
                .background_executor()
                .spawn(async move {
                    let loaded = load(&project, &path, commit.as_deref());
                    let tokens = highlights(language, &loaded.rows);
                    (loaded, tokens)
                })
                .await;
            let _ = this.update(cx, |editor, cx| editor.apply(loaded, tokens, cx));
        })
        .detach();
    }

    fn apply(&mut self, loaded: Loaded, tokens: Vec<Vec<(Range<usize>, Token)>>, cx: &mut Context<Self>) {
        if self.dirty {
            return;
        }
        let line = self.logical_line();
        self.content = loaded.content;
        self.rows = loaded.rows;
        self.tokens = tokens;
        self.original = loaded.text;
        self.trailing_newline = loaded.trailing_newline;
        self.modified = loaded.modified;
        self.undo.clear();
        self.redo.clear();
        self.added = self.rows.iter().filter(|row| row.kind == LineKind::Added).count();
        self.removed = self.rows.iter().filter(|row| row.kind == LineKind::Removed).count();
        if let Some(target) = self.pending_line.take() {
            self.goto_line(target);
        } else if self.first_load {
            if let Some(first) = self.rows.iter().position(|row| row.kind != LineKind::Context) {
                self.cursor = Pos { row: first, col: 0 };
                self.scroll.scroll_to_item(first, ScrollStrategy::Center);
            }
        } else if let Some((line, col)) = line {
            let row =
                self.rows.iter().position(|row| row.new == Some(line)).unwrap_or(self.rows.len().saturating_sub(1));
            self.cursor = Pos { row, col: col.min(self.rows.get(row).map_or(0, |row| row.text.len())) };
            self.cursor.col = self.clamp_col(self.cursor.row, self.cursor.col);
            self.anchor = None;
        }
        self.first_load = false;
        if self.find.is_some() {
            self.search(false, cx);
        }
        cx.notify();
    }

    fn logical_line(&self) -> Option<(usize, usize)> {
        let row = self.rows.get(self.cursor.row)?;
        Some((row.new?, self.cursor.col))
    }

    pub fn goto_line(&mut self, line: usize) {
        if self.rows.is_empty() {
            self.pending_line = Some(line);
            return;
        }
        let row = self.rows.iter().position(|row| row.new == Some(line)).unwrap_or(self.rows.len() - 1);
        self.cursor = Pos { row, col: 0 };
        self.anchor = None;
        self.scroll.scroll_to_item(row, ScrollStrategy::Center);
    }

    pub fn position_label(&self) -> String {
        let column = self
            .rows
            .get(self.cursor.row)
            .map_or(0, |row| row.text[..self.cursor.col.min(row.text.len())].chars().count());
        let line = self.rows.get(self.cursor.row).and_then(|row| row.new.or(row.old)).unwrap_or(self.cursor.row + 1);
        format!("Ln {line}, Col {}", column + 1)
    }

    fn clamp_col(&self, row: usize, col: usize) -> usize {
        let Some(text) = self.rows.get(row).map(|row| row.text.as_str()) else { return 0 };
        let mut col = col.min(text.len());
        while !text.is_char_boundary(col) {
            col -= 1;
        }
        col
    }

    fn selection(&self) -> Option<(Pos, Pos)> {
        let anchor = self.anchor?;
        if anchor == self.cursor {
            return None;
        }
        Some(if anchor < self.cursor { (anchor, self.cursor) } else { (self.cursor, anchor) })
    }

    fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection()?;
        if start.row == end.row {
            return Some(self.rows[start.row].text[start.col..end.col].to_string());
        }
        let mut parts = vec![self.rows[start.row].text[start.col..].to_string()];
        for row in &self.rows[start.row + 1..end.row] {
            parts.push(row.text.clone());
        }
        parts.push(self.rows[end.row].text[..end.col].to_string());
        Some(parts.join("\n"))
    }

    fn snapshot(&mut self, group: bool) {
        let now = Instant::now();
        let coalesce =
            group && self.last_edit.is_some_and(|last| now.duration_since(last) < Duration::from_millis(600));
        self.last_edit = Some(now);
        if !coalesce {
            self.undo.push(Snapshot { rows: self.rows.clone(), cursor: self.cursor, anchor: self.anchor });
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
    }

    fn after_edit(&mut self, cx: &mut Context<Self>) {
        self.dirty = file_text(&self.rows, self.trailing_newline) != self.original;
        self.tokens.resize(self.rows.len(), Vec::new());
        self.scroll.scroll_to_item(self.cursor.row, ScrollStrategy::Top);
        cx.notify();
        self.refresh_tokens(cx);
        if self.find.is_some() {
            self.search(false, cx);
        }
    }

    fn delete_selection(&mut self) -> bool {
        let Some((mut start, end)) = self.selection() else { return false };
        while start.row < end.row && self.rows[start.row].kind == LineKind::Removed {
            start = Pos { row: start.row + 1, col: 0 };
        }
        let tail = self.rows[end.row].text[end.col..].to_string();
        let row = &mut self.rows[start.row];
        if row.kind == LineKind::Removed {
            self.anchor = None;
            return false;
        }
        row.text.truncate(start.col);
        row.text.push_str(&tail);
        if row.kind == LineKind::Context {
            row.kind = LineKind::Context;
        }
        self.rows.drain(start.row + 1..=end.row);
        self.tokens.drain(start.row + 1..=end.row.min(self.tokens.len().saturating_sub(1)));
        self.cursor = start;
        self.anchor = None;
        true
    }

    fn insert(&mut self, text: &str, cx: &mut Context<Self>) {
        if !self.is_editable() {
            return;
        }
        if self.rows.is_empty() {
            self.rows.push(Row { kind: LineKind::Added, text: String::new(), old: None, new: None });
        }
        self.snapshot(!text.contains('\n') && text.chars().count() == 1 && self.anchor.is_none());
        self.delete_selection();
        if self.rows[self.cursor.row].kind == LineKind::Removed {
            cx.notify();
            return;
        }
        self.insert_at_cursor(text);
        self.after_edit(cx);
    }

    fn insert_at_cursor(&mut self, text: &str) {
        let row = self.cursor.row;
        let col = self.cursor.col;
        let tail = self.rows[row].text.split_off(col);
        let mut lines = text.split('\n');
        self.rows[row].text.push_str(lines.next().unwrap_or(""));
        let mut current = row;
        for line in lines {
            current += 1;
            self.rows.insert(
                current,
                Row { kind: LineKind::Added, text: line.trim_end_matches('\r').to_string(), old: None, new: None },
            );
            self.tokens.insert(current.min(self.tokens.len()), Vec::new());
        }
        self.cursor = Pos { row: current, col: self.rows[current].text.len() };
        self.rows[current].text.push_str(&tail);
        if let Some(tokens) = self.tokens.get_mut(row) {
            tokens.retain(|(range, _)| range.end <= col);
        }
    }

    pub fn save(&mut self, _: &Save, _: &mut Window, cx: &mut Context<Self>) {
        self.save_file(cx);
    }

    pub fn save_file(&mut self, cx: &mut Context<Self>) {
        if !self.is_editable() || !self.dirty {
            return;
        }
        let text = file_text(&self.rows, self.trailing_newline);
        if std::fs::write(&self.full, &text).is_ok() {
            self.original = text;
            self.dirty = false;
            self.reload(cx);
        }
        cx.notify();
    }

    fn move_to(&mut self, pos: Pos, select: bool, cx: &mut Context<Self>) {
        if select {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else {
            self.anchor = None;
        }
        self.cursor = pos;
        self.scroll.scroll_to_item(pos.row, ScrollStrategy::Top);
        cx.notify();
    }

    fn left_of(&self, pos: Pos) -> Pos {
        if pos.col > 0 {
            let text = &self.rows[pos.row].text;
            let col = text[..pos.col].char_indices().next_back().map(|(index, _)| index).unwrap_or(0);
            Pos { row: pos.row, col }
        } else if pos.row > 0 {
            Pos { row: pos.row - 1, col: self.rows[pos.row - 1].text.len() }
        } else {
            pos
        }
    }

    fn right_of(&self, pos: Pos) -> Pos {
        let Some(row) = self.rows.get(pos.row) else { return pos };
        if pos.col < row.text.len() {
            let width = row.text[pos.col..].chars().next().map_or(0, char::len_utf8);
            Pos { row: pos.row, col: pos.col + width }
        } else if pos.row + 1 < self.rows.len() {
            Pos { row: pos.row + 1, col: 0 }
        } else {
            pos
        }
    }

    fn word_left_of(&self, pos: Pos) -> Pos {
        if pos.col == 0 {
            return self.left_of(pos);
        }
        let text = &self.rows[pos.row].text[..pos.col];
        let trimmed = text.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_');
        let col = trimmed.rfind(|c: char| !c.is_alphanumeric() && c != '_').map(|index| index + 1).unwrap_or(0);
        Pos { row: pos.row, col: self.clamp_col(pos.row, col) }
    }

    fn word_right_of(&self, pos: Pos) -> Pos {
        let Some(row) = self.rows.get(pos.row) else { return pos };
        if pos.col >= row.text.len() {
            return self.right_of(pos);
        }
        let text = &row.text[pos.col..];
        let skipped = text.len() - text.trim_start_matches(|c: char| !c.is_alphanumeric() && c != '_').len();
        let rest = &text[skipped..];
        let word = rest.find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(rest.len());
        Pos { row: pos.row, col: pos.col + skipped + word }
    }

    fn vertical(&self, pos: Pos, delta: i32) -> Pos {
        if self.rows.is_empty() {
            return pos;
        }
        let row = (pos.row as i32 + delta).clamp(0, self.rows.len() as i32 - 1) as usize;
        let chars = self.rows[pos.row].text[..pos.col.min(self.rows[pos.row].text.len())].chars().count();
        let col =
            self.rows[row].text.char_indices().nth(chars).map(|(index, _)| index).unwrap_or(self.rows[row].text.len());
        Pos { row, col }
    }

    pub fn jump_change(&mut self, forward: bool, cx: &mut Context<Self>) {
        let is_change = |row: &Row| row.kind != LineKind::Context;
        let start = self.cursor.row;
        let mut blocks = Vec::new();
        for index in 0..self.rows.len() {
            if is_change(&self.rows[index]) && (index == 0 || !is_change(&self.rows[index - 1])) {
                blocks.push(index);
            }
        }
        let target = if forward {
            blocks.iter().find(|row| **row > start).or(blocks.first())
        } else {
            blocks.iter().rev().find(|row| **row < start).or(blocks.last())
        };
        if let Some(target) = target.copied() {
            self.cursor = Pos { row: target, col: 0 };
            self.anchor = None;
            self.scroll.scroll_to_item(target, ScrollStrategy::Center);
            cx.notify();
        }
    }

    fn pos_at(&self, row: usize, x: Pixels, window: &Window, cx: &App) -> Pos {
        let (_, cell) = cell_size(window, cx);
        let target = (f32::from(x - self.code_origin.x) / f32::from(cell.width)).max(0.0);
        let Some(line) = self.rows.get(row) else { return Pos { row: self.rows.len().saturating_sub(1), col: 0 } };
        let mut width = 0.0;
        for (index, character) in line.text.char_indices() {
            let advance = char_width(character);
            if width + advance / 2.0 > target {
                return Pos { row, col: index };
            }
            width += advance;
        }
        Pos { row, col: line.text.len() }
    }

    fn row_runs(&self, index: usize, theme: &Theme, family: &str, ligatures: bool) -> Vec<TextRun> {
        let row = &self.rows[index];
        let base = TextRun {
            len: 0,
            font: font(family, false, false, ligatures),
            color: theme.text,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let mut runs = Vec::new();
        let mut position = 0;
        let empty = Vec::new();
        for (range, token) in self.tokens.get(index).unwrap_or(&empty) {
            if range.start < position || range.end > row.text.len() {
                continue;
            }
            if range.start > position {
                runs.push(TextRun { len: range.start - position, ..base.clone() });
            }
            runs.push(TextRun { len: range.end - range.start, color: token_color(*token, theme), ..base.clone() });
            position = range.end;
        }
        if position < row.text.len() {
            runs.push(TextRun { len: row.text.len() - position, ..base });
        }
        runs
    }
}

fn char_width(character: char) -> f32 {
    let wide = matches!(character as u32,
        0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF
        | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1FAFF | 0x20000..=0x3FFFD);
    if wide { 2.0 } else { 1.0 }
}

fn text_width(text: &str) -> f32 {
    text.chars().map(char_width).sum()
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui::theme(cx);
        let focused = self.focus.is_focused(window);
        let count = self.rows.len();
        let (font_size, cell) = cell_size(window, cx);
        let two_columns = self.removed > 0;
        let digits = self.rows.iter().filter_map(|row| row.new.or(row.old)).max().unwrap_or(1).to_string().len().max(3);
        let selection = self.selection();
        let cursor = self.cursor;
        let (family, ligatures) = (settings::get(cx).font.clone(), settings::get(cx).ligatures);
        let list = uniform_list(
            "editor-rows",
            count,
            cx.processor(move |editor: &mut Editor, range: Range<usize>, _window, cx| {
                let theme = ui::theme(cx);
                range
                    .map(|index| {
                        let row = &editor.rows[index];
                        let background = match row.kind {
                            LineKind::Added => Some(theme.added),
                            LineKind::Removed => Some(theme.removed),
                            LineKind::Context if index == cursor.row && selection.is_none() => Some(theme.hover),
                            LineKind::Context => None,
                        };
                        let marker = match row.kind {
                            LineKind::Added => "+",
                            LineKind::Removed => "-",
                            LineKind::Context => " ",
                        };
                        let number = |value: Option<usize>| value.map(|value| value.to_string()).unwrap_or_default();
                        let selected = selection.and_then(|(start, end)| {
                            if index < start.row || index > end.row {
                                return None;
                            }
                            let from = if index == start.row { text_width(&row.text[..start.col]) } else { 0.0 };
                            let to = if index == end.row {
                                text_width(&row.text[..end.col])
                            } else {
                                text_width(&row.text) + 1.0
                            };
                            Some((from, to))
                        });
                        let hits: Vec<(f32, f32, bool)> = editor
                            .find
                            .as_ref()
                            .and_then(|find| {
                                find.by_row.get(&index).map(|found| {
                                    found
                                        .iter()
                                        .map(|(position, range)| {
                                            (
                                                text_width(&row.text[..range.start]),
                                                text_width(&row.text[..range.end]),
                                                *position == find.current,
                                            )
                                        })
                                        .collect()
                                })
                            })
                            .unwrap_or_default();
                        let caret = (index == cursor.row && focused)
                            .then(|| text_width(&row.text[..cursor.col.min(row.text.len())]));
                        let text = SharedString::from(row.text.clone());
                        let runs = editor.row_runs(index, &theme, &family, ligatures);
                        let this = cx.entity();
                        let marked = if index == cursor.row { editor.marked.clone() } else { None };
                        div()
                            .id(("editor-row", index))
                            .h(cell.height)
                            .flex()
                            .flex_row()
                            .min_w_full()
                            .when_some(background, |this, color| this.bg(color))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |editor, event: &gpui::MouseDownEvent, window, cx| {
                                    window.focus(&editor.focus);
                                    let pos = editor.pos_at(index, event.position.x, window, cx);
                                    if event.click_count == 2 {
                                        let start = editor.word_left_of(editor.right_of(pos));
                                        editor.anchor = Some(start);
                                        editor.cursor = editor.word_right_of(start);
                                    } else if event.click_count >= 3 {
                                        editor.anchor = Some(Pos { row: index, col: 0 });
                                        editor.cursor = Pos { row: index, col: editor.rows[index].text.len() };
                                    } else if event.modifiers.shift {
                                        if editor.anchor.is_none() {
                                            editor.anchor = Some(editor.cursor);
                                        }
                                        editor.cursor = pos;
                                    } else {
                                        editor.cursor = pos;
                                        editor.anchor = Some(pos);
                                    }
                                    editor.selecting = true;
                                    cx.notify();
                                }),
                            )
                            .on_mouse_move(cx.listener(move |editor, event: &gpui::MouseMoveEvent, window, cx| {
                                if editor.selecting && event.pressed_button == Some(MouseButton::Left) {
                                    editor.cursor = editor.pos_at(index, event.position.x, window, cx);
                                    cx.notify();
                                }
                            }))
                            .child(
                                div()
                                    .flex()
                                    .flex_none()
                                    .gap(cell.width)
                                    .pl(cell.width)
                                    .pr(cell.width)
                                    .text_color(if index == cursor.row { theme.muted } else { theme.faint })
                                    .when(two_columns, |this| {
                                        this.child(
                                            div()
                                                .w(cell.width * digits as f32)
                                                .flex()
                                                .justify_end()
                                                .child(number(row.old)),
                                        )
                                    })
                                    .child(
                                        div().w(cell.width * digits as f32).flex().justify_end().child(number(row.new)),
                                    )
                                    .child(div().w(cell.width).child(marker)),
                            )
                            .child(
                                div()
                                    .relative()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(font_size)
                                    .child(
                                        canvas(
                                            move |bounds, _, cx| {
                                                this.update(cx, |editor, _| editor.code_origin = bounds.origin);
                                            },
                                            |_, _, _, _| {},
                                        )
                                        .absolute()
                                        .size_full(),
                                    )
                                    .when_some(selected, |this, (from, to)| {
                                        this.child(
                                            div()
                                                .absolute()
                                                .top_0()
                                                .bottom_0()
                                                .left(cell.width * from)
                                                .w(cell.width * (to - from))
                                                .bg(theme.selection),
                                        )
                                    })
                                    .children(hits.into_iter().map(|(from, to, current)| {
                                        div()
                                            .absolute()
                                            .top_0()
                                            .bottom_0()
                                            .left(cell.width * from)
                                            .w(cell.width * (to - from).max(0.25))
                                            .bg(if current {
                                                theme.orange.opacity(0.7)
                                            } else {
                                                theme.yellow.opacity(0.35)
                                            })
                                    }))
                                    .child(div().whitespace_nowrap().child(StyledText::new(text).with_runs(runs)))
                                    .when_some(caret, |this, x| {
                                        this.child(
                                            div()
                                                .absolute()
                                                .top_0()
                                                .bottom_0()
                                                .left(cell.width * x)
                                                .w(px(2.0))
                                                .bg(theme.accent),
                                        )
                                    })
                                    .when_some(marked, |this, text| {
                                        let x = text_width(
                                            &editor.rows[index].text[..cursor.col.min(editor.rows[index].text.len())],
                                        );
                                        this.child(
                                            div()
                                                .absolute()
                                                .top_0()
                                                .left(cell.width * x)
                                                .bg(theme.elevated)
                                                .underline()
                                                .child(text),
                                        )
                                    }),
                            )
                            .into_any_element()
                    })
                    .collect::<Vec<AnyElement>>()
            }),
        )
        .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
        .track_scroll(self.scroll.clone())
        .size_full();
        div()
            .id("editor")
            .key_context("FileView")
            .track_focus(&self.focus)
            .size_full()
            .bg(theme.background)
            .font_family(settings::get(cx).font.clone())
            .text_size(font_size)
            .line_height(cell.height)
            .on_mouse_up(MouseButton::Left, cx.listener(|editor, _, _, _| editor.selecting = false))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|editor, _, _, _| editor.selecting = false))
            .relative()
            .on_action(cx.listener(|editor, _: &commands::Find, window, cx| editor.open_find(false, window, cx)))
            .on_action(cx.listener(|editor, _: &commands::Replace, window, cx| editor.open_find(true, window, cx)))
            .on_action(cx.listener(|editor, _: &commands::HideFileView, window, cx| {
                if editor.find.is_some() {
                    editor.close_find(window, cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|editor, _: &commands::FindNext, _, cx| editor.find_step(1, cx)))
            .on_action(cx.listener(|editor, _: &commands::FindPrevious, _, cx| editor.find_step(-1, cx)))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(|editor, _: &NextChange, _, cx| editor.jump_change(true, cx)))
            .on_action(cx.listener(|editor, _: &PreviousChange, _, cx| editor.jump_change(false, cx)))
            .on_action(cx.listener(|editor, _: &MoveLeft, _, cx| {
                let pos = match editor.selection() {
                    Some((start, _)) => start,
                    None => editor.left_of(editor.cursor),
                };
                editor.move_to(pos, false, cx)
            }))
            .on_action(cx.listener(|editor, _: &MoveRight, _, cx| {
                let pos = match editor.selection() {
                    Some((_, end)) => end,
                    None => editor.right_of(editor.cursor),
                };
                editor.move_to(pos, false, cx)
            }))
            .on_action(
                cx.listener(|editor, _: &MoveUp, _, cx| editor.move_to(editor.vertical(editor.cursor, -1), false, cx)),
            )
            .on_action(
                cx.listener(|editor, _: &MoveDown, _, cx| editor.move_to(editor.vertical(editor.cursor, 1), false, cx)),
            )
            .on_action(
                cx.listener(|editor, _: &SelectLeft, _, cx| editor.move_to(editor.left_of(editor.cursor), true, cx)),
            )
            .on_action(
                cx.listener(|editor, _: &SelectRight, _, cx| editor.move_to(editor.right_of(editor.cursor), true, cx)),
            )
            .on_action(
                cx.listener(|editor, _: &SelectUp, _, cx| editor.move_to(editor.vertical(editor.cursor, -1), true, cx)),
            )
            .on_action(
                cx.listener(|editor, _: &SelectDown, _, cx| {
                    editor.move_to(editor.vertical(editor.cursor, 1), true, cx)
                }),
            )
            .on_action(
                cx.listener(|editor, _: &WordLeft, _, cx| {
                    editor.move_to(editor.word_left_of(editor.cursor), false, cx)
                }),
            )
            .on_action(cx.listener(|editor, _: &WordRight, _, cx| {
                editor.move_to(editor.word_right_of(editor.cursor), false, cx)
            }))
            .on_action(cx.listener(|editor, _: &SelectWordLeft, _, cx| {
                editor.move_to(editor.word_left_of(editor.cursor), true, cx)
            }))
            .on_action(cx.listener(|editor, _: &SelectWordRight, _, cx| {
                editor.move_to(editor.word_right_of(editor.cursor), true, cx)
            }))
            .on_action(cx.listener(|editor, _: &LineStart, _, cx| {
                editor.move_to(Pos { row: editor.cursor.row, col: 0 }, false, cx)
            }))
            .on_action(cx.listener(|editor, _: &SelectLineStart, _, cx| {
                editor.move_to(Pos { row: editor.cursor.row, col: 0 }, true, cx)
            }))
            .on_action(cx.listener(|editor, _: &LineEnd, _, cx| {
                let col = editor.rows.get(editor.cursor.row).map_or(0, |row| row.text.len());
                editor.move_to(Pos { row: editor.cursor.row, col }, false, cx)
            }))
            .on_action(cx.listener(|editor, _: &SelectLineEnd, _, cx| {
                let col = editor.rows.get(editor.cursor.row).map_or(0, |row| row.text.len());
                editor.move_to(Pos { row: editor.cursor.row, col }, true, cx)
            }))
            .on_action(cx.listener(|editor, _: &DocumentStart, _, cx| editor.move_to(Pos::default(), false, cx)))
            .on_action(cx.listener(|editor, _: &SelectDocumentStart, _, cx| editor.move_to(Pos::default(), true, cx)))
            .on_action(cx.listener(|editor, _: &DocumentEnd, _, cx| {
                let row = editor.rows.len().saturating_sub(1);
                let col = editor.rows.get(row).map_or(0, |row| row.text.len());
                editor.move_to(Pos { row, col }, false, cx)
            }))
            .on_action(cx.listener(|editor, _: &SelectDocumentEnd, _, cx| {
                let row = editor.rows.len().saturating_sub(1);
                let col = editor.rows.get(row).map_or(0, |row| row.text.len());
                editor.move_to(Pos { row, col }, true, cx)
            }))
            .on_action(
                cx.listener(|editor, _: &PageUp, _, cx| editor.move_to(editor.vertical(editor.cursor, -30), false, cx)),
            )
            .on_action(
                cx.listener(|editor, _: &PageDown, _, cx| {
                    editor.move_to(editor.vertical(editor.cursor, 30), false, cx)
                }),
            )
            .on_action(
                cx.listener(|editor, _: &Backspace, _, cx| editor.delete_with(|editor, pos| editor.left_of(pos), cx)),
            )
            .on_action(cx.listener(|editor, _: &DeleteForward, _, cx| {
                editor.delete_with(|editor, pos| editor.right_of(pos), cx)
            }))
            .on_action(cx.listener(|editor, _: &DeleteWordLeft, _, cx| {
                editor.delete_with(|editor, pos| editor.word_left_of(pos), cx)
            }))
            .on_action(cx.listener(|editor, _: &DeleteLineLeft, _, cx| {
                editor.delete_with(|_, pos| Pos { row: pos.row, col: 0 }, cx)
            }))
            .on_action(cx.listener(|editor, _: &Newline, _, cx| {
                let indent: String = editor
                    .rows
                    .get(editor.cursor.row)
                    .map(|row| row.text.chars().take_while(|c| *c == ' ' || *c == '\t').collect())
                    .unwrap_or_default();
                editor.insert(&format!("\n{indent}"), cx);
            }))
            .on_action(cx.listener(|editor, _: &Indent, _, cx| editor.insert("    ", cx)))
            .on_action(cx.listener(|editor, _: &commands::Copy, _, cx| {
                if let Some(text) = editor.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }))
            .on_action(cx.listener(|editor, _: &commands::Cut, _, cx| {
                if let Some(text) = editor.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    if editor.is_editable() {
                        editor.snapshot(false);
                        editor.delete_selection();
                        editor.after_edit(cx);
                    }
                }
            }))
            .on_action(cx.listener(|editor, _: &commands::Paste, _, cx| {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    editor.insert(&text, cx);
                }
            }))
            .on_action(cx.listener(|editor, _: &commands::SelectAll, _, cx| {
                let row = editor.rows.len().saturating_sub(1);
                editor.anchor = Some(Pos::default());
                editor.cursor = Pos { row, col: editor.rows.get(row).map_or(0, |row| row.text.len()) };
                cx.notify();
            }))
            .on_action(cx.listener(|editor, _: &commands::Undo, _, cx| editor.history(true, cx)))
            .on_action(cx.listener(|editor, _: &commands::Redo, _, cx| editor.history(false, cx)))
            .child(list)
            .children(self.find_bar(cx))
            .child(
                canvas(|_, _, _| {}, {
                    let focus = self.focus.clone();
                    let entity = cx.entity();
                    move |bounds, _, window, cx| {
                        window.handle_input(&focus, ElementInputHandler::new(bounds, entity), cx);
                    }
                })
                .absolute()
                .size_0(),
            )
    }
}

impl Editor {
    fn delete_with(&mut self, target: impl Fn(&Editor, Pos) -> Pos, cx: &mut Context<Self>) {
        if !self.is_editable() || self.rows.is_empty() {
            return;
        }
        self.snapshot(self.anchor.is_none());
        if self.selection().is_none() {
            let to = target(self, self.cursor);
            if to == self.cursor {
                return;
            }
            self.anchor = Some(to);
        }
        if self.delete_selection() {
            self.after_edit(cx);
        } else {
            cx.notify();
        }
    }

    fn history(&mut self, undo: bool, cx: &mut Context<Self>) {
        let snapshot = if undo { self.undo.pop() } else { self.redo.pop() };
        let Some(snapshot) = snapshot else { return };
        let current = Snapshot {
            rows: std::mem::replace(&mut self.rows, snapshot.rows),
            cursor: self.cursor,
            anchor: self.anchor,
        };
        if undo {
            self.redo.push(current)
        } else {
            self.undo.push(current)
        }
        self.cursor = snapshot.cursor;
        self.anchor = snapshot.anchor;
        self.last_edit = None;
        self.after_edit(cx);
    }
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text = &self.rows.get(self.cursor.row)?.text;
        let units: Vec<u16> = text.encode_utf16().collect();
        let end = range.end.min(units.len());
        let start = range.start.min(end);
        actual.replace(start..end);
        Some(String::from_utf16_lossy(&units[start..end]))
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        let text = &self.rows.get(self.cursor.row)?.text;
        let offset = text[..self.cursor.col.min(text.len())].encode_utf16().count();
        Some(UTF16Selection { range: offset..offset, reversed: false })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let text = self.rows.get(self.cursor.row).map(|row| &row.text)?;
        let offset = text[..self.cursor.col.min(text.len())].encode_utf16().count();
        self.marked.as_ref().map(|marked| offset..offset + marked.encode_utf16().count())
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        self.insert(text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = Some(text.to_string()).filter(|text| !text.is_empty());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let (_, cell) = cell_size(window, cx);
        let row = self.rows.get(self.cursor.row)?;
        let x = text_width(&row.text[..self.cursor.col.min(row.text.len())]);
        Some(Bounds::new(point(self.code_origin.x + cell.width * x, self.code_origin.y), size(cell.width, cell.height)))
    }

    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(items: &[(LineKind, &str)]) -> Vec<Row> {
        items.iter().map(|(kind, text)| Row { kind: *kind, text: text.to_string(), old: None, new: None }).collect()
    }

    #[test]
    fn saved_text_skips_removed_lines() {
        let rows = rows(&[(LineKind::Context, "a"), (LineKind::Removed, "old"), (LineKind::Added, "new")]);
        assert_eq!(file_text(&rows, true), "a\nnew\n");
        assert_eq!(file_text(&rows, false), "a\nnew");
    }

    #[test]
    fn highlights_map_to_rows() {
        let rows = rows(&[
            (LineKind::Context, "fn main() {}"),
            (LineKind::Removed, "let old = 1;"),
            (LineKind::Added, "let x = 2;"),
        ]);
        let tokens = highlights(Some("Rust"), &rows);
        assert_eq!(tokens.len(), 3);
        assert!(tokens[0].iter().any(|(range, token)| *range == (0..2) && *token == Token::Keyword));
        assert!(tokens[1].iter().any(|(range, token)| *range == (0..3) && *token == Token::Keyword));
        assert!(tokens[2].iter().any(|(range, token)| *range == (0..3) && *token == Token::Keyword));
    }

    #[test]
    fn widths() {
        assert_eq!(text_width("ab"), 2.0);
        assert_eq!(text_width("你a"), 3.0);
    }
}

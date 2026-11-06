use fancy_regex::{Captures, Regex};
use serde::{Deserialize, Serialize};

use crate::editor::{Pos, Row};
use crate::git::LineKind;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub case: bool,
    pub word: bool,
    pub regex: bool,
    pub preserve: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub start: Pos,
    pub end: Pos,
}

pub struct Query {
    regex: Regex,
    multiline: bool,
}

impl Query {
    pub fn new(text: &str, options: Options) -> Result<Query, String> {
        let body = if options.regex { text.to_string() } else { fancy_regex::escape(text).into_owned() };
        let body = if options.word { format!(r"\b(?:{body})\b") } else { body };
        let flags = if options.case { "(?m)" } else { "(?mi)" };
        let regex = Regex::new(&format!("{flags}{body}")).map_err(|error| error.to_string())?;
        Ok(Query { regex, multiline: options.regex && text.contains("\\n") })
    }

    pub fn walk(&self, rows: &[Row], mut visit: impl FnMut(Hit, &Captures<'_, str>)) {
        if self.multiline {
            self.walk_joined(rows, &mut visit);
            return;
        }
        for (index, row) in rows.iter().enumerate() {
            for captures in self.regex.captures_iter(row.text.as_str()).map_while(Result::ok) {
                let Some(found) = captures.get(0) else { continue };
                let start = Pos { row: index, col: found.start() };
                let end = Pos { row: index, col: found.end() };
                visit(Hit { start, end }, &captures);
            }
        }
    }

    fn walk_joined(&self, rows: &[Row], visit: &mut impl FnMut(Hit, &Captures<'_, str>)) {
        let mut text = String::new();
        let mut starts = Vec::new();
        for (index, row) in rows.iter().enumerate().filter(|(_, row)| row.kind != LineKind::Removed) {
            starts.push((text.len(), index));
            text.push_str(&row.text);
            text.push('\n');
        }
        text.pop();
        let position = |offset: usize| {
            let slot = starts.partition_point(|(start, _)| *start <= offset).saturating_sub(1);
            let (start, row) = starts[slot];
            Pos { row, col: offset - start }
        };
        for captures in self.regex.captures_iter(text.as_str()).map_while(Result::ok) {
            let Some(found) = captures.get(0) else { continue };
            visit(Hit { start: position(found.start()), end: position(found.end()) }, &captures);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Change {
    Upper,
    Lower,
}

fn changed(text: &str, change: Change) -> String {
    match change {
        Change::Upper => text.to_uppercase(),
        Change::Lower => text.to_lowercase(),
    }
}

#[derive(Default)]
struct Output {
    text: String,
    next: Option<Change>,
    span: Option<Change>,
}

impl Output {
    fn push(&mut self, piece: &str) {
        let spanned = match self.span {
            Some(change) => changed(piece, change),
            None => piece.to_string(),
        };
        let mut chars = spanned.chars();
        match (self.next.take(), chars.next()) {
            (Some(change), Some(first)) => {
                self.text.push_str(&changed(&first.to_string(), change));
                self.text.push_str(chars.as_str());
            }
            (change, _) => {
                self.next = change;
                self.text.push_str(&spanned);
            }
        }
    }
}

fn group_text<'a>(captures: &'a Captures<'_, str>, index: usize) -> &'a str {
    captures.get(index).map_or("", |group| group.as_str())
}

fn numbered_group(captures: &Captures<'_, str>, mut index: usize) -> String {
    if index == 0 {
        return group_text(captures, 0).to_string();
    }
    let mut remainder = String::new();
    while index > 0 {
        if index < captures.len() {
            return format!("{}{remainder}", group_text(captures, index));
        }
        remainder.insert_str(0, &(index % 10).to_string());
        index /= 10;
    }
    format!("${remainder}")
}

fn expand_template(template: &str, captures: &Captures<'_, str>) -> String {
    let mut output = Output::default();
    let mut rest = template;
    while let Some(first) = rest.chars().next() {
        let after = &rest[first.len_utf8()..];
        if first == '$' {
            if let Some(tail) = after.strip_prefix('$') {
                output.push("$");
                rest = tail;
                continue;
            }
            if let Some(tail) = after.strip_prefix('&') {
                output.push(group_text(captures, 0));
                rest = tail;
                continue;
            }
            let digits = after.chars().take_while(char::is_ascii_digit).take(2).count();
            if digits > 0 {
                let index = after[..digits].parse::<usize>().unwrap_or(0);
                output.push(&numbered_group(captures, index));
                rest = &after[digits..];
                continue;
            }
            if let Some((name, tail)) = after.strip_prefix('{').and_then(|braced| braced.split_once('}')) {
                let group = name.parse::<usize>().ok().and_then(|index| captures.get(index));
                let group = group.or_else(|| captures.name(name));
                output.push(group.map_or("", |group| group.as_str()));
                rest = tail;
                continue;
            }
        }
        if first == '\\'
            && let Some(code) = after.chars().next()
        {
            let tail = &after[code.len_utf8()..];
            let handled = match code {
                'n' => {
                    output.push("\n");
                    true
                }
                't' => {
                    output.push("\t");
                    true
                }
                '\\' => {
                    output.push("\\");
                    true
                }
                'u' => {
                    output.next = Some(Change::Upper);
                    true
                }
                'l' => {
                    output.next = Some(Change::Lower);
                    true
                }
                'U' => {
                    output.span = Some(Change::Upper);
                    true
                }
                'L' => {
                    output.span = Some(Change::Lower);
                    true
                }
                'E' => {
                    output.span = None;
                    true
                }
                _ => false,
            };
            if handled {
                rest = tail;
                continue;
            }
        }
        output.push(&rest[..first.len_utf8()]);
        rest = after;
    }
    output.text
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn uncapitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn preserve_case(matched: &str, text: &str) -> String {
    let has_letters = matched.chars().any(char::is_alphabetic);
    if !has_letters || text.is_empty() {
        return text.to_string();
    }
    if matched.chars().all(|c| !c.is_lowercase()) && matched.chars().count() > 1 {
        return text.to_uppercase();
    }
    if matched.chars().all(|c| !c.is_uppercase()) {
        return text.to_lowercase();
    }
    for separator in ['-', '_'] {
        let parts: Vec<&str> = matched.split(separator).collect();
        let pieces: Vec<&str> = text.split(separator).collect();
        if parts.len() > 1 && parts.len() == pieces.len() {
            let joined: Vec<String> =
                parts.iter().zip(&pieces).map(|(part, piece)| preserve_case(part, piece)).collect();
            return joined.join(&separator.to_string());
        }
    }
    if matched.chars().next().is_some_and(char::is_uppercase) { capitalized(text) } else { uncapitalized(text) }
}

pub fn expand(template: &str, captures: &Captures<'_, str>, options: Options) -> String {
    let matched = group_text(captures, 0);
    let text = if options.regex { expand_template(template, captures) } else { template.to_string() };
    if options.preserve { preserve_case(matched, &text) } else { text }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(lines: &[(LineKind, &str)]) -> Vec<Row> {
        lines.iter().map(|(kind, text)| Row { kind: *kind, text: text.to_string(), old: None, new: None }).collect()
    }

    fn hits(rows: &[Row], text: &str, options: Options) -> Vec<Hit> {
        let mut found = Vec::new();
        Query::new(text, options).unwrap().walk(rows, |hit, _| found.push(hit));
        found
    }

    fn replaced(line: &str, text: &str, template: &str, options: Options) -> Vec<String> {
        let rows = rows(&[(LineKind::Context, line)]);
        let mut found = Vec::new();
        Query::new(text, options).unwrap().walk(&rows, |_, captures| found.push(expand(template, captures, options)));
        found
    }

    fn pos(row: usize, col: usize) -> Pos {
        Pos { row, col }
    }

    #[test]
    fn plain_text_ignores_case_by_default() {
        let rows = rows(&[(LineKind::Context, "Foo foo a.b"), (LineKind::Removed, "FOO")]);
        let found = hits(&rows, "foo", Options::default());
        assert_eq!(found.len(), 3);
        assert_eq!(found[1], Hit { start: pos(0, 4), end: pos(0, 7) });
        assert_eq!(found[2].start.row, 1);
        assert_eq!(hits(&rows, "a.b", Options::default()).len(), 1);
        assert_eq!(hits(&rows, "foo", Options { case: true, ..Default::default() }).len(), 1);
    }

    #[test]
    fn whole_word() {
        let rows = rows(&[(LineKind::Context, "cat concat cat_x cat")]);
        assert_eq!(hits(&rows, "cat", Options { word: true, ..Default::default() }).len(), 2);
    }

    #[test]
    fn regex_lookaround_backreference_and_invalid() {
        let options = Options { regex: true, ..Default::default() };
        let rows = rows(&[(LineKind::Context, "foobar foobaz abab")]);
        assert_eq!(hits(&rows, r"foo(?=bar)", options).len(), 1);
        assert_eq!(hits(&rows, r"(ab)\1", options).len(), 1);
        assert!(Query::new("(", options).is_err());
    }

    #[test]
    fn multiline_regex_spans_rows_and_skips_removed() {
        let options = Options { regex: true, ..Default::default() };
        let rows = rows(&[(LineKind::Context, "one"), (LineKind::Removed, "old"), (LineKind::Added, "two")]);
        assert_eq!(hits(&rows, r"e\ntw", options), vec![Hit { start: pos(0, 2), end: pos(2, 2) }]);
    }

    #[test]
    fn line_anchors_match_each_row() {
        let options = Options { regex: true, ..Default::default() };
        let rows = rows(&[(LineKind::Context, "a"), (LineKind::Context, "b")]);
        assert_eq!(hits(&rows, "^", options).len(), 2);
    }

    #[test]
    fn expands_groups_and_escapes() {
        let options = Options { regex: true, ..Default::default() };
        assert_eq!(replaced("ab", "(a)(b)", "$2$1", options), ["ba"]);
        assert_eq!(replaced("ab", "(a)(b)", "$& $0 $$", options), ["ab ab $"]);
        assert_eq!(replaced("ab", "(?<first>a)b", "${first}!", options), ["a!"]);
        assert_eq!(replaced("ab", "(a)b", "$1\\n\\t\\\\", options), ["a\n\t\\"]);
        assert_eq!(replaced("ab", "(a)b", "$12", options), ["a2"]);
        assert_eq!(replaced("ab", "(a)b", "$9", options), ["$9"]);
    }

    #[test]
    fn changes_case_in_templates() {
        let options = Options { regex: true, ..Default::default() };
        assert_eq!(replaced("foo bar", "(\\w+) (\\w+)", "\\u$1 \\U$2\\E!", options), ["Foo BAR!"]);
        assert_eq!(replaced("FOO", "(\\w+)", "\\L$1", options), ["foo"]);
        assert_eq!(replaced("FOO", "(\\w+)", "\\l$1", options), ["fOO"]);
    }

    #[test]
    fn literal_mode_keeps_template() {
        assert_eq!(replaced("ab", "a", "$1\\n", Options::default()), ["$1\\n"]);
    }

    #[test]
    fn preserves_case() {
        assert_eq!(preserve_case("FOO", "bar"), "BAR");
        assert_eq!(preserve_case("foo", "BAR"), "bar");
        assert_eq!(preserve_case("Foo", "bar"), "Bar");
        assert_eq!(preserve_case("fOo", "Bar"), "bar");
        assert_eq!(preserve_case("Foo-Bar", "one-two"), "One-Two");
        assert_eq!(preserve_case("FOO_bar", "one_two"), "ONE_two");
        assert_eq!(preserve_case("123", "x"), "x");
        assert_eq!(preserve_case("A", "bar"), "Bar");
    }
}

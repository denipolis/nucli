use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Url(String),
    File(PathBuf, Option<usize>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub line: i32,
    pub start: usize,
    pub end: usize,
    pub target: Target,
}

fn is_boundary(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>' | '|' | '\0')
}

pub fn token_at(line: &[char], column: usize) -> Option<(usize, usize, String)> {
    if column >= line.len() || is_boundary(line[column]) {
        return None;
    }
    let mut start = column;
    while start > 0 && !is_boundary(line[start - 1]) {
        start -= 1;
    }
    let mut end = column;
    while end + 1 < line.len() && !is_boundary(line[end + 1]) {
        end += 1;
    }
    while start < end && matches!(line[start], '(' | '[' | '{') {
        start += 1;
    }
    while end > start && matches!(line[end], ')' | ']' | '}' | '.' | ',' | ';' | ':' | '!' | '?') {
        end -= 1;
    }
    if column < start || column > end {
        return None;
    }
    Some((start, end, line[start..=end].iter().collect()))
}

fn split_position(token: &str) -> (&str, Option<usize>) {
    let mut path = token;
    let mut numbers = Vec::new();
    while let Some((head, tail)) = path.rsplit_once(':') {
        match tail.parse::<usize>() {
            Ok(number) if numbers.len() < 2 => {
                numbers.push(number);
                path = head;
            }
            _ => break,
        }
    }
    (path, numbers.last().copied())
}

pub fn classify(token: &str, cwd: &Path, project: &Path) -> Option<Target> {
    for scheme in ["http://", "https://", "ftp://", "mailto:"] {
        if token.starts_with(scheme) {
            return Some(Target::Url(token.to_string()));
        }
    }
    let token = token.strip_prefix("file://").unwrap_or(token);
    let (path, line) = split_position(token);
    if path.is_empty() {
        return None;
    }
    let path = path
        .strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .filter(|rest| !cwd.join(path).exists() && cwd.join(rest).exists())
        .unwrap_or(path);
    let expanded = crate::paths::expand(path);
    let candidates =
        if expanded.is_absolute() { vec![expanded] } else { vec![cwd.join(&expanded), project.join(&expanded)] };
    candidates.into_iter().find(|candidate| candidate.is_file()).map(|file| Target::File(file, line))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    #[test]
    fn finds_tokens() {
        let line = chars("see (https://a.io/x). next");
        assert_eq!(token_at(&line, 8).map(|token| token.2), Some("https://a.io/x".to_string()));
        assert_eq!(token_at(&line, 3), None);
        assert_eq!(token_at(&line, 22).map(|token| token.2), Some("next".to_string()));
        assert_eq!(
            token_at(&chars("at src/main.rs:12:4,"), 5).map(|token| token.2),
            Some("src/main.rs:12:4".to_string())
        );
    }

    #[test]
    fn classifies_targets() {
        let dir = std::env::temp_dir().join("nucli-links-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.rs"), "x").unwrap();
        let cwd = dir.as_path();
        let other = Path::new("/nonexistent");
        assert_eq!(classify("https://a.io", cwd, other), Some(Target::Url("https://a.io".into())));
        assert_eq!(classify("a.rs:12:4", cwd, other), Some(Target::File(dir.join("a.rs"), Some(12))));
        assert_eq!(classify("a.rs", other, cwd), Some(Target::File(dir.join("a.rs"), None)));
        assert_eq!(classify("missing.rs", cwd, other), None);
    }
}

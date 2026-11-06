use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const WHOLE_FILE: &str = "-U100000000";
const MAX_TEXT_SIZE: u64 = 4000000;
const COLORS: usize = 8;

#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub path: String,
    pub status: char,
    pub staged: bool,
    pub added: usize,
    pub removed: usize,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Status {
    pub branch: String,
    pub changes: Vec<Change>,
    pub files: usize,
    pub added: usize,
    pub removed: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiffStat {
    pub files: usize,
    pub added: usize,
    pub removed: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiffLine {
    pub kind: LineKind,
    pub text: String,
    pub old: Option<usize>,
    pub new: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Commit {
    pub hash: String,
    pub parents: Vec<String>,
    pub refs: Vec<String>,
    pub author: String,
    pub time: i64,
    pub subject: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub from: usize,
    pub to: usize,
    pub upper: bool,
    pub color: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphRow {
    pub column: usize,
    pub color: usize,
    pub segments: Vec<Segment>,
    pub width: usize,
}

fn output(dir: &Path, args: &[&str]) -> Option<Output> {
    let mut command = Command::new("git");
    command
        .args(["-c", "core.quotepath=off", "--no-pager"])
        .args(args)
        .current_dir(dir)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.output().ok()
}

pub fn run(dir: &Path, args: &[&str]) -> Option<String> {
    let output = output(dir, args)?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn run_checked(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = output(dir, args).ok_or("git failed")?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    [&output.stderr, &output.stdout]
        .into_iter()
        .map(|bytes| String::from_utf8_lossy(bytes).trim().to_string())
        .find(|message| !message.is_empty())
        .map_or_else(|| Err("git failed".to_string()), Err)
}

fn nul_split(output: &str) -> Vec<String> {
    output.split('\0').filter(|item| !item.is_empty()).map(String::from).collect()
}

fn has_head(dir: &Path) -> bool {
    run(dir, &["rev-parse", "--verify", "-q", "HEAD"]).is_some()
}

fn base(dir: &Path) -> &'static str {
    if has_head(dir) { "HEAD" } else { EMPTY_TREE }
}

fn parent(dir: &Path, hash: &str) -> String {
    run(dir, &["rev-parse", "--verify", "-q", &format!("{hash}^")])
        .map_or_else(|| EMPTY_TREE.to_string(), |output| output.trim().to_string())
}

fn numstat(token: &str) -> Option<(usize, usize, &str)> {
    let mut parts = token.splitn(3, '\t');
    let added = parts.next()?.parse().unwrap_or(0);
    let removed = parts.next()?.parse().unwrap_or(0);
    Some((added, removed, parts.next()?))
}

fn parse_changes(output: &str, staged: bool) -> Vec<Change> {
    let mut changes = Vec::new();
    let mut stats = HashMap::new();
    let mut tokens = output.split('\0').filter(|token| !token.is_empty());
    while let Some(token) = tokens.next() {
        if token.starts_with(':') {
            let status = token.chars().last().filter(|letter| matches!(letter, 'A' | 'D')).unwrap_or('M');
            if let Some(path) = tokens.next() {
                changes.push(Change { path: path.to_string(), status, staged, added: 0, removed: 0 });
            }
        } else if let Some((added, removed, path)) = numstat(token) {
            stats.insert(path, (added, removed));
        }
    }
    for change in &mut changes {
        if let Some(&(added, removed)) = stats.get(change.path.as_str()) {
            change.added = added;
            change.removed = removed;
        }
    }
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    changes
}

fn changes(dir: &Path, revisions: &[&str], staged: bool) -> Option<Vec<Change>> {
    let mut args = vec!["diff"];
    args.extend(revisions);
    args.extend(["--relative", "--no-renames", "--raw", "--numstat", "-z", "--"]);
    Some(parse_changes(&run(dir, &args)?, staged))
}

fn read_text(path: &Path) -> Option<String> {
    if fs::metadata(path).ok()?.len() > MAX_TEXT_SIZE {
        return None;
    }
    let text = String::from_utf8_lossy(&fs::read(path).ok()?).into_owned();
    (!text.contains('\0')).then_some(text)
}

fn untracked(dir: &Path) -> Vec<(String, usize)> {
    let output = run(dir, &["ls-files", "--others", "--exclude-standard", "-z"]).unwrap_or_default();
    nul_split(&output)
        .into_iter()
        .map(|path| {
            let lines = read_text(&dir.join(&path)).map_or(0, |text| text.lines().count());
            (path, lines)
        })
        .collect()
}

fn totals(dir: &Path, base: &str, untracked: &[(String, usize)]) -> DiffStat {
    let output = run(dir, &["diff", base, "--relative", "--no-renames", "--numstat", "-z", "--"]).unwrap_or_default();
    let stats: Vec<_> = output.split('\0').filter_map(numstat).collect();
    DiffStat {
        files: stats.len() + untracked.len(),
        added: stats.iter().map(|stat| stat.0).sum::<usize>() + untracked.iter().map(|file| file.1).sum::<usize>(),
        removed: stats.iter().map(|stat| stat.1).sum(),
    }
}

fn branch(dir: &Path) -> String {
    run(dir, &["symbolic-ref", "--short", "-q", "HEAD"])
        .or_else(|| run(dir, &["rev-parse", "--verify", "-q", "--short", "HEAD"]))
        .map(|output| output.trim().to_string())
        .unwrap_or_default()
}

pub fn is_repo(dir: &Path) -> bool {
    run(dir, &["rev-parse", "--is-inside-work-tree"]).is_some_and(|output| output.trim() == "true")
}

pub fn status(dir: &Path) -> Option<Status> {
    if !is_repo(dir) {
        return None;
    }
    let base = base(dir);
    let untracked = untracked(dir);
    let mut changes_list = changes(dir, &["--cached", base], true).unwrap_or_default();
    changes_list.extend(changes(dir, &[], false).unwrap_or_default());
    changes_list.extend(untracked.iter().map(|(path, lines)| Change {
        path: path.clone(),
        status: 'U',
        staged: false,
        added: *lines,
        removed: 0,
    }));
    changes_list.sort_by(|a, b| b.staged.cmp(&a.staged).then_with(|| a.path.cmp(&b.path)));
    let DiffStat { files, added, removed } = totals(dir, base, &untracked);
    Some(Status { branch: branch(dir), changes: changes_list, files, added, removed })
}

pub fn diff_stat(dir: &Path) -> Option<DiffStat> {
    is_repo(dir).then(|| totals(dir, base(dir), &untracked(dir)))
}

fn hunk_range(range: &str) -> Option<(usize, usize)> {
    let (start, count) = range.split_once(',').unwrap_or((range, "1"));
    Some((start.parse().ok()?, count.parse().ok()?))
}

fn parse_diff(diff: &str) -> Option<Vec<DiffLine>> {
    if diff.contains('\0') {
        return None;
    }
    let mut lines = Vec::new();
    let (mut old, mut new, mut old_left, mut new_left) = (0, 0, 0, 0);
    for line in diff.lines() {
        if old_left == 0 && new_left == 0 {
            if line.starts_with("Binary files ") {
                return None;
            }
            if let Some(header) = line.strip_prefix("@@ -") {
                let mut ranges = header.split(' ');
                (old, old_left) = hunk_range(ranges.next()?)?;
                (new, new_left) = hunk_range(ranges.next()?.strip_prefix('+')?)?;
            }
            continue;
        }
        let text = line.get(1..).unwrap_or("").to_string();
        match line.as_bytes().first() {
            Some(b'+') => {
                lines.push(DiffLine { kind: LineKind::Added, text, old: None, new: Some(new) });
                new += 1;
                new_left = new_left.saturating_sub(1);
            }
            Some(b'-') => {
                lines.push(DiffLine { kind: LineKind::Removed, text, old: Some(old), new: None });
                old += 1;
                old_left = old_left.saturating_sub(1);
            }
            Some(b'\\') => {}
            _ => {
                lines.push(DiffLine { kind: LineKind::Context, text, old: Some(old), new: Some(new) });
                old += 1;
                new += 1;
                old_left = old_left.saturating_sub(1);
                new_left = new_left.saturating_sub(1);
            }
        }
    }
    Some(lines)
}

fn text_lines(text: &str, kind: LineKind) -> Vec<DiffLine> {
    text.lines()
        .enumerate()
        .map(|(index, line)| DiffLine {
            kind,
            text: line.to_string(),
            old: (kind == LineKind::Context).then_some(index + 1),
            new: Some(index + 1),
        })
        .collect()
}

fn is_untracked(dir: &Path, path: &str) -> bool {
    run(dir, &["--literal-pathspecs", "ls-files", "--others", "--exclude-standard", "-z", "--", path])
        .is_some_and(|output| !output.is_empty())
}

pub fn file_diff(dir: &Path, path: &str, commit: Option<&str>) -> Option<Vec<DiffLine>> {
    let revisions = match commit {
        Some(hash) => vec![parent(dir, hash), hash.to_string()],
        None => vec![base(dir).to_string()],
    };
    let mut args = vec!["--literal-pathspecs", "diff"];
    args.extend(revisions.iter().map(String::as_str));
    args.extend(["--no-color", "--no-ext-diff", "--no-renames", WHOLE_FILE, "--", path]);
    let diff = run(dir, &args)?;
    if !diff.is_empty() {
        return parse_diff(&diff);
    }
    let (text, kind) = match commit {
        Some(hash) => (run(dir, &["show", &format!("{hash}:./{path}")])?, LineKind::Context),
        None if is_untracked(dir, path) => (read_text(&dir.join(path))?, LineKind::Added),
        None => (read_text(&dir.join(path))?, LineKind::Context),
    };
    (!text.contains('\0')).then(|| text_lines(&text, kind))
}

fn run_paths(dir: &Path, args: &[&str], paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut all = vec!["--literal-pathspecs"];
    all.extend(args);
    all.push("--");
    all.extend(paths.iter().map(String::as_str));
    run_checked(dir, &all).map(drop)
}

pub fn stage(dir: &Path, paths: &[String]) -> Result<(), String> {
    run_paths(dir, &["add"], paths)
}

pub fn unstage(dir: &Path, paths: &[String]) -> Result<(), String> {
    if has_head(dir) {
        run_paths(dir, &["restore", "--staged"], paths)
    } else {
        run_paths(dir, &["rm", "--cached", "-r", "-q"], paths)
    }
}

pub fn discard(dir: &Path, paths: &[String]) -> Result<(), String> {
    run_paths(dir, &["restore", "--worktree"], paths)
}

pub fn commit(dir: &Path, message: &str) -> Result<(), String> {
    run_checked(dir, &["commit", "-m", message]).map(drop)
}

fn parse_commit(line: &str) -> Option<Commit> {
    let mut fields = line.splitn(6, '\x1f');
    Some(Commit {
        hash: fields.next()?.to_string(),
        parents: fields.next()?.split_whitespace().map(String::from).collect(),
        refs: fields.next()?.split(", ").filter(|name| !name.is_empty()).map(String::from).collect(),
        author: fields.next()?.to_string(),
        time: fields.next()?.parse().unwrap_or(0),
        subject: fields.next()?.to_string(),
    })
}

pub fn log(dir: &Path, limit: usize) -> Option<Vec<Commit>> {
    let limit = limit.to_string();
    let format = "--format=%H%x1f%P%x1f%D%x1f%an%x1f%at%x1f%s";
    let args = ["log", "--branches", "--remotes", "--tags", "HEAD", "--topo-order", "-n", limit.as_str(), format, "--"];
    match run(dir, &args) {
        Some(output) => Some(output.lines().filter_map(parse_commit).collect()),
        None => is_repo(dir).then(Vec::new),
    }
}

fn free_lane(lanes: &mut Vec<Option<(&str, usize)>>) -> usize {
    lanes.iter().position(Option::is_none).unwrap_or_else(|| {
        lanes.push(None);
        lanes.len() - 1
    })
}

fn waiting(lanes: &[Option<(&str, usize)>], hash: &str) -> Option<usize> {
    lanes.iter().position(|lane| lane.is_some_and(|(waiting, _)| waiting == hash))
}

fn next_color(colors: &mut usize) -> usize {
    *colors += 1;
    (*colors - 1) % COLORS
}

pub fn graph(commits: &[Commit]) -> Vec<GraphRow> {
    let mut lanes: Vec<Option<(&str, usize)>> = Vec::new();
    let mut colors = 0;
    let mut rows = Vec::new();
    for commit in commits {
        let found = waiting(&lanes, &commit.hash);
        let column = found.unwrap_or_else(|| free_lane(&mut lanes));
        let has_branch = commit.refs.iter().any(|reference| !reference.starts_with("tag: "));
        let color = match found {
            Some(_) if has_branch => next_color(&mut colors),
            Some(index) => lanes[index].map_or(0, |(_, color)| color),
            None => next_color(&mut colors),
        };
        let mut segments = Vec::new();
        for (index, lane) in lanes.iter_mut().enumerate() {
            let Some((hash, lane_color)) = *lane else { continue };
            if hash == commit.hash {
                segments.push(Segment { from: index, to: column, upper: true, color: lane_color });
                *lane = None;
            } else {
                segments.push(Segment { from: index, to: index, upper: true, color: lane_color });
                segments.push(Segment { from: index, to: index, upper: false, color: lane_color });
            }
        }
        for (position, parent) in commit.parents.iter().enumerate() {
            let target = match (position, waiting(&lanes, parent)) {
                (0, _) => column,
                (_, Some(index)) => index,
                _ => free_lane(&mut lanes),
            };
            let lane_color = match lanes[target] {
                Some((_, lane_color)) => lane_color,
                None if target == column => color,
                None => next_color(&mut colors),
            };
            lanes[target] = Some((parent.as_str(), lane_color));
            segments.push(Segment { from: column, to: target, upper: false, color: lane_color });
        }
        while lanes.last().is_some_and(Option::is_none) {
            lanes.pop();
        }
        let width = segments.iter().map(|segment| segment.from.max(segment.to)).fold(column, usize::max) + 1;
        rows.push(GraphRow { column, color, segments, width });
    }
    rows
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommitDetails {
    pub date: String,
    pub message: String,
    pub stats: String,
}

fn parse_details(output: &str) -> Option<CommitDetails> {
    let mut parts = output.splitn(3, '\0');
    Some(CommitDetails {
        date: parts.next()?.trim().to_string(),
        message: parts.next()?.trim().to_string(),
        stats: parts.next().unwrap_or_default().trim().to_string(),
    })
}

pub fn commit_details(dir: &Path, hash: &str) -> Option<CommitDetails> {
    let args = [
        "show",
        "-m",
        "--first-parent",
        "--shortstat",
        "--date=format-local:%B %d, %Y at %I:%M %p",
        "--format=%ad%x00%B%x00",
        hash,
    ];
    parse_details(&run(dir, &args)?)
}

pub fn commit_changes(dir: &Path, hash: &str) -> Option<Vec<Change>> {
    changes(dir, &[parent(dir, hash).as_str(), hash], false)
}

pub fn branches(dir: &Path, limit: usize) -> Vec<String> {
    let count = format!("--count={limit}");
    let args = ["for-each-ref", "--sort=-committerdate", count.as_str(), "--format=%(refname:lstrip=2)", "refs/heads"];
    run(dir, &args).map(|output| output.lines().map(String::from).collect()).unwrap_or_default()
}

pub fn switch(dir: &Path, branch: &str) -> Result<(), String> {
    run_checked(dir, &["switch", branch]).map(drop)
}

pub fn create_branch(dir: &Path, name: &str) -> Result<(), String> {
    run_checked(dir, &["switch", "-c", name]).map(drop)
}

pub fn current_branch(path: &Path) -> Option<String> {
    let dot_git = path.ancestors().map(|folder| folder.join(".git")).find(|candidate| candidate.exists())?;
    let git_dir = if dot_git.is_file() {
        let content = fs::read_to_string(&dot_git).ok()?;
        let target = content.lines().find_map(|line| line.strip_prefix("gitdir:"))?.trim();
        dot_git.parent()?.join(target)
    } else {
        dot_git
    };
    let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    let name = match head.strip_prefix("ref:") {
        Some(reference) => reference.trim().strip_prefix("refs/heads/").unwrap_or(reference.trim()).to_string(),
        None => head.chars().take(7).collect(),
    };
    (!name.is_empty() && name != ".invalid").then_some(name)
}

pub fn files(dir: &Path) -> Option<Vec<String>> {
    let mut files = nul_split(&run(dir, &["ls-files", "--cached", "--others", "--exclude-standard", "-z"])?);
    files.sort();
    files.dedup();
    Some(files)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TreeStatus {
    pub changes: Vec<(String, char, bool)>,
    pub ignored: Vec<String>,
}

fn parse_tree_status(output: &str, prefix: &str) -> TreeStatus {
    let mut tree = TreeStatus::default();
    for entry in output.split('\0') {
        let (Some(code), Some(path)) = (entry.get(..2), entry.get(3..)) else { continue };
        let Some(path) = path.strip_prefix(prefix).filter(|path| !path.is_empty()) else { continue };
        let mut letters = code.chars();
        let (index, worktree) = (letters.next().unwrap_or(' '), letters.next().unwrap_or(' '));
        match (index, worktree) {
            ('?', _) => tree.changes.push((path.to_string(), 'U', false)),
            ('U', _) | (_, 'U') => tree.changes.push((path.to_string(), 'M', false)),
            _ => {
                if index != ' ' {
                    tree.changes.push((path.to_string(), index, true));
                }
                if worktree != ' ' {
                    tree.changes.push((path.to_string(), worktree, false));
                }
            }
        }
    }
    tree
}

pub fn tree_status(dir: &Path) -> Option<TreeStatus> {
    let prefix = run(dir, &["rev-parse", "--show-prefix"])?;
    let args = ["status", "--porcelain=v1", "-z", "--no-renames", "--untracked-files=all", "--", "."];
    let mut tree = parse_tree_status(&run(dir, &args)?, prefix.trim_end_matches('\n'));
    let ignored = ["ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "-z"];
    tree.ignored = run(dir, &ignored)
        .map(|output| nul_split(&output))
        .unwrap_or_default()
        .into_iter()
        .map(|path| path.trim_end_matches('/').to_string())
        .collect();
    Some(tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tree_status() {
        let output = " M sub/a.txt\0M  sub/b.txt\0MM sub/c.txt\0?? sub/new.txt\0UU sub/d.txt\0 M other.txt\0";
        let tree = parse_tree_status(output, "sub/");
        assert_eq!(
            tree.changes,
            vec![
                ("a.txt".to_string(), 'M', false),
                ("b.txt".to_string(), 'M', true),
                ("c.txt".to_string(), 'M', true),
                ("c.txt".to_string(), 'M', false),
                ("new.txt".to_string(), 'U', false),
                ("d.txt".to_string(), 'M', false),
            ]
        );
    }

    #[test]
    fn parses_commit_details() {
        let details =
            parse_details("June 01, 2026 at 09:30 AM\0feat: a\n\nbody\n\0\n 2 files changed, 3 insertions(+)\n");
        let details = details.unwrap();
        assert_eq!(details.date, "June 01, 2026 at 09:30 AM");
        assert_eq!(details.message, "feat: a\n\nbody");
        assert_eq!(details.stats, "2 files changed, 3 insertions(+)");
    }

    fn node(hash: &str, parents: &[&str]) -> Commit {
        Commit {
            hash: hash.to_string(),
            parents: parents.iter().map(|parent| parent.to_string()).collect(),
            refs: Vec::new(),
            author: String::new(),
            time: 0,
            subject: String::new(),
        }
    }

    fn segment(from: usize, to: usize, upper: bool, color: usize) -> Segment {
        Segment { from, to, upper, color }
    }

    fn line(kind: LineKind, text: &str, old: Option<usize>, new: Option<usize>) -> DiffLine {
        DiffLine { kind, text: text.to_string(), old, new }
    }

    #[test]
    fn parses_raw_and_numstat() {
        let output = concat!(
            ":100644 100644 abc def M\0x.txt\0:000000 100644 000 abc A\0new\tfile\0",
            ":100644 000000 abc 000 D\0gone\0:100644 100644 abc def T\0link\0",
            "2\t1\tx.txt\0",
            "3\t0\tnew\tfile\0-\t-\tgone\0",
        );
        let changes = parse_changes(output, true);
        let summary: Vec<_> = changes.iter().map(|c| (c.path.as_str(), c.status, c.added, c.removed)).collect();
        assert_eq!(
            summary,
            vec![("gone", 'D', 0, 0), ("link", 'M', 0, 0), ("new\tfile", 'A', 3, 0), ("x.txt", 'M', 2, 1)]
        );
        assert!(changes.iter().all(|change| change.staged));
    }

    #[test]
    fn parses_whole_file_diff() {
        let diff = concat!(
            "diff --git a/x b/x\nindex 1..2 100644\n--- a/x\n+++ b/x\n",
            "@@ -1,3 +1,3 @@\n a\n--- dashes\n+c\r\n d\n\\ No newline at end of file\n",
        );
        assert_eq!(
            parse_diff(diff),
            Some(vec![
                line(LineKind::Context, "a", Some(1), Some(1)),
                line(LineKind::Removed, "-- dashes", Some(2), None),
                line(LineKind::Added, "c", None, Some(2)),
                line(LineKind::Context, "d", Some(3), Some(3)),
            ])
        );
        assert_eq!(parse_diff("diff --git a/b b/b\nBinary files a/b and b/b differ\n"), None);
        assert_eq!(parse_diff("@@ -0,0 +1 @@\n+only\n").map(|lines| lines[0].new), Some(Some(1)));
    }

    #[test]
    fn parses_log_line() {
        let commit =
            parse_commit("abc\x1fp1 p2\x1fHEAD -> main, origin/main, tag: v1\x1fAnn\x1f1700000000\x1fFix: a, b")
                .unwrap();
        assert_eq!(commit.parents, vec!["p1", "p2"]);
        assert_eq!(commit.refs, vec!["HEAD -> main", "origin/main", "tag: v1"]);
        assert_eq!((commit.time, commit.subject.as_str()), (1700000000, "Fix: a, b"));
        assert_eq!(parse_commit("abc\x1f\x1f\x1fAnn\x1f1\x1fs").unwrap().refs, Vec::<String>::new());
    }

    #[test]
    fn graph_changes_color_at_a_branch() {
        let mut base = node("b", &["a"]);
        base.refs = vec!["origin/main".to_string()];
        let rows = graph(&[node("c", &["b"]), base, node("a", &[])]);
        assert_eq!(rows[0].color, 0);
        assert_eq!(rows[1].color, 1);
        assert_eq!(rows[2].color, 1);
        assert_eq!(rows[1].segments.iter().find(|segment| segment.upper).map(|segment| segment.color), Some(0));
        assert_eq!(rows[1].segments.iter().find(|segment| !segment.upper).map(|segment| segment.color), Some(1));
    }

    #[test]
    fn graph_linear_and_outside_parent() {
        let rows = graph(&[node("c", &["b"]), node("b", &["a"]), node("a", &["outside"])]);
        assert_eq!(rows[0].segments, vec![segment(0, 0, false, 0)]);
        assert_eq!(rows[1].segments, vec![segment(0, 0, true, 0), segment(0, 0, false, 0)]);
        assert_eq!(rows[2].segments, vec![segment(0, 0, true, 0), segment(0, 0, false, 0)]);
        assert!(rows.iter().all(|row| row.column == 0 && row.width == 1));
    }

    #[test]
    fn graph_fork_converges_on_parent() {
        let rows = graph(&[node("m2", &["m1"]), node("f1", &["x"]), node("m1", &["x"]), node("x", &[])]);
        assert_eq!((rows[1].column, rows[1].color, rows[1].width), (1, 1, 2));
        assert_eq!(rows[3].column, 0);
        assert_eq!(rows[3].segments, vec![segment(0, 0, true, 0), segment(1, 0, true, 1)]);
    }

    #[test]
    fn graph_merge_and_lane_reuse() {
        let rows = graph(&[node("m", &["a", "b"]), node("b", &["a"]), node("a", &[]), node("z", &[])]);
        assert_eq!(rows[0].segments, vec![segment(0, 0, false, 0), segment(0, 1, false, 1)]);
        assert_eq!((rows[1].column, rows[1].color), (1, 1));
        assert_eq!(
            rows[1].segments,
            vec![segment(0, 0, true, 0), segment(0, 0, false, 0), segment(1, 1, true, 1), segment(1, 1, false, 1)]
        );
        assert_eq!(rows[2].segments, vec![segment(0, 0, true, 0), segment(1, 0, true, 1)]);
        assert_eq!((rows[3].column, rows[3].color, rows[3].width), (0, 2, 1));
        assert!(rows[3].segments.is_empty());
    }

    #[test]
    fn repository_round_trip() {
        let root = std::env::temp_dir().join(format!("nucli-git-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let sub = root.join("sub");
        fs::create_dir_all(&sub).unwrap();
        run_checked(&root, &["init", "-q", "-b", "main"]).unwrap();
        for (key, value) in [("user.name", "Test"), ("user.email", "test@example.com"), ("commit.gpgsign", "false")] {
            run_checked(&root, &["config", key, value]).unwrap();
        }
        assert_eq!(log(&root, 10), Some(Vec::new()));
        assert_eq!(current_branch(&sub).as_deref(), Some("main"));
        fs::write(sub.join("a.txt"), "one\ntwo\n").unwrap();
        fs::write(root.join("top.txt"), "top\n").unwrap();
        stage(&root, &["sub/a.txt".to_string(), "top.txt".to_string()]).unwrap();
        unstage(&root, &["top.txt".to_string()]).unwrap();
        assert_eq!(
            status(&root).unwrap().changes.iter().map(|c| (c.status, c.staged)).collect::<Vec<_>>(),
            [('A', true), ('U', false)]
        );
        stage(&root, &["top.txt".to_string()]).unwrap();
        commit(&root, "first").unwrap();
        fs::write(sub.join("a.txt"), "one\n2\n").unwrap();
        stage(&sub, &["a.txt".to_string()]).unwrap();
        fs::write(sub.join("a.txt"), "one\n2\nthree\n").unwrap();
        fs::write(sub.join("[n].txt"), "x\ny\nz").unwrap();
        let status = status(&sub).unwrap();
        let summary: Vec<_> =
            status.changes.iter().map(|c| (c.path.as_str(), c.status, c.staged, c.added, c.removed)).collect();
        assert_eq!(
            summary,
            vec![("a.txt", 'M', true, 1, 1), ("[n].txt", 'U', false, 3, 0), ("a.txt", 'M', false, 1, 0)]
        );
        assert_eq!((status.branch.as_str(), status.files, status.added, status.removed), ("main", 2, 5, 1));
        assert_eq!(diff_stat(&sub), Some(DiffStat { files: 2, added: 5, removed: 1 }));
        assert_eq!(
            file_diff(&sub, "a.txt", None).unwrap(),
            vec![
                line(LineKind::Context, "one", Some(1), Some(1)),
                line(LineKind::Removed, "two", Some(2), None),
                line(LineKind::Added, "2", None, Some(2)),
                line(LineKind::Added, "three", None, Some(3)),
            ]
        );
        assert_eq!(file_diff(&sub, "[n].txt", None).unwrap()[2], line(LineKind::Added, "z", None, Some(3)));
        assert_eq!(file_diff(&root, "top.txt", None).unwrap(), vec![line(LineKind::Context, "top", Some(1), Some(1))]);
        stage(&sub, &["a.txt".to_string(), "[n].txt".to_string()]).unwrap();
        commit(&root, "second").unwrap();
        let commits = log(&root, 10).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].refs, vec!["HEAD -> main"]);
        let statuses: Vec<_> =
            commit_changes(&sub, &commits[0].hash).unwrap().iter().map(|c| (c.path.clone(), c.status)).collect();
        assert_eq!(statuses, vec![("[n].txt".to_string(), 'A'), ("a.txt".to_string(), 'M')]);
        let first = file_diff(&sub, "a.txt", Some(&commits[1].hash)).unwrap();
        assert!(first.iter().all(|line| line.kind == LineKind::Added) && first.len() == 2);
        let unchanged = file_diff(&sub, "../top.txt", Some(&commits[0].hash)).unwrap();
        assert_eq!(unchanged, vec![line(LineKind::Context, "top", Some(1), Some(1))]);
        assert_eq!(files(&sub), Some(vec!["[n].txt".to_string(), "a.txt".to_string()]));
        assert_eq!(branches(&root, 5), vec!["main"]);
        assert!(is_repo(&sub));
        fs::remove_dir_all(&root).unwrap();
    }
}

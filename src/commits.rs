use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, AppContext, Context, Entity, HighlightStyle, Hsla, InteractiveElement, IntoElement, MouseButton,
    ParentElement, PathBuilder, Pixels, Render, SharedString, StatefulInteractiveElement, Styled, StyledText, Window,
    canvas, div, fill, point, px, relative, size, uniform_list,
};

use crate::git::{self, Change, Commit, CommitDetails, GitRef, GraphRow};
use crate::history::relative_age;
use crate::icons;
use crate::menu::{self, Item};
use crate::store;
use crate::theme::ItemColor;
use crate::ui;
use crate::window::WorkspaceView;

const LANE: f32 = 12.0;
const CARD_LINES: usize = 20;
const LANE_COLORS: [ItemColor; 8] = [
    ItemColor::Blue,
    ItemColor::Orange,
    ItemColor::Green,
    ItemColor::Purple,
    ItemColor::Teal,
    ItemColor::Pink,
    ItemColor::Yellow,
    ItemColor::Red,
];

#[derive(Clone)]
enum Row {
    Commit(usize),
    File(usize, Change),
    Note(&'static str),
}

pub struct CommitsPanel {
    workspace: String,
    root: PathBuf,
    revision: u64,
    commits: Vec<Commit>,
    graph: Vec<GraphRow>,
    cache: HashMap<PathBuf, (Option<Vec<Commit>>, Vec<GraphRow>)>,
    loaded: bool,
    is_repo: bool,
    loading: bool,
    stale: bool,
    expanded: Option<usize>,
    files: Option<Vec<Change>>,
}

impl CommitsPanel {
    fn new(workspace: String, root: PathBuf, cx: &mut Context<Self>) -> Self {
        let store = store::store(cx);
        cx.observe(&store, |panel, store, cx| {
            let revision = store.read(cx).revision(&panel.root);
            if revision != panel.revision {
                panel.revision = revision;
                panel.reload(cx);
            }
        })
        .detach();
        let mut panel = CommitsPanel {
            workspace,
            root,
            revision: 0,
            commits: Vec::new(),
            graph: Vec::new(),
            cache: HashMap::new(),
            loaded: false,
            is_repo: true,
            loading: false,
            stale: false,
            expanded: None,
            files: None,
        };
        panel.reload(cx);
        panel
    }

    fn set_root(&mut self, root: &Path, cx: &mut Context<Self>) {
        if self.root != root {
            self.root = root.to_path_buf();
            match self.cache.get(root).cloned() {
                Some((commits, graph)) => {
                    self.loaded = true;
                    self.is_repo = commits.is_some();
                    self.commits = commits.unwrap_or_default();
                    self.graph = graph;
                }
                None => {
                    self.loaded = false;
                    self.commits.clear();
                    self.graph.clear();
                }
            }
            self.expanded = None;
            self.files = None;
            self.revision = store::store(cx).read(cx).revision(root);
            self.reload(cx);
        }
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        if self.loading {
            self.stale = true;
            return;
        }
        self.loading = true;
        let root = self.root.clone();
        let progress = store::begin_progress("git log", cx);
        cx.spawn(async move |this, cx| {
            let folder = root.clone();
            let (commits, graph) = cx
                .background_executor()
                .spawn(async move {
                    let commits = git::log(&folder, 500);
                    let graph = commits.as_ref().map(|commits| git::graph(commits)).unwrap_or_default();
                    (commits, graph)
                })
                .await;
            let _ = this.update(cx, |panel, cx| {
                store::finish_progress(progress, cx);
                panel.loading = false;
                panel.cache.insert(root.clone(), (commits.clone(), graph.clone()));
                if panel.root != root {
                    if std::mem::take(&mut panel.stale) {
                        panel.reload(cx);
                    }
                    return;
                }
                panel.loaded = true;
                panel.is_repo = commits.is_some();
                let previous =
                    panel.expanded.and_then(|index| panel.commits.get(index)).map(|commit| commit.hash.clone());
                panel.commits = commits.unwrap_or_default();
                panel.graph = graph;
                panel.expanded = previous.and_then(|hash| panel.commits.iter().position(|commit| commit.hash == hash));
                if panel.expanded.is_none() {
                    panel.files = None;
                }
                cx.notify();
                if std::mem::take(&mut panel.stale) {
                    panel.reload(cx);
                }
            });
        })
        .detach();
    }

    fn toggle(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.expanded == Some(index) {
            self.expanded = None;
            self.files = None;
            cx.notify();
            return;
        }
        self.expanded = Some(index);
        self.files = None;
        cx.notify();
        let root = self.root.clone();
        let hash = self.commits[index].hash.clone();
        let progress = store::begin_progress("git show", cx);
        cx.spawn(async move |this, cx| {
            let files = cx.background_executor().spawn(async move { git::commit_changes(&root, &hash) }).await;
            let _ = this.update(cx, |panel, cx| {
                store::finish_progress(progress, cx);
                if panel.expanded == Some(index) {
                    panel.files = Some(files.unwrap_or_default());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn git(&mut self, text: String, args: Vec<String>, cx: &mut Context<Self>) {
        let (root, workspace) = (self.root.clone(), self.workspace.clone());
        let progress = store::begin_progress(text, cx);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { git::run_checked(&root, &args.iter().map(String::as_str).collect::<Vec<_>>()) })
                .await;
            let _ = this.update(cx, |panel, cx| {
                store::finish_progress(progress, cx);
                if let Err(error) = result {
                    crate::actions::alert(&workspace, "git failed", error, cx);
                }
                panel.reload(cx);
            });
        })
        .detach();
    }

    fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::with_capacity(self.commits.len());
        for index in 0..self.commits.len() {
            rows.push(Row::Commit(index));
            if self.expanded == Some(index) {
                match &self.files {
                    Some(files) => rows.extend(files.iter().cloned().map(|file| Row::File(index, file))),
                    None => rows.push(Row::Note("loading…")),
                }
            }
        }
        rows
    }
}

fn git_item(panel: &Entity<CommitsPanel>, label: impl Into<SharedString>, args: &[&str]) -> Item {
    let (panel, args) = (panel.clone(), args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>());
    let text = format!("git {}", args.iter().take(2).cloned().collect::<Vec<_>>().join(" "));
    menu::action(label, move |_, _, cx| panel.update(cx, |panel, cx| panel.git(text.clone(), args.clone(), cx)))
}

fn confirmed(item: Item, title: String, detail: &'static str, button: &'static str) -> Item {
    let Item::Action { label, run, .. } = item else { return item };
    menu::action(label, move |_, window, cx| {
        let answer = window.prompt(gpui::PromptLevel::Warning, &title, Some(detail), &[button, "Cancel"], cx);
        let run = run.clone();
        cx.spawn_in(window, async move |this, cx| {
            if answer.await == Ok(0) {
                let _ = this.update_in(cx, |view, window, cx| run(view, window, cx));
            }
        })
        .detach();
    })
}

fn named(panel: &Entity<CommitsPanel>, label: &'static str, title: &'static str, args: Vec<String>) -> Item {
    let panel = panel.clone();
    menu::action(label, move |view, window, cx| {
        let (panel, args) = (panel.clone(), args.clone());
        crate::actions::prompt(view, title, "", window, cx, move |name, _, _, cx| {
            if name.is_empty() {
                return;
            }
            let args: Vec<String> =
                args.iter().map(|arg| if arg.is_empty() { name.clone() } else { arg.clone() }).collect();
            let text = format!("git {} {name}", args[0]);
            panel.update(cx, |panel, cx| panel.git(text, args, cx));
        });
    })
}

fn commit_menu(panel: &Entity<CommitsPanel>, commit: &Commit) -> Vec<Item> {
    let hash = commit.hash.as_str();
    let mut items = Vec::new();
    for reference in &commit.refs {
        match reference {
            GitRef::Branch(branch) => items.push(git_item(panel, format!("Switch to {branch}"), &["switch", branch])),
            GitRef::Remote(remote) if !remote.ends_with("/HEAD") => {
                items.push(git_item(panel, format!("Check Out {remote}"), &["switch", "--track", remote]))
            }
            _ => {}
        }
    }
    items.push(git_item(panel, "Check Out Detached", &["switch", "--detach", hash]));
    items.push(named(
        panel,
        "Create Branch…",
        "new branch from this commit",
        vec!["switch".into(), "-c".into(), String::new(), hash.into()],
    ));
    items.push(named(panel, "Create Tag…", "new tag on this commit", vec!["tag".into(), String::new(), hash.into()]));
    items.push(Item::Separator);
    let merge: &[&str] = if commit.parents.len() > 1 { &["-m", "1"] } else { &[] };
    items.push(git_item(panel, "Cherry-Pick", &[&["cherry-pick"], merge, &[hash]].concat()));
    items.push(git_item(panel, "Revert", &[&["revert", "--no-edit"], merge, &[hash]].concat()));
    items.push(Item::Header("reset current branch to here".into()));
    items.push(git_item(panel, "  Soft (keep changes staged)", &["reset", "--soft", hash]));
    items.push(git_item(panel, "  Mixed (keep changes)", &["reset", "--mixed", hash]));
    items.push(confirmed(
        git_item(panel, "  Hard (discard changes)…", &["reset", "--hard", hash]),
        "Reset hard?".into(),
        "Uncommitted changes are discarded and later commits leave the branch.",
        "Reset",
    ));
    let deletable: Vec<Item> = commit
        .refs
        .iter()
        .filter_map(|reference| match reference {
            GitRef::Branch(branch) => Some(confirmed(
                git_item(panel, format!("Delete Branch {branch}…"), &["branch", "-D", branch]),
                format!("Delete branch {branch}?"),
                "Commits only on this branch become unreachable.",
                "Delete",
            )),
            GitRef::Tag(tag) => Some(confirmed(
                git_item(panel, format!("Delete Tag {tag}…"), &["tag", "-d", tag]),
                format!("Delete tag {tag}?"),
                "The tag is removed locally.",
                "Delete",
            )),
            _ => None,
        })
        .collect();
    if !deletable.is_empty() {
        items.push(Item::Separator);
        items.extend(deletable);
    }
    items.push(Item::Separator);
    items.push(menu::copy("Copy Hash", hash));
    items.push(menu::copy("Copy Short Hash", &hash[..hash.len().min(7)]));
    items.push(menu::copy("Copy Message", commit.subject.clone()));
    items
}

fn commit_file_menu(
    panel: &Entity<CommitsPanel>,
    root: &Path,
    hash: &str,
    change: &Change,
    view: &WorkspaceView,
    cx: &gpui::App,
) -> Vec<Item> {
    let mut items = Vec::new();
    let (diff_root, diff) = (root.to_path_buf(), crate::file_view::spec(&change.path, Some(hash)));
    items.push(menu::action("Open Changes", move |view, window, cx| {
        crate::file_view::open_file(view, &diff_root, &diff, None, window, cx)
    }));
    {
        let (root, path) = (root.to_path_buf(), change.path.clone());
        items.push(menu::action("Open Current Version", move |view, window, cx| {
            crate::file_view::open_file(view, &root, &path, None, window, cx)
        }));
    }
    if change.status != 'D' {
        items.push(confirmed(
            git_item(panel, "Restore This Version…", &["checkout", hash, "--", &change.path]),
            format!("Restore {} from this commit?", change.path),
            "The file in the working tree is overwritten.",
            "Restore",
        ));
    }
    items.push(Item::Separator);
    items.extend(crate::file_view::file_menu(view, &root.join(&change.path), &change.path, cx));
    items
}

fn lane_color(index: usize, dark: bool) -> Hsla {
    LANE_COLORS[index % LANE_COLORS.len()].color(dark)
}

fn graph_canvas(row: GraphRow, height: Pixels, dark: bool) -> impl IntoElement {
    let width = LANE * row.width.max(1) as f32 + 4.0;
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let x = |column: usize| bounds.origin.x + px(LANE * column as f32 + LANE / 2.0);
            let (top, middle, bottom) = (bounds.origin.y, bounds.origin.y + height / 2.0, bounds.origin.y + height);
            for segment in &row.segments {
                let mut path = PathBuilder::stroke(px(1.5));
                if segment.upper {
                    path.move_to(point(x(segment.from), top));
                    path.line_to(point(x(segment.to), middle));
                } else {
                    path.move_to(point(x(segment.from), middle));
                    path.line_to(point(x(segment.to), bottom));
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, lane_color(segment.color, dark));
                }
            }
            let dot = px(7.0);
            let origin = point(x(row.column) - dot / 2.0, middle - dot / 2.0);
            window.paint_quad(fill(gpui::Bounds::new(origin, size(dot, dot)), lane_color(row.color, dark)));
        },
    )
    .flex_none()
    .w(px(width))
    .h(height)
}

fn lanes_through(row: GraphRow, height: Pixels, dark: bool) -> impl IntoElement {
    let width = LANE * row.width.max(1) as f32 + 4.0;
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            for segment in row.segments.iter().filter(|segment| !segment.upper) {
                let x = bounds.origin.x + px(LANE * segment.to as f32 + LANE / 2.0);
                let mut path = PathBuilder::stroke(px(1.5));
                path.move_to(point(x, bounds.origin.y));
                path.line_to(point(x, bounds.origin.y + height));
                if let Ok(path) = path.build() {
                    window.paint_path(path, lane_color(segment.color, dark));
                }
            }
        },
    )
    .flex_none()
    .w(px(width))
    .h(height)
}

fn highlighted(text: String, ranges: Vec<(std::ops::Range<usize>, Hsla)>) -> StyledText {
    StyledText::new(text)
        .with_highlights(ranges.into_iter().map(|(range, color)| (range, HighlightStyle::color(color))))
}

fn link_ranges(line: &str, theme: &crate::theme::Theme) -> Vec<(std::ops::Range<usize>, Hsla)> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    for word in line.split_inclusive(' ') {
        let token = word.trim_end();
        if token.starts_with("http") || token.contains('@') {
            ranges.push((offset..offset + token.len(), theme.cyan));
        }
        offset += word.len();
    }
    ranges
}

fn stat_ranges(stats: &str, theme: &crate::theme::Theme) -> Vec<(std::ops::Range<usize>, Hsla)> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    for part in stats.split(", ") {
        if part.contains("insertion") {
            ranges.push((offset..offset + part.len(), theme.green));
        } else if part.contains("deletion") {
            ranges.push((offset..offset + part.len(), theme.red));
        }
        offset += part.len() + 2;
    }
    ranges
}

fn co_authors(message: &str) -> Vec<String> {
    message
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case("co-authored-by")
                .then(|| value.split('<').next().unwrap_or(value).trim().to_string())
        })
        .collect()
}

fn ref_label(reference: &GitRef, lane: Hsla, theme: &crate::theme::Theme) -> impl IntoElement {
    let (text, background): (String, Hsla) = match reference {
        GitRef::Current(name) | GitRef::Branch(name) => (name.clone(), lane),
        GitRef::Head => ("HEAD".to_string(), theme.accent),
        GitRef::Tag(tag) => (format!("{} {tag}", icons::TAG), theme.yellow),
        GitRef::Remote(name) => (format!("{} {name}", icons::CLOUD), lane),
    };
    div()
        .flex_shrink()
        .min_w_0()
        .max_w(px(140.0))
        .truncate()
        .px(px(4.0))
        .bg(background)
        .text_color(theme.on_accent)
        .child(text)
}

struct CommitTooltip {
    commit: Commit,
    lane: Hsla,
    details: Option<CommitDetails>,
}

impl CommitTooltip {
    fn new(root: PathBuf, commit: Commit, lane: Hsla, cx: &mut Context<Self>) -> Self {
        let hash = commit.hash.clone();
        cx.spawn(async move |this, cx| {
            let details = cx.background_executor().spawn(async move { git::commit_details(&root, &hash) }).await;
            let _ = this.update(cx, |tooltip, cx| {
                tooltip.details = details;
                cx.notify();
            });
        })
        .detach();
        CommitTooltip { commit, lane, details: None }
    }
}

impl Render for CommitTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui::theme(cx);
        let size = crate::settings::get(cx).ui_font_size();
        let time = UNIX_EPOCH + Duration::from_secs(self.commit.time.max(0) as u64);
        let details = self.details.clone();
        let message = details.as_ref().map(|details| details.message.clone()).unwrap_or(self.commit.subject.clone());
        let mut lines = message.lines().map(str::to_string);
        let subject = lines.next().unwrap_or_default();
        let mut body: Vec<String> = lines.filter(|line| !line.trim().is_empty()).collect();
        let hidden_lines = body.len().saturating_sub(CARD_LINES);
        body.truncate(CARD_LINES);
        let date = details.as_ref().map(|details| format!("({})", details.date)).unwrap_or_default();
        let authors = co_authors(&message);
        let lane = self.lane;
        let rule = || div().h(px(1.0)).bg(theme.border);
        div()
            .w(px(480.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(10.0))
            .font_family(crate::settings::FONT)
            .text_size(px(size))
            .line_height(px((size * 1.5).round()))
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .text_color(theme.text)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_none()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme.accent)
                            .child(self.commit.author.clone()),
                    )
                    .child(div().flex_none().text_color(theme.faint).child(icons::HISTORY))
                    .child(div().flex_none().text_color(theme.muted).child(relative_age(time)))
                    .child(div().min_w_0().truncate().text_color(theme.muted).child(date)),
            )
            .children(authors.into_iter().map(|author| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().flex_none().font_weight(gpui::FontWeight::BOLD).text_color(theme.accent).child(author))
                    .child(div().flex_none().italic().text_color(theme.faint).child("co-author"))
            }))
            .when(!self.commit.refs.is_empty(), |this| {
                this.children(self.commit.refs.chunks(3).map(|refs| {
                    div().flex().gap(px(4.0)).children(refs.iter().map(|reference| ref_label(reference, lane, &theme)))
                }))
            })
            .child(rule())
            .child(div().font_weight(gpui::FontWeight::BOLD).child(subject))
            .children(body.into_iter().map(|line| {
                let ranges = link_ranges(&line, &theme);
                div().text_color(theme.muted).child(highlighted(line, ranges))
            }))
            .when(hidden_lines > 0, |this| {
                this.child(div().text_color(theme.faint).child(format!("{hidden_lines} more lines")))
            })
            .child(rule())
            .when_some(details.filter(|details| !details.stats.is_empty()), |this, details| {
                let ranges = stat_ranges(&details.stats, &theme);
                this.child(div().text_color(theme.text).child(highlighted(details.stats, ranges)))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().flex_none().text_color(lane).child(icons::COMMIT))
                    .child(div().min_w_0().truncate().text_color(theme.yellow).child(self.commit.hash.clone())),
            )
    }
}

impl Render for CommitsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui::theme(cx);
        if !self.loaded {
            return div().p(px(10.0)).text_color(theme.faint).child("loading…").into_any_element();
        }
        if !self.is_repo {
            return div().p(px(10.0)).text_color(theme.faint).child("not a git repository").into_any_element();
        }
        if self.commits.is_empty() {
            return div().p(px(10.0)).text_color(theme.faint).child("no commits yet").into_any_element();
        }
        let rows = self.rows();
        let count = rows.len();
        let line = px((crate::settings::get(cx).ui_font_size() * 1.7).round());
        uniform_list(
            "commits-list",
            count,
            cx.processor(move |panel: &mut CommitsPanel, range: std::ops::Range<usize>, _, cx| {
                let theme = ui::theme(cx);
                let age_width = px((crate::settings::get(cx).ui_font_size() * 0.6 * 4.0).ceil());
                range
                    .map(|index| match rows[index].clone() {
                        Row::Note(text) => div()
                            .id(("commit-row", index))
                            .h(line)
                            .w_full()
                            .pl(px(28.0))
                            .text_color(theme.faint)
                            .child(text)
                            .into_any_element(),
                        Row::File(commit, change) => {
                            let hash = panel.commits[commit].hash.clone();
                            let (folder, name) = match change.path.rsplit_once('/') {
                                Some((folder, name)) => (folder.to_string(), name.to_string()),
                                None => (String::new(), change.path.clone()),
                            };
                            let (glyph, color) = icons::file(&name);
                            let letter = match change.status {
                                'M' => theme.yellow,
                                'D' => theme.red,
                                _ => theme.green,
                            };
                            let path = change.path.clone();
                            let (menu_hash, menu_change) = (hash.clone(), change.clone());
                            let through = panel.graph.get(commit).cloned();
                            div()
                                .id(("commit-row", index))
                                .h(line)
                                .w_full()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .pr(px(8.0))
                                .hover(|style| style.bg(theme.hover))
                                .children(through.map(|row| lanes_through(row, line, theme.dark)))
                                .child(div().flex_none().w(px(8.0)))
                                .child(div().flex_none().text_color(color.color(theme.dark)).child(glyph))
                                .child(div().min_w_0().truncate().child(name))
                                .child(div().flex_1().min_w_0().truncate().text_color(theme.faint).child(folder))
                                .child(div().flex_none().text_color(letter).child(change.status.to_string()))
                                .on_click(cx.listener(move |panel, _, _, cx| {
                                    let file = crate::file_view::spec(&path, Some(&hash));
                                    let root = panel.root.clone();
                                    let handle = store::store(cx).read(cx).windows.get(&panel.workspace).copied();
                                    cx.defer(move |cx| {
                                        if let Some(handle) = handle {
                                            let _ = handle.update(cx, |view, window, cx| {
                                                crate::file_view::open_file(view, &root, &file, None, window, cx)
                                            });
                                        }
                                    });
                                }))
                                .on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(move |panel, event: &gpui::MouseDownEvent, _, cx| {
                                        let (entity, root, position) =
                                            (cx.entity(), panel.root.clone(), event.position);
                                        let (hash, change) = (menu_hash.clone(), menu_change.clone());
                                        crate::actions::in_window(&panel.workspace, cx, move |view, _, cx| {
                                            let items = commit_file_menu(&entity, &root, &hash, &change, view, cx);
                                            view.open_menu(position, items, cx);
                                        });
                                    }),
                                )
                                .into_any_element()
                        }
                        Row::Commit(commit_index) => {
                            let commit = panel.commits[commit_index].clone();
                            let graph = panel.graph.get(commit_index).cloned();
                            let expanded = panel.expanded == Some(commit_index);
                            let time = UNIX_EPOCH + Duration::from_secs(commit.time.max(0) as u64);
                            let menu_commit = commit.clone();
                            let hidden_refs = commit.refs.len().saturating_sub(2);
                            let lane = graph.as_ref().map_or(theme.accent, |row| lane_color(row.color, theme.dark));
                            let (tooltip_root, tooltip_commit) = (panel.root.clone(), commit.clone());
                            div()
                                .id(("commit-row", index))
                                .h(line)
                                .w_full()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .pr(px(8.0))
                                .overflow_hidden()
                                .when(expanded, |this| this.bg(theme.hover))
                                .hover(|style| style.bg(theme.hover))
                                .children(graph.map(|graph| graph_canvas(graph, line, theme.dark)))
                                .when(!commit.refs.is_empty(), |this| {
                                    this.child(
                                        div()
                                            .flex()
                                            .flex_shrink()
                                            .min_w_0()
                                            .max_w(relative(0.5))
                                            .gap(px(4.0))
                                            .children(
                                                commit
                                                    .refs
                                                    .iter()
                                                    .take(2)
                                                    .map(|reference| ref_label(reference, lane, &theme)),
                                            )
                                            .when(hidden_refs > 0, |this| {
                                                this.child(
                                                    div()
                                                        .flex_none()
                                                        .text_color(theme.faint)
                                                        .child(format!("+{hidden_refs}")),
                                                )
                                            }),
                                    )
                                })
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .child(SharedString::from(commit.subject.clone())),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_none()
                                        .justify_end()
                                        .w(age_width)
                                        .ml(px(6.0))
                                        .text_color(theme.faint)
                                        .child(relative_age(time)),
                                )
                                .tooltip(move |_, cx| {
                                    let (root, commit) = (tooltip_root.clone(), tooltip_commit.clone());
                                    cx.new(|cx| CommitTooltip::new(root, commit, lane, cx)).into()
                                })
                                .on_click(cx.listener(move |panel, _, _, cx| panel.toggle(commit_index, cx)))
                                .on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(move |panel, event: &gpui::MouseDownEvent, _, cx| {
                                        let (items, position) =
                                            (commit_menu(&cx.entity(), &menu_commit), event.position);
                                        crate::actions::in_window(&panel.workspace, cx, move |view, _, cx| {
                                            view.open_menu(position, items, cx)
                                        });
                                    }),
                                )
                                .into_any_element()
                        }
                    })
                    .collect::<Vec<AnyElement>>()
            }),
        )
        .size_full()
        .into_any_element()
    }
}

pub fn panel(
    view: &mut WorkspaceView,
    path: &Path,
    _window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    let entity = match view.panel.commits.clone() {
        Some(entity) => entity,
        None => {
            let (workspace, root) = (view.workspace.clone(), path.to_path_buf());
            let entity = cx.new(|cx| CommitsPanel::new(workspace, root, cx));
            view.panel.commits = Some(entity.clone());
            entity
        }
    };
    let root = path.to_path_buf();
    entity.update(cx, |panel, cx| panel.set_root(&root, cx));
    entity.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_co_authors() {
        let message = "feat: a\n\nbody\n\nCo-Authored-By: Some One <a@b.c>\nco-authored-by: Other";
        assert_eq!(co_authors(message), vec!["Some One".to_string(), "Other".to_string()]);
    }
}

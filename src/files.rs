use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, ClipboardItem, Context, Entity, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Window, div, px, uniform_list,
};

use crate::icons;
use crate::menu::{self, Item};
use crate::store;
use crate::ui;
use crate::window::WorkspaceView;

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TreeRow {
    pub path: String,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub expanded: bool,
    pub letter: Option<char>,
    pub folder_new: Option<bool>,
    pub ignored: bool,
}

#[derive(Default)]
struct Scan {
    entries: HashMap<String, Vec<Entry>>,
    letters: HashMap<String, char>,
    folders: HashMap<String, bool>,
    ignored: HashSet<String>,
}

fn read_dir(root: &Path, relative: &str) -> Vec<Entry> {
    let folder = if relative.is_empty() { root.to_path_buf() } else { root.join(relative) };
    let Ok(read) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut entries: Vec<Entry> = read
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == ".git" || name == ".DS_Store" {
                return None;
            }
            let is_dir = entry.path().is_dir();
            Some(Entry { name, is_dir })
        })
        .collect();
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    entries
}

fn scan(root: &Path, expanded: &BTreeSet<String>) -> Scan {
    let mut result = Scan::default();
    result.entries.insert(String::new(), read_dir(root, ""));
    for folder in expanded {
        result.entries.insert(folder.clone(), read_dir(root, folder));
    }
    if let Some(status) = crate::git::status(root) {
        for change in &status.changes {
            let letter = result.letters.entry(change.path.clone()).or_insert(change.status);
            if !change.staged {
                *letter = change.status;
            }
            let is_new = matches!(change.status, 'U' | 'A');
            let mut parent = change.path.as_str();
            while let Some((folder, _)) = parent.rsplit_once('/') {
                let current = result.folders.entry(folder.to_string()).or_insert(is_new);
                *current = *current && is_new;
                parent = folder;
            }
        }
    }
    result.ignored = crate::git::ignored(root).into_iter().map(|path| path.trim_end_matches('/').to_string()).collect();
    result
}

fn build_rows(scan: &Scan, expanded: &BTreeSet<String>) -> Vec<TreeRow> {
    fn walk(
        scan: &Scan,
        expanded: &BTreeSet<String>,
        folder: &str,
        depth: usize,
        ignored: bool,
        rows: &mut Vec<TreeRow>,
    ) {
        let Some(entries) = scan.entries.get(folder) else { return };
        for entry in entries {
            let path = if folder.is_empty() { entry.name.clone() } else { format!("{folder}/{}", entry.name) };
            let is_ignored = ignored || scan.ignored.contains(&path);
            let is_expanded = entry.is_dir && expanded.contains(&path);
            rows.push(TreeRow {
                path: path.clone(),
                name: entry.name.clone(),
                depth,
                is_dir: entry.is_dir,
                expanded: is_expanded,
                letter: scan.letters.get(&path).copied(),
                folder_new: scan.folders.get(&path).copied(),
                ignored: is_ignored,
            });
            if is_expanded {
                walk(scan, expanded, &path, depth + 1, is_ignored, rows);
            }
        }
    }
    let mut rows = Vec::new();
    walk(scan, expanded, "", 0, false, &mut rows);
    rows
}

pub struct FilesPanel {
    workspace: String,
    root: PathBuf,
    revision: u64,
    rows: Vec<TreeRow>,
    loading: bool,
    stale: bool,
}

impl FilesPanel {
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
        let mut panel = FilesPanel { workspace, root, revision: 0, rows: Vec::new(), loading: false, stale: false };
        panel.reload(cx);
        panel
    }

    fn expanded(&self, cx: &App) -> BTreeSet<String> {
        store::store(cx)
            .read(cx)
            .workspace(&self.workspace)
            .and_then(|state| state.project(&self.root))
            .map(|project| project.expanded.clone())
            .unwrap_or_default()
    }

    fn set_root(&mut self, root: &Path, cx: &mut Context<Self>) {
        if self.root != root {
            self.root = root.to_path_buf();
            self.rows.clear();
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
        let expanded = self.expanded(cx);
        cx.spawn(async move |this, cx| {
            let keep = expanded.clone();
            let rows = cx.background_executor().spawn(async move { build_rows(&scan(&root, &keep), &keep) }).await;
            let _ = this.update(cx, |panel, cx| {
                panel.loading = false;
                panel.rows = rows;
                cx.notify();
                if std::mem::take(&mut panel.stale) {
                    panel.reload(cx);
                }
            });
        })
        .detach();
    }

    fn toggle(&mut self, path: &str, cx: &mut Context<Self>) {
        let (workspace, root, path) = (self.workspace.clone(), self.root.clone(), path.to_string());
        store::store(cx).update(cx, |store, _| {
            if let Some(project) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(&root))
                && !project.expanded.remove(&path)
            {
                project.expanded.insert(path);
            }
        });
        self.reload(cx);
    }
}

fn file_menu(root: &Path, row: &TreeRow, view: &WorkspaceView, cx: &App) -> Vec<Item> {
    let full = root.join(&row.path);
    let mut items = crate::file_view::open_with_menu(view, full.clone(), cx);
    items.push(Item::Separator);
    let (reveal, copy, relative) = (full.clone(), full, row.path.clone());
    items.push(menu::action(crate::actions::reveal_label(), move |_, _, cx| cx.reveal_path(&reveal)));
    items.push(menu::action("Copy Path", move |_, _, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(copy.to_string_lossy().into_owned()))
    }));
    items.push(menu::action("Copy Relative Path", move |_, _, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(relative.clone()))
    }));
    items
}

impl Render for FilesPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.rows.len();
        let line = px((crate::settings::get(cx).ui_font_size() * 1.7).round());
        uniform_list(
            "files-list",
            count,
            cx.processor(move |panel: &mut FilesPanel, range: std::ops::Range<usize>, _, cx| {
                let theme = ui::theme(cx);
                range
                    .map(|index| {
                        let row = panel.rows[index].clone();
                        let (glyph, color) = if row.is_dir {
                            (if row.expanded { icons::FOLDER_OPEN } else { icons::FOLDER }, theme.accent)
                        } else {
                            let (glyph, color) = icons::file(&row.name);
                            (glyph, color.color(theme.dark))
                        };
                        let text = match row.letter {
                            _ if row.ignored => theme.faint,
                            Some('M') => theme.yellow,
                            Some('D') => theme.red,
                            Some('A' | 'U') => theme.green,
                            _ => theme.text,
                        };
                        let right: Option<(SharedString, gpui::Hsla)> = match (row.letter, row.folder_new) {
                            (Some(letter), _) => Some((letter.to_string().into(), text)),
                            (None, Some(true)) => Some((icons::DOT.into(), theme.green)),
                            (None, Some(false)) => Some((icons::DOT.into(), theme.orange)),
                            _ => None,
                        };
                        let (click, menu_row) = (row.clone(), row.clone());
                        div()
                            .id(("file-row", index))
                            .h(line)
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .pl(px(8.0 + row.depth as f32 * 12.0))
                            .pr(px(8.0))
                            .text_color(text)
                            .hover(|style| style.bg(theme.hover))
                            .child(
                                div()
                                    .flex_none()
                                    .text_color(if row.ignored { theme.faint } else { color })
                                    .child(glyph),
                            )
                            .child(div().flex_1().min_w_0().truncate().child(row.name.clone()))
                            .when_some(right, |this, (label, color)| {
                                this.child(div().flex_none().text_color(color).child(label))
                            })
                            .on_click(cx.listener(move |panel, _, window, cx| {
                                if click.is_dir {
                                    panel.toggle(&click.path, cx);
                                } else {
                                    let (root, path, workspace) =
                                        (panel.root.clone(), click.path.clone(), panel.workspace.clone());
                                    let handle = store::store(cx).read(cx).windows.get(&workspace).copied();
                                    let _ = window;
                                    cx.defer(move |cx| {
                                        if let Some(handle) = handle {
                                            let _ = handle.update(cx, |view, window, cx| {
                                                crate::file_view::open_file(view, &root, &path, None, window, cx)
                                            });
                                        }
                                    });
                                }
                            }))
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(move |panel, event: &gpui::MouseDownEvent, _, cx| {
                                    let (root, row, workspace, position) =
                                        (panel.root.clone(), menu_row.clone(), panel.workspace.clone(), event.position);
                                    let handle = store::store(cx).read(cx).windows.get(&workspace).copied();
                                    cx.defer(move |cx| {
                                        if let Some(handle) = handle {
                                            let _ = handle.update(cx, |view, _, cx| {
                                                let items = file_menu(&root, &row, view, cx);
                                                view.open_menu(position, items, cx);
                                            });
                                        }
                                    });
                                }),
                            )
                            .into_any_element()
                    })
                    .collect::<Vec<AnyElement>>()
            }),
        )
        .size_full()
    }
}

pub fn panel(
    view: &mut WorkspaceView,
    path: &Path,
    _window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    let entity: Entity<FilesPanel> = match view.panel.files.clone() {
        Some(entity) => entity,
        None => {
            let (workspace, root) = (view.workspace.clone(), path.to_path_buf());
            let entity = cx.new(|cx| FilesPanel::new(workspace, root, cx));
            view.panel.files = Some(entity.clone());
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
    fn builds_tree_rows() {
        let mut scan = Scan::default();
        scan.entries.insert(
            String::new(),
            vec![Entry { name: "src".into(), is_dir: true }, Entry { name: "a.txt".into(), is_dir: false }],
        );
        scan.entries.insert("src".into(), vec![Entry { name: "main.rs".into(), is_dir: false }]);
        scan.letters.insert("src/main.rs".into(), 'M');
        scan.folders.insert("src".into(), false);
        scan.ignored.insert("a.txt".into());
        let expanded = BTreeSet::from(["src".to_string()]);
        let rows = build_rows(&scan, &expanded);
        assert_eq!(rows.iter().map(|row| row.path.as_str()).collect::<Vec<_>>(), vec!["src", "src/main.rs", "a.txt"]);
        assert_eq!(rows[1].depth, 1);
        assert_eq!(rows[1].letter, Some('M'));
        assert_eq!(rows[0].folder_new, Some(false));
        assert!(rows[2].ignored);
        assert_eq!(build_rows(&scan, &BTreeSet::new()).len(), 2);
    }
}

use std::path::{Path, PathBuf};

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, PromptLevel, Render,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::git::{self, Change, Status};
use crate::icons;
use crate::store;
use crate::text_input::{InputEvent, TextInput};
use crate::ui;
use crate::window::WorkspaceView;

const ROW_LIMIT: usize = 500;

pub struct ChangesPanel {
    workspace: String,
    root: PathBuf,
    revision: u64,
    status: Option<Status>,
    loaded: bool,
    loading: bool,
    stale: bool,
    message: Entity<TextInput>,
    staged_collapsed: bool,
    changes_collapsed: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Batch {
    Stage,
    Unstage,
    Discard,
}

impl ChangesPanel {
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
        let message = cx.new(|cx| TextInput::new("message ([enter] to commit)", cx));
        cx.subscribe(&message, |panel, _, event, cx| {
            if matches!(event, InputEvent::Submit) {
                panel.commit(cx);
            }
        })
        .detach();
        let mut panel = ChangesPanel {
            workspace,
            root,
            revision: 0,
            status: None,
            loaded: false,
            loading: false,
            stale: false,
            message,
            staged_collapsed: false,
            changes_collapsed: false,
        };
        panel.reload(cx);
        panel
    }

    fn set_root(&mut self, root: &Path, cx: &mut Context<Self>) {
        if self.root != root {
            self.root = root.to_path_buf();
            self.status = None;
            self.loaded = false;
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
        let progress = store::begin_progress("git status", cx);
        cx.spawn(async move |this, cx| {
            let status = cx.background_executor().spawn(async move { git::status(&root) }).await;
            let _ = this.update(cx, |panel, cx| {
                store::finish_progress(progress, cx);
                panel.loading = false;
                panel.loaded = true;
                panel.status = status;
                cx.notify();
                if std::mem::take(&mut panel.stale) {
                    panel.reload(cx);
                }
            });
        })
        .detach();
    }

    fn run(
        &mut self,
        text: &str,
        action: impl FnOnce(&Path) -> Result<(), String> + Send + 'static,
        done: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let root = self.root.clone();
        let workspace = self.workspace.clone();
        let progress = store::begin_progress(text, cx);
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { action(&root) }).await;
            let _ = this.update(cx, |panel, cx| {
                store::finish_progress(progress, cx);
                match result {
                    Ok(()) => done(panel, cx),
                    Err(error) => {
                        let handle = store::store(cx).read(cx).windows.get(&workspace).copied();
                        cx.defer(move |cx| {
                            if let Some(handle) = handle {
                                let _ = handle.update(cx, |_, window, cx| {
                                    drop(window.prompt(PromptLevel::Warning, "git failed", Some(&error), &["OK"], cx));
                                });
                            }
                        });
                    }
                }
                panel.reload(cx);
            });
        })
        .detach();
    }

    fn commit(&mut self, cx: &mut Context<Self>) {
        let text = self.message.read(cx).text.trim().to_string();
        let has_staged = self.status.as_ref().is_some_and(|status| status.changes.iter().any(|change| change.staged));
        if text.is_empty() || !has_staged {
            return;
        }
        self.run(
            "git commit",
            move |root| git::commit(root, &text),
            |panel, cx| panel.message.update(cx, |input, cx| input.clear(cx)),
            cx,
        );
    }

    fn batch(&mut self, kind: Batch, changes: Vec<Change>, cx: &mut Context<Self>) {
        let paths: Vec<String> = changes.iter().map(|change| change.path.clone()).collect();
        if paths.is_empty() {
            return;
        }
        match kind {
            Batch::Stage => self.run("git add", move |root| git::stage(root, &paths), |_, _| {}, cx),
            Batch::Unstage => self.run("git restore --staged", move |root| git::unstage(root, &paths), |_, _| {}, cx),
            Batch::Discard => self.confirm_discard(changes, cx),
        }
    }

    fn confirm_discard(&mut self, changes: Vec<Change>, cx: &mut Context<Self>) {
        let detail = if changes.len() == 1 {
            format!("Discard changes to {}? This cannot be undone. New files go to the trash.", changes[0].path)
        } else {
            format!("Discard changes to {} files? This cannot be undone. New files go to the trash.", changes.len())
        };
        let Some(handle) = store::store(cx).read(cx).windows.get(&self.workspace).copied() else { return };
        let panel = cx.entity();
        cx.defer(move |cx| {
            let answer = handle.update(cx, |_, window, cx| {
                window.prompt(PromptLevel::Warning, "Discard changes?", Some(&detail), &["Discard", "Cancel"], cx)
            });
            let Ok(answer) = answer else { return };
            cx.spawn(async move |cx| {
                if answer.await == Ok(0) {
                    let _ = panel.update(cx, |panel, cx| {
                        let root = panel.root.clone();
                        panel.run("discarding changes", move |_| discard(&root, &changes), |_, _| {}, cx);
                    });
                }
            })
            .detach();
        });
    }

    fn section(&self, title: &'static str, staged: bool, changes: Vec<Change>, cx: &mut Context<Self>) -> AnyElement {
        let theme = ui::theme(cx);
        let collapsed = if staged { self.staged_collapsed } else { self.changes_collapsed };
        let id = if staged { "staged" } else { "unstaged" };
        let (stage_all, unstage_all, discard_all) = (changes.clone(), changes.clone(), changes.clone());
        let tracked_or_new = !staged;
        let mut section = div().flex().flex_col().child(
            div()
                .id(SharedString::from(format!("section-{id}")))
                .group("section")
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .py(px(2.0))
                .hover(|style| style.bg(theme.hover))
                .child(div().text_color(theme.faint).child(if collapsed {
                    icons::CHEVRON_RIGHT
                } else {
                    icons::CHEVRON_DOWN
                }))
                .child(div().flex_1().text_color(theme.muted).child(title))
                .when(tracked_or_new, |this| {
                    this.child(action_button(format!("{id}-discard"), icons::DISCARD, theme.muted).on_click(
                        cx.listener(move |panel, _, _, cx| {
                            panel.batch(Batch::Discard, discard_all.clone(), cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .child(
                        action_button(format!("{id}-stage"), icons::ADD, theme.muted).on_click(cx.listener(
                            move |panel, _, _, cx| {
                                panel.batch(Batch::Stage, stage_all.clone(), cx);
                                cx.stop_propagation();
                            },
                        )),
                    )
                })
                .when(staged, |this| {
                    this.child(action_button(format!("{id}-unstage"), icons::REMOVE, theme.muted).on_click(
                        cx.listener(move |panel, _, _, cx| {
                            panel.batch(Batch::Unstage, unstage_all.clone(), cx);
                            cx.stop_propagation();
                        }),
                    ))
                })
                .child(div().px(px(4.0)).bg(theme.border).text_color(theme.text).child(changes.len().to_string()))
                .on_click(cx.listener(move |panel, _, _, cx| {
                    if staged {
                        panel.staged_collapsed = !panel.staged_collapsed;
                    } else {
                        panel.changes_collapsed = !panel.changes_collapsed;
                    }
                    cx.notify();
                })),
        );
        if collapsed {
            return section.into_any_element();
        }
        for (index, change) in changes.iter().take(ROW_LIMIT).enumerate() {
            section = section.child(self.row(index, change.clone(), cx));
        }
        if changes.len() > ROW_LIMIT {
            section = section.child(
                div()
                    .px(px(24.0))
                    .text_color(theme.faint)
                    .child(format!("{} more files not shown", changes.len() - ROW_LIMIT)),
            );
        }
        section.into_any_element()
    }

    fn row(&self, index: usize, change: Change, cx: &mut Context<Self>) -> AnyElement {
        let theme = ui::theme(cx);
        let (folder, name) = match change.path.rsplit_once('/') {
            Some((folder, name)) => (folder.to_string(), name.to_string()),
            None => (String::new(), change.path.clone()),
        };
        let (glyph, color) = icons::file(&name);
        let letter_color = match change.status {
            'M' => theme.yellow,
            'D' => theme.red,
            _ => theme.green,
        };
        let id = format!("{}{index}", if change.staged { "s" } else { "c" });
        let group = SharedString::from(format!("change-{id}"));
        let (open_change, discard_change, toggle_change) = (change.clone(), change.clone(), change.clone());
        let staged = change.staged;
        let workspace = self.workspace.clone();
        let root = self.root.clone();
        let show = move |change: &Change, cx: &mut Context<Self>| {
            if change.status == 'D' && !change.staged {
                return;
            }
            let handle = store::store(cx).read(cx).windows.get(&workspace).copied();
            let (root, path) = (root.clone(), change.path.clone());
            cx.defer(move |cx| {
                if let Some(handle) = handle {
                    let _ = handle.update(cx, |view, window, cx| {
                        crate::file_view::open_file(view, &root, &path, None, window, cx)
                    });
                }
            });
        };
        let hover_group = group.clone();
        div()
            .id(SharedString::from(format!("change-row-{id}")))
            .group(group)
            .flex()
            .items_center()
            .gap(px(6.0))
            .pl(px(20.0))
            .pr(px(8.0))
            .hover(|style| style.bg(theme.hover))
            .child(div().flex_none().text_color(color.color(theme.dark)).child(glyph))
            .child(div().flex_none().max_w(px(160.0)).truncate().child(name))
            .child(div().flex_1().min_w_0().truncate().text_color(theme.faint).child(folder))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .invisible()
                    .group_hover(hover_group, |style| style.visible())
                    .when(!staged, |this| {
                        this.child(action_button(format!("discard-{id}"), icons::DISCARD, theme.muted).on_click(
                            cx.listener(move |panel, _, _, cx| {
                                panel.batch(Batch::Discard, vec![discard_change.clone()], cx);
                                cx.stop_propagation();
                            }),
                        ))
                    })
                    .child(
                        action_button(
                            format!("toggle-{id}"),
                            if staged { icons::REMOVE } else { icons::ADD },
                            theme.muted,
                        )
                        .on_click(cx.listener(move |panel, _, _, cx| {
                            let kind = if staged { Batch::Unstage } else { Batch::Stage };
                            panel.batch(kind, vec![toggle_change.clone()], cx);
                            cx.stop_propagation();
                        })),
                    ),
            )
            .child(div().flex_none().w(px(10.0)).text_color(letter_color).child(change.status.to_string()))
            .on_click(cx.listener(move |_, _, _, cx| show(&open_change, cx)))
            .into_any_element()
    }
}

fn discard(root: &Path, changes: &[Change]) -> Result<(), String> {
    let (new, tracked): (Vec<&Change>, Vec<&Change>) = changes.iter().partition(|change| change.status == 'U');
    for change in new {
        trash::delete(root.join(&change.path)).map_err(|error| error.to_string())?;
    }
    let paths: Vec<String> = tracked.iter().map(|change| change.path.clone()).collect();
    if paths.is_empty() { Ok(()) } else { git::discard(root, &paths) }
}

fn action_button(id: String, glyph: &'static str, color: gpui::Hsla) -> gpui::Stateful<gpui::Div> {
    div().id(SharedString::from(id)).px(px(3.0)).text_color(color).child(glyph)
}

impl Render for ChangesPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui::theme(cx);
        let Some(status) = self.status.clone() else {
            let text = if self.loaded { "not a git repository" } else { "loading…" };
            return div().p(px(10.0)).text_color(theme.faint).child(text).into_any_element();
        };
        let staged: Vec<Change> = status.changes.iter().filter(|change| change.staged).cloned().collect();
        let unstaged: Vec<Change> = status.changes.iter().filter(|change| !change.staged).cloned().collect();
        let can_commit = !staged.is_empty() && !self.message.read(cx).text.trim().is_empty();
        let summary = if status.changes.is_empty() {
            "no changes".to_string()
        } else {
            format!("{} {}", status.files, if status.files == 1 { "file" } else { "files" })
        };
        let mut body = div()
            .id("changes")
            .size_full()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .child(div().flex_1().min_w_0().truncate().child(format!("{} {}", icons::BRANCH, status.branch)))
                    .child(div().text_color(theme.muted).child(summary))
                    .when(status.added + status.removed > 0, |this| {
                        this.child(div().text_color(theme.green).child(format!("+{}", status.added)))
                            .child(div().text_color(theme.red).child(format!("-{}", status.removed)))
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .mx(px(8.0))
                    .px(px(6.0))
                    .border_1()
                    .border_color(theme.border)
                    .child(self.message.clone()),
            )
            .child(
                div()
                    .id("commit-button")
                    .flex()
                    .flex_none()
                    .justify_center()
                    .mx(px(8.0))
                    .mt(px(4.0))
                    .mb(px(6.0))
                    .when(can_commit, |this| this.bg(theme.green).text_color(theme.on_accent))
                    .when(!can_commit, |this| this.bg(theme.hover).text_color(theme.faint))
                    .child(format!("{} COMMIT", icons::CHECK))
                    .on_click(cx.listener(|panel, _, _, cx| panel.commit(cx))),
            );
        if status.changes.is_empty() {
            body = body.child(div().px(px(10.0)).py(px(6.0)).text_color(theme.faint).child("working tree clean"));
        }
        if !staged.is_empty() {
            body = body.child(self.section("STAGED", true, staged, cx));
        }
        if !unstaged.is_empty() {
            body = body.child(self.section("CHANGES", false, unstaged, cx));
        }
        body.into_any_element()
    }
}

pub fn panel(
    view: &mut WorkspaceView,
    path: &Path,
    _window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    let entity = match view.panel.changes.clone() {
        Some(entity) => entity,
        None => {
            let (workspace, root) = (view.workspace.clone(), path.to_path_buf());
            let entity = cx.new(|cx| ChangesPanel::new(workspace, root, cx));
            view.panel.changes = Some(entity.clone());
            entity
        }
    };
    let root = path.to_path_buf();
    entity.update(cx, |panel, cx| panel.set_root(&root, cx));
    entity.into_any_element()
}

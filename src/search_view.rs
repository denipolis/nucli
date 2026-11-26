use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, Context, Entity, Focusable, HighlightStyle, InteractiveElement, IntoElement,
    MouseButton, ParentElement, PromptLevel, Render, ScrollStrategy, SharedString, StatefulInteractiveElement,
    StrikethroughStyle, Styled, StyledText, UniformListScrollHandle, Window, div, px, uniform_list,
};

use crate::actions::in_window;
use crate::editor::find_bar::{find_button, find_toggle};
use crate::file_ops::DraggedPath;
use crate::find::Hit;
use crate::icons;
use crate::menu;
use crate::persist::View;
use crate::search::{self, FileHits, Found, Request};
use crate::settings;
use crate::store;
use crate::text_input::{InputEvent, TextInput};
use crate::theme::Theme;
use crate::ui;
use crate::window::WorkspaceView;

#[derive(Clone, Copy)]
enum Row {
    File(usize),
    Match(usize, usize),
}

#[derive(Clone, Copy, PartialEq)]
enum Field {
    Query,
    Replacement,
    Include,
    Exclude,
}

pub struct SearchPanel {
    workspace: String,
    root: PathBuf,
    revision: u64,
    query: Entity<TextInput>,
    replacement: Entity<TextInput>,
    include: Entity<TextInput>,
    exclude: Entity<TextInput>,
    files: Vec<FileHits>,
    rows: Vec<Row>,
    collapsed: HashSet<String>,
    current: Option<(usize, usize)>,
    searched: bool,
    searching: bool,
    stale: bool,
    capped: bool,
    error: Option<String>,
    notice: Option<String>,
    generation: Arc<AtomicUsize>,
    typing: usize,
    scroll: UniformListScrollHandle,
}

fn options(cx: &App) -> search::Options {
    store::store(cx).read(cx).ui.search.clone()
}

fn plural(count: usize, word: &str) -> String {
    if count == 1 { format!("{count} {word}") } else { format!("{count} {word}s") }
}

fn targets(files: &[FileHits]) -> Vec<(String, Vec<Hit>)> {
    files.iter().map(|file| (file.path.clone(), file.found.iter().map(|found| found.hit).collect())).collect()
}

impl SearchPanel {
    fn new(workspace: String, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = store::store(cx);
        cx.observe(&store, |panel, store, cx| {
            let revision = store.read(cx).revision(&panel.root);
            panel.stale |= revision != panel.revision && panel.searched;
            panel.revision = revision;
            if panel.stale && store.read(cx).ui.docks.shown(View::Search) {
                panel.stale = false;
                panel.schedule(cx);
            }
        })
        .detach();
        let saved = options(cx);
        let input = |field: Field, placeholder: &str, text: &str, cx: &mut Context<Self>| {
            let input = cx.new(|cx| {
                let mut input = TextInput::new(placeholder, cx);
                input.set_text(text, cx);
                input
            });
            cx.subscribe_in(&input, window, move |panel, _, event, window, cx| {
                panel.input_event(field, event, window, cx)
            })
            .detach();
            input
        };
        let query = input(Field::Query, "search", "", cx);
        let replacement = input(Field::Replacement, "replace", "", cx);
        let include = input(Field::Include, "files to include (*.rs, ./src)", &saved.include, cx);
        let exclude = input(Field::Exclude, "files to exclude", &saved.exclude, cx);
        let revision = store.read(cx).revision(&root);
        SearchPanel {
            workspace,
            root,
            revision,
            query,
            replacement,
            include,
            exclude,
            files: Vec::new(),
            rows: Vec::new(),
            collapsed: HashSet::new(),
            current: None,
            searched: false,
            searching: false,
            stale: false,
            capped: false,
            error: None,
            notice: None,
            generation: Arc::new(AtomicUsize::new(0)),
            typing: 0,
            scroll: UniformListScrollHandle::new(),
        }
    }

    fn set_root(&mut self, root: &Path, cx: &mut Context<Self>) {
        if self.root != root {
            self.root = root.to_path_buf();
            self.revision = store::store(cx).read(cx).revision(root);
            self.files.clear();
            self.collapsed.clear();
            self.notice = None;
            self.run(cx);
        }
    }

    fn input_event(&mut self, field: Field, event: &InputEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            InputEvent::Changed => {
                self.notice = None;
                self.schedule(cx);
            }
            InputEvent::Submit => self.run(cx),
            InputEvent::NextField => self.focus_next(field, window, cx),
            InputEvent::Cancel => in_window(&self.workspace, cx, |view, window, cx| view.focus_current(window, cx)),
            InputEvent::Up | InputEvent::Down => {}
        }
    }

    fn focus_next(&self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        let saved = options(cx);
        let mut order = vec![(Field::Query, &self.query)];
        if saved.replace {
            order.push((Field::Replacement, &self.replacement));
        }
        if saved.details {
            order.extend([(Field::Include, &self.include), (Field::Exclude, &self.exclude)]);
        }
        let position = order.iter().position(|(item, _)| *item == field).unwrap_or(0);
        let (_, next) = order[(position + 1) % order.len()];
        window.focus(&next.focus_handle(cx), cx);
    }

    fn focus(&mut self, seed: Option<String>, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        let text = seed.clone().unwrap_or_else(|| self.query.read(cx).text.clone());
        self.query.update(cx, |input, cx| input.set_text(&text, cx));
        if seed.is_some() {
            self.run(cx);
        }
        let target = if replace && !text.is_empty() { &self.replacement } else { &self.query };
        window.focus(&target.focus_handle(cx), cx);
    }

    fn request(&self, cx: &App) -> Request {
        let saved = options(cx);
        let text = |input: &Entity<TextInput>| if saved.details { input.read(cx).text.clone() } else { String::new() };
        Request {
            text: self.query.read(cx).text.clone(),
            options: saved.find,
            include: text(&self.include),
            exclude: text(&self.exclude),
            template: saved.replace.then(|| self.replacement.read(cx).text.clone()),
        }
    }

    fn change(&mut self, change: impl FnOnce(&mut search::Options), cx: &mut Context<Self>) {
        store::store(cx).update(cx, |store, cx| {
            change(&mut store.ui.search);
            store.changed(cx);
        });
        self.run(cx);
    }

    fn save_globs(&self, cx: &mut Context<Self>) {
        let (include, exclude) = (self.include.read(cx).text.clone(), self.exclude.read(cx).text.clone());
        store::store(cx).update(cx, |store, cx| {
            let saved = &mut store.ui.search;
            if saved.include != include || saved.exclude != exclude {
                (saved.include, saved.exclude) = (include, exclude);
                store.changed(cx);
            }
        });
    }

    fn schedule(&mut self, cx: &mut Context<Self>) {
        self.typing += 1;
        let typing = self.typing;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(250)).await;
            let _ = this.update(cx, |panel, cx| {
                if panel.typing == typing {
                    panel.run(cx);
                }
            });
        })
        .detach();
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        self.typing += 1;
        self.save_globs(cx);
        let request = self.request(cx);
        let generation = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        self.error = None;
        if request.text.is_empty() {
            (self.files, self.searched, self.searching, self.capped, self.current) =
                (Vec::new(), false, false, false, None);
            self.rebuild();
            cx.notify();
            return;
        }
        self.searching = true;
        cx.notify();
        let (root, counter) = (self.root.clone(), self.generation.clone());
        let progress = store::begin_progress("searching", cx);
        let backend = crate::backend::Backend::of(&self.workspace, &root, cx);
        let remote = backend.is_remote().then(|| {
            backend
                .ask::<Result<search::Outcome, String>>(crate::backend::Query::Search { request: request.clone() }, cx)
        });
        cx.spawn(async move |this, cx| {
            let outcome = match remote {
                Some(asked) => asked.await.unwrap_or_else(|| Err("The other computer did not answer.".to_string())),
                None => {
                    cx.background_executor()
                        .spawn(async move {
                            search::search(&root, &request, &|| counter.load(Ordering::Relaxed) != generation)
                        })
                        .await
                }
            };
            let _ = this.update(cx, |panel, cx| {
                store::finish_progress(progress, cx);
                if panel.generation.load(Ordering::Relaxed) != generation {
                    return;
                }
                (panel.searching, panel.searched, panel.current) = (false, true, None);
                match outcome {
                    Ok(outcome) => (panel.files, panel.capped) = (outcome.files, outcome.capped),
                    Err(error) => (panel.files, panel.capped, panel.error) = (Vec::new(), false, Some(error)),
                }
                panel.rebuild();
                cx.notify();
            });
        })
        .detach();
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        self.query.update(cx, |input, cx| input.set_text("", cx));
        self.notice = None;
        self.run(cx);
    }

    fn rebuild(&mut self) {
        self.rows.clear();
        for (index, file) in self.files.iter().enumerate() {
            self.rows.push(Row::File(index));
            if !self.collapsed.contains(&file.path) {
                self.rows.extend((0..file.found.len()).map(|found| Row::Match(index, found)));
            }
        }
    }

    fn toggle(&mut self, file: usize, cx: &mut Context<Self>) {
        let Some(path) = self.files.get(file).map(|file| file.path.clone()) else { return };
        if !self.collapsed.remove(&path) {
            self.collapsed.insert(path);
        }
        self.rebuild();
        cx.notify();
    }

    fn toggle_all(&mut self, cx: &mut Context<Self>) {
        if self.collapsed.is_empty() {
            self.collapsed = self.files.iter().map(|file| file.path.clone()).collect();
        } else {
            self.collapsed.clear();
        }
        self.rebuild();
        cx.notify();
    }

    fn dismiss(&mut self, file: usize, found: Option<usize>, cx: &mut Context<Self>) {
        let Some(hits) = self.files.get_mut(file) else { return };
        if let Some(found) = found.filter(|found| *found < hits.found.len()) {
            hits.found.remove(found);
        }
        if found.is_none() || hits.found.is_empty() {
            self.files.remove(file);
        }
        self.current = None;
        self.rebuild();
        cx.notify();
    }

    fn open(&mut self, file: usize, index: usize, cx: &mut Context<Self>) {
        let Some(hits) = self.files.get(file) else { return };
        let Some(hit) = hits.found.get(index).map(|found| found.hit) else { return };
        let (root, path) = (self.root.clone(), hits.path.clone());
        self.current = Some((file, index));
        if let Some(row) = self.rows.iter().position(|row| matches!(row, Row::Match(f, i) if (*f, *i) == (file, index)))
        {
            self.scroll.scroll_to_item(row, ScrollStrategy::Center);
        }
        let end = if hit.end.row == hit.start.row { hit.end.col } else { usize::MAX };
        in_window(&self.workspace, cx, move |view, window, cx| {
            crate::file_view::open_file(view, &root, &path, None, window, cx);
            if let Some(editor) = crate::file_view::active_editor(view, cx) {
                editor.update(cx, |editor, cx| {
                    editor.goto(hit.start.row + 1, hit.start.col..end);
                    cx.notify();
                });
            }
        });
        cx.notify();
    }

    fn step(&mut self, delta: i32, cx: &mut Context<Self>) {
        let all: Vec<(usize, usize)> = self
            .files
            .iter()
            .enumerate()
            .flat_map(|(file, hits)| (0..hits.found.len()).map(move |index| (file, index)))
            .collect();
        if all.is_empty() {
            return;
        }
        let next = match self.current.and_then(|current| all.iter().position(|item| *item == current)) {
            Some(position) => (position as i32 + delta).rem_euclid(all.len() as i32) as usize,
            None if delta > 0 => 0,
            None => all.len() - 1,
        };
        let (file, index) = all[next];
        if self.collapsed.remove(&self.files[file].path) {
            self.rebuild();
        }
        self.open(file, index, cx);
    }

    fn replace(&mut self, targets: Vec<(String, Vec<Hit>)>, cx: &mut Context<Self>) {
        if crate::backend::Backend::of(&self.workspace, &self.root, cx).is_remote() {
            self.notice = Some("replace works in projects of this computer".into());
            cx.notify();
            return;
        }
        let request = self.request(cx);
        if request.template.is_none() || targets.is_empty() {
            return;
        }
        let (panel, root) = (cx.entity(), self.root.clone());
        in_window(&self.workspace, cx, move |view, _, cx| {
            let (targets, skipped): (Vec<_>, Vec<_>) =
                targets.into_iter().partition(|(path, _)| !crate::file_view::is_dirty(view, &root, path, cx));
            panel.update(cx, |panel, cx| panel.write(targets, skipped.len(), request, cx));
        });
    }

    fn write(&mut self, targets: Vec<(String, Vec<Hit>)>, skipped: usize, request: Request, cx: &mut Context<Self>) {
        let (root, workspace) = (self.root.clone(), self.workspace.clone());
        let progress = store::begin_progress("replacing", cx);
        cx.spawn(async move |this, cx| {
            let (count, files, errors) = cx
                .background_executor()
                .spawn(async move {
                    let (mut count, mut files, mut errors) = (0, 0, Vec::new());
                    for (path, hits) in &targets {
                        match search::replace(&root.join(path), &request, hits) {
                            Ok(0) => {}
                            Ok(changed) => (count, files) = (count + changed, files + 1),
                            Err(error) => errors.push(format!("{path}: {error}")),
                        }
                    }
                    (count, files, errors)
                })
                .await;
            let _ = this.update(cx, |panel, cx| {
                store::finish_progress(progress, cx);
                let mut notice = format!("replaced {} in {}", count, plural(files, "file"));
                if skipped > 0 {
                    notice.push_str(&format!(", skipped {} with unsaved edits", plural(skipped, "file")));
                }
                panel.notice = Some(notice);
                if !errors.is_empty() {
                    crate::actions::alert(&workspace, "Replace failed", errors.join("\n"), cx);
                }
                panel.run(cx);
            });
        })
        .detach();
    }

    fn replace_all(&mut self, cx: &mut Context<Self>) {
        let count: usize = self.files.iter().map(|file| file.found.len()).sum();
        if count == 0 || !options(cx).replace {
            return;
        }
        let targets = targets(&self.files);
        let with = self.replacement.read(cx).text.clone();
        let detail = format!(
            "Replace {} across {} with \"{with}\"?",
            plural(count, "occurrence"),
            plural(targets.len(), "file")
        );
        let Some(handle) = store::store(cx).read(cx).windows.get(&self.workspace).copied() else { return };
        let panel = cx.entity();
        cx.defer(move |cx| {
            let answer = handle.update(cx, |_, window, cx| {
                window.prompt(PromptLevel::Warning, "Replace all?", Some(&detail), &["Replace", "Cancel"], cx)
            });
            let Ok(answer) = answer else { return };
            cx.spawn(async move |cx| {
                if answer.await == Ok(0) {
                    panel.update(cx, |panel, cx| panel.replace(targets, cx));
                }
            })
            .detach();
        });
    }

    fn key(&mut self, event: &gpui::KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keys = &event.keystroke.modifiers;
        let primary = if cfg!(target_os = "macos") { keys.platform } else { keys.control };
        if event.keystroke.key == "enter" && primary {
            self.replace_all(cx);
            cx.stop_propagation();
            return;
        }
        if !(keys.alt && primary) {
            return;
        }
        match event.keystroke.key.as_str() {
            "c" => self.change(|saved| saved.find.case = !saved.find.case, cx),
            "w" => self.change(|saved| saved.find.word = !saved.find.word, cx),
            "r" => self.change(|saved| saved.find.regex = !saved.find.regex, cx),
            "p" => self.change(|saved| saved.find.preserve = !saved.find.preserve, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn summary(&self, theme: &Theme) -> (String, gpui::Hsla) {
        let count: usize = self.files.iter().map(|file| file.found.len()).sum();
        if let Some(error) = &self.error {
            (error.lines().next().unwrap_or("invalid").to_string(), theme.red)
        } else if let Some(notice) = &self.notice {
            (notice.clone(), theme.green)
        } else if self.searching {
            ("searching…".to_string(), theme.muted)
        } else if !self.searched {
            (String::new(), theme.muted)
        } else if count == 0 {
            ("no results".to_string(), theme.orange)
        } else {
            let capped = if self.capped { format!(" (first {})", search::LIMIT) } else { String::new() };
            (format!("{} in {}{capped}", plural(count, "result"), plural(self.files.len(), "file")), theme.muted)
        }
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui::theme(cx);
        let saved = options(cx);
        let size = settings::get(cx).ui_font_size();
        let height = px((size * 1.6).round() + 2.0);
        let field = |input: &Entity<TextInput>, invalid: bool| {
            div()
                .flex_1()
                .min_w_0()
                .h(height)
                .px(px(4.0))
                .bg(theme.background)
                .border_1()
                .border_color(if invalid { theme.red } else { theme.border })
                .child(input.clone())
        };
        let line = || div().flex().items_center().gap(px(4.0));
        let gutter = || div().flex_none().w(px(16.0));
        let controls = || div().flex().flex_none().items_center().gap(px(2.0)).w(px((size * 6.0).round()));
        let chevron = if saved.replace { icons::CHEVRON_DOWN } else { icons::CHEVRON_RIGHT };
        let find_line = line()
            .child(
                find_button("search-expand", chevron, "Toggle Replace", &theme)
                    .w(px(16.0))
                    .on_click(cx.listener(|panel, _, _, cx| panel.change(|saved| saved.replace = !saved.replace, cx))),
            )
            .child(field(&self.query, self.error.is_some()))
            .child(
                controls()
                    .child(find_toggle("search-case", "Aa", "Match Case", saved.find.case, &theme).on_click(
                        cx.listener(|panel, _, _, cx| panel.change(|saved| saved.find.case = !saved.find.case, cx)),
                    ))
                    .child(find_toggle("search-word", "ab", "Match Whole Word", saved.find.word, &theme).on_click(
                        cx.listener(|panel, _, _, cx| panel.change(|saved| saved.find.word = !saved.find.word, cx)),
                    ))
                    .child(
                        find_toggle("search-regex", ".*", "Use Regular Expression", saved.find.regex, &theme).on_click(
                            cx.listener(|panel, _, _, cx| {
                                panel.change(|saved| saved.find.regex = !saved.find.regex, cx)
                            }),
                        ),
                    ),
            );
        let replace_line = saved.replace.then(|| {
            line().child(gutter()).child(field(&self.replacement, false)).child(
                controls()
                    .child(find_toggle("search-preserve", "AB", "Preserve Case", saved.find.preserve, &theme).on_click(
                        cx.listener(|panel, _, _, cx| {
                            panel.change(|saved| saved.find.preserve = !saved.find.preserve, cx)
                        }),
                    ))
                    .child(
                        find_button("search-replace-all", icons::REPLACE_ALL, "Replace All", &theme)
                            .on_click(cx.listener(|panel, _, _, cx| panel.replace_all(cx))),
                    ),
            )
        });
        let details = saved.details.then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(line().child(gutter()).child(field(&self.include, false)))
                .child(line().child(gutter()).child(field(&self.exclude, false)))
        });
        let (summary, color) = self.summary(&theme);
        let collapse = if self.collapsed.is_empty() { icons::COLLAPSE_ALL } else { icons::EXPAND_ALL };
        let status = line()
            .child(gutter())
            .child(div().flex_1().min_w_0().truncate().text_color(color).child(summary))
            .child(
                find_toggle("search-details", icons::ELLIPSIS, "Toggle Search Details", saved.details, &theme)
                    .on_click(cx.listener(|panel, _, _, cx| panel.change(|saved| saved.details = !saved.details, cx))),
            )
            .child(
                find_button("search-refresh", icons::REFRESH, "Refresh", &theme)
                    .on_click(cx.listener(|panel, _, _, cx| panel.run(cx))),
            )
            .child(
                find_button("search-collapse", collapse, "Collapse or Expand All", &theme)
                    .on_click(cx.listener(|panel, _, _, cx| panel.toggle_all(cx))),
            )
            .child(
                find_button("search-clear", icons::CLEAR_ALL, "Clear Search Results", &theme)
                    .on_click(cx.listener(|panel, _, _, cx| panel.clear(cx))),
            );
        div()
            .flex()
            .flex_none()
            .flex_col()
            .gap(px(4.0))
            .px(px(8.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(theme.border)
            .child(find_line)
            .children(replace_line)
            .children(details)
            .child(status)
    }

    fn file_row(&self, index: usize, file: usize, replacing: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = ui::theme(cx);
        let hits = &self.files[file];
        let (folder, name) = match hits.path.rsplit_once('/') {
            Some((folder, name)) => (folder.to_string(), name.to_string()),
            None => (String::new(), hits.path.clone()),
        };
        let (glyph, color) = icons::file(&name);
        let chevron = if self.collapsed.contains(&hits.path) { icons::CHEVRON_RIGHT } else { icons::CHEVRON_DOWN };
        let group = SharedString::from(format!("search-file-{index}"));
        let dragged = DraggedPath { path: self.root.join(&hits.path), title: name.clone() };
        let (full, relative) = (self.root.join(&hits.path), hits.path.clone());
        let replace_targets = targets(std::slice::from_ref(hits));
        div()
            .id(("search-row", index))
            .group(group.clone())
            .w_full()
            .h_full()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(8.0))
            .hover(|style| style.bg(theme.hover))
            .child(div().flex_none().text_color(theme.faint).child(chevron))
            .child(div().flex_none().text_color(color.color(theme.dark)).child(glyph))
            .child(div().flex_none().max_w(gpui::relative(0.7)).truncate().child(name))
            .child(div().flex_1().min_w_0().truncate().text_color(theme.faint).child(folder))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .invisible()
                    .group_hover(group, |style| style.visible())
                    .when(replacing, |this| {
                        this.child(
                            find_button("search-replace-file", icons::REPLACE_ALL, "Replace All in File", &theme)
                                .on_click(cx.listener(move |panel, _, _, cx| {
                                    panel.replace(replace_targets.clone(), cx);
                                    cx.stop_propagation();
                                })),
                        )
                    })
                    .child(find_button("search-dismiss-file", icons::CLOSE, "Dismiss", &theme).on_click(cx.listener(
                        move |panel, _, _, cx| {
                            panel.dismiss(file, None, cx);
                            cx.stop_propagation();
                        },
                    ))),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(4.0))
                    .bg(theme.border)
                    .text_color(theme.text)
                    .child(hits.found.len().to_string()),
            )
            .on_drag(dragged, |drag: &DraggedPath, _, _, cx| {
                cx.new(|_| crate::sidebar::DragPreview(drag.title.clone()))
            })
            .on_click(cx.listener(move |panel, _, _, cx| panel.toggle(file, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |panel, event: &gpui::MouseDownEvent, _, cx| {
                    let (entity, full, relative, position) =
                        (cx.entity(), full.clone(), relative.clone(), event.position);
                    let targets = targets(std::slice::from_ref(&panel.files[file]));
                    let replacing = options(cx).replace;
                    cx.stop_propagation();
                    in_window(&panel.workspace, cx, move |view, _, cx| {
                        let mut items = Vec::new();
                        if replacing {
                            let (entity, targets) = (entity.clone(), targets.clone());
                            items.push(menu::action("Replace All in File", move |_, _, cx| {
                                entity.update(cx, |panel, cx| panel.replace(targets.clone(), cx))
                            }));
                        }
                        let dismiss = entity.clone();
                        items.push(menu::action("Dismiss", move |_, _, cx| {
                            dismiss.update(cx, |panel, cx| panel.dismiss(file, None, cx))
                        }));
                        items.push(menu::Item::Separator);
                        items.extend(crate::file_view::file_menu(view, &full, &relative, cx));
                        view.open_menu(position, items, cx);
                    });
                }),
            )
            .into_any_element()
    }

    fn match_row(
        &self,
        index: usize,
        file: usize,
        found: usize,
        replacing: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = ui::theme(cx);
        let item = &self.files[file].found[found];
        let (text, highlights) = preview(item, &theme);
        let group = SharedString::from(format!("search-match-{index}"));
        let target = vec![(self.files[file].path.clone(), vec![item.hit])];
        let line_number = format!("{}", item.hit.start.row + 1);
        div()
            .id(("search-row", index))
            .group(group.clone())
            .w_full()
            .h_full()
            .flex()
            .items_center()
            .gap(px(6.0))
            .pl(px(30.0))
            .pr(px(8.0))
            .when(self.current == Some((file, found)), |this| this.bg(theme.selection))
            .hover(|style| style.bg(theme.hover))
            .child(div().flex_1().min_w_0().truncate().child(StyledText::new(text).with_highlights(highlights)))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .invisible()
                    .group_hover(group, |style| style.visible())
                    .when(replacing, |this| {
                        this.child(find_button("search-replace-match", icons::REPLACE, "Replace", &theme).on_click(
                            cx.listener(move |panel, _, _, cx| {
                                panel.replace(target.clone(), cx);
                                cx.stop_propagation();
                            }),
                        ))
                    })
                    .child(find_button("search-dismiss-match", icons::CLOSE, "Dismiss", &theme).on_click(cx.listener(
                        move |panel, _, _, cx| {
                            panel.dismiss(file, Some(found), cx);
                            cx.stop_propagation();
                        },
                    ))),
            )
            .child(div().flex_none().text_color(theme.faint).child(line_number))
            .on_click(cx.listener(move |panel, _, _, cx| panel.open(file, found, cx)))
            .into_any_element()
    }
}

fn preview(found: &Found, theme: &Theme) -> (String, Vec<(Range<usize>, HighlightStyle)>) {
    let Some(replacement) = &found.replacement else {
        let style = HighlightStyle { background_color: Some(theme.yellow.opacity(0.35)), ..Default::default() };
        return (found.preview.clone(), vec![(found.range.clone(), style)]);
    };
    let replacement = replacement.replace('\n', "↵");
    let (before, after) = (&found.preview[..found.range.end], &found.preview[found.range.end..]);
    let end = before.len() + replacement.len();
    let removed = HighlightStyle {
        background_color: Some(theme.removed),
        strikethrough: Some(StrikethroughStyle { thickness: px(1.0), color: Some(theme.red) }),
        ..Default::default()
    };
    let added = HighlightStyle { background_color: Some(theme.added), ..Default::default() };
    let ranges = [(found.range.clone(), removed), (before.len()..end, added)];
    (format!("{before}{replacement}{after}"), ranges.into_iter().filter(|(range, _)| !range.is_empty()).collect())
}

impl Render for SearchPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let line = px((settings::get(cx).ui_font_size() * 1.7).round());
        let replacing = options(cx).replace;
        let list = uniform_list(
            "search-results",
            self.rows.len(),
            cx.processor(move |panel: &mut SearchPanel, range: Range<usize>, _, cx| {
                range
                    .map(|index| {
                        let row = match panel.rows[index] {
                            Row::File(file) => panel.file_row(index, file, replacing, cx),
                            Row::Match(file, found) => panel.match_row(index, file, found, replacing, cx),
                        };
                        div().h(line).w_full().child(row).into_any_element()
                    })
                    .collect::<Vec<AnyElement>>()
            }),
        )
        .track_scroll(&self.scroll)
        .size_full();
        div()
            .id("search")
            .key_context("SearchView")
            .size_full()
            .flex()
            .flex_col()
            .on_key_down(cx.listener(Self::key))
            .child(self.header(cx))
            .child(div().flex_1().min_h_0().child(list))
    }
}

fn entity(
    view: &mut WorkspaceView,
    path: &Path,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> Entity<SearchPanel> {
    let entity = match view.panel.search.clone() {
        Some(entity) => entity,
        None => {
            let (workspace, root) = (view.workspace.clone(), path.to_path_buf());
            let entity = cx.new(|cx| SearchPanel::new(workspace, root, window, cx));
            view.panel.search = Some(entity.clone());
            entity
        }
    };
    entity.update(cx, |panel, cx| panel.set_root(path, cx));
    entity
}

pub fn panel(
    view: &mut WorkspaceView,
    path: &Path,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    entity(view, path, window, cx).into_any_element()
}

pub fn open(view: &mut WorkspaceView, replace: bool, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let seed = crate::file_view::active_editor(view, cx)
        .filter(|editor| editor.read(cx).focus.is_focused(window))
        .and_then(|editor| editor.read(cx).selected_text())
        .filter(|text| !text.is_empty() && !text.contains('\n'));
    crate::docks_view::show(View::Search, cx);
    if replace {
        store::store(cx).update(cx, |store, cx| {
            store.ui.search.replace = true;
            store.changed(cx);
        });
    }
    let Some(path) = view.current_project_path(cx).filter(|path| !path.as_os_str().is_empty()) else { return };
    let entity = entity(view, &path, window, cx);
    entity.update(cx, |panel, cx| panel.focus(seed, replace, window, cx));
}

pub fn step(view: &mut WorkspaceView, delta: i32, cx: &mut Context<WorkspaceView>) {
    if let Some(entity) = view.panel.search.clone() {
        entity.update(cx, |panel, cx| panel.step(delta, cx));
    }
}

use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term};
use futures::StreamExt;
use gpui::{
    App, Bounds, Context, ElementInputHandler, Entity, EntityInputHandler, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, Keystroke, Modifiers, ParentElement, Pixels, Point, Render, Size,
    StatefulInteractiveElement, Styled, UTF16Selection, Window, canvas, div, point, px, size,
};
use nucli_core::fonts::FAMILY;
use nucli_core::terminal::render::{self, Frame, Look};
use nucli_core::terminal::{keys, spawn_mirror};
use nucli_core::uplink::wire::Key;

use crate::link::Link;

const FONT_SIZE: f32 = 13.0;
const LINE_HEIGHT: f32 = 1.3;

pub struct Mirror {
    pub key: Key,
    pub tab: String,
    link: Entity<Link>,
    term: Arc<FairMutex<Term<VoidListener>>>,
    focus: FocusHandle,
    marked: Option<String>,
    fit: (usize, usize),
    held: bool,
    pub ctrl: bool,
    grid: Option<(Point<Pixels>, Size<Pixels>)>,
}

impl Mirror {
    pub fn new(link: Entity<Link>, key: Key, tab: String, cx: &mut Context<Self>) -> Mirror {
        let term = Arc::new(FairMutex::new(Term::new(Config::default(), &TermSize::new(80, 24), VoidListener)));
        let (wake, mut woken) = futures::channel::mpsc::unbounded::<()>();
        let pending = Arc::new(AtomicBool::new(false));
        let waking = pending.clone();
        let feeds = spawn_mirror(
            term.clone(),
            |term, cols, rows| term.resize(TermSize::new(cols.clamp(2, 500), rows.clamp(1, 200))),
            move |_| {
                if !waking.swap(true, Ordering::AcqRel) {
                    let _ = wake.unbounded_send(());
                }
            },
        );
        cx.spawn(async move |this, cx| {
            while woken.next().await.is_some() {
                pending.store(false, Ordering::Release);
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        link.update(cx, |link, _| link.watch(key, &tab, feeds));
        Mirror {
            key,
            tab,
            link,
            term,
            focus: cx.focus_handle(),
            marked: None,
            fit: (0, 0),
            held: false,
            ctrl: false,
            grid: None,
        }
    }

    pub fn press(&mut self, key: &str, cx: &mut Context<Self>) {
        let modifiers = Modifiers { control: std::mem::take(&mut self.ctrl), ..Default::default() };
        self.encode(&Keystroke { modifiers, key: key.into(), key_char: None }, cx);
    }

    fn encode(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        let mode = *self.term.lock().mode();
        let Some(bytes) = keys::encode(keystroke, mode, false) else { return false };
        self.send(&bytes, cx);
        true
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus, cx);
    }

    pub fn type_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let mut characters = text.chars();
        if self.ctrl
            && let (Some(character), None) = (characters.next(), characters.next())
        {
            self.press(&character.to_lowercase().to_string(), cx);
            return;
        }
        self.ctrl = false;
        let text = text.replace('\n', "\r");
        self.send(text.as_bytes(), cx);
    }

    fn send(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        let size = {
            let mut term = self.term.lock();
            term.scroll_display(Scroll::Bottom);
            (term.columns(), term.screen_lines())
        };
        if self.fit.0 > 0 && (!self.held || size != self.fit) {
            self.link.read(cx).resize(self.key, &self.tab, self.fit.0, self.fit.1);
        }
        self.held = true;
        self.link.read(cx).input(self.key, &self.tab, bytes);
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.encode(&event.keystroke, cx) {
            cx.stop_propagation();
        }
    }
}

fn layout(mirror: &Entity<Mirror>, bounds: Bounds<Pixels>, focused: bool, window: &mut Window, cx: &mut App) -> Frame {
    let theme = crate::theme(window);
    let font_id = window.text_system().resolve_font(&render::font(FAMILY, false, false, true));
    let full = px(FONT_SIZE);
    let advance = f32::from(window.text_system().advance(font_id, full, 'm').map_or(full * 0.6, |size| size.width));
    let line = (FONT_SIZE * LINE_HEIGHT).round();
    let width = f32::from(bounds.size.width);
    let fit =
        ((width / advance).floor().max(2.0) as usize, (f32::from(bounds.size.height) / line).floor().max(1.0) as usize);
    let (term, held, marked) = mirror.update(cx, |mirror, cx| {
        if mirror.held && mirror.fit != fit {
            mirror.link.read(cx).resize(mirror.key, &mirror.tab, fit.0, fit.1);
        }
        mirror.fit = fit;
        (mirror.term.clone(), mirror.held, mirror.marked.clone())
    });
    let (cols, rows) = {
        let term = term.lock();
        (term.columns(), term.screen_lines())
    };
    let scale = if held { 1.0 } else { (width / (advance * cols as f32)).min(1.0) };
    let cell = size(px(advance * scale), px(line * scale));
    let height = cell.height * rows as f32;
    let top = if height > bounds.size.height { bounds.bottom() - height } else { bounds.origin.y };
    let grid = Bounds::new(point(bounds.origin.x, top), size(cell.width * cols as f32, height));
    mirror.update(cx, |mirror, _| mirror.grid = Some((grid.origin, cell)));
    let look = Look {
        theme: &theme,
        family: FAMILY,
        ligatures: true,
        font_size: full * scale,
        cell,
        focused,
        cursor_hidden: false,
        marked,
        hover: None,
        found: &[],
        current: 0,
    };
    render::layout(&term, rows, grid, look, window)
}

impl Render for Mirror {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme(window);
        let focused = self.focus.is_focused(window);
        let (laying, painting, focus) = (cx.entity(), cx.entity(), self.focus.clone());
        div()
            .id("mirror")
            .size_full()
            .overflow_hidden()
            .bg(theme.background)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_click(cx.listener(|mirror, _, window, cx| mirror.focus(window, cx)))
            .child(
                canvas(
                    move |bounds, window, cx| layout(&laying, bounds, focused, window, cx),
                    move |bounds, mut frame, window, cx| {
                        window.handle_input(&focus, ElementInputHandler::new(bounds, painting), cx);
                        render::paint(&mut frame, bounds, window, cx);
                    },
                )
                .size_full(),
            )
    }
}

impl EntityInputHandler for Mirror {
    fn text_for_range(
        &mut self,
        _range: Range<usize>,
        _adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: 0..0, reversed: false })
    }

    fn marked_text_range(&self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|text| 0..text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = None;
        if !text.is_empty() {
            self.type_text(text, cx);
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _selected: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = Some(text.to_string()).filter(|text| !text.is_empty());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let (origin, cell) = self.grid?;
        let term = self.term.lock();
        let cursor = term.grid().cursor.point;
        let offset = term.grid().display_offset() as i32;
        let position = point(
            origin.x + cell.width * cursor.column.0 as f32,
            origin.y + cell.height * (cursor.line.0 + offset) as f32,
        );
        Some(Bounds::new(position, cell))
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

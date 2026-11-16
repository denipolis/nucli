use std::cell::Cell;

use gpui::prelude::FluentBuilder;
use gpui::{
    Bounds, Context, Div, InteractiveElement, MouseDownEvent, ParentElement, Pixels, ScrollEvent, ScrollSource,
    SharedString, Stateful, StatefulInteractiveElement, Styled, TouchPhase, Window, canvas, div, px,
};
use nucli_core::theme::Theme;

use crate::home::Home;
use crate::sheet::Run;

const ACTION: f32 = 88.0;
const FULL: f32 = 0.55;

pub struct Slide {
    id: SharedString,
    offset: f32,
    dragging: bool,
}

thread_local! {
    static OPEN: Cell<Option<Bounds<Pixels>>> = const { Cell::new(None) };
}

impl Home {
    pub fn swipeable(
        &self,
        id: SharedString,
        row: Stateful<Div>,
        label: &'static str,
        run: Run,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let slide = self.slide.as_ref().filter(|slide| slide.id == id);
        let (offset, settled) = slide.map_or((0.0, false), |slide| (slide.offset, !slide.dragging));
        let (slid, tapped) = (run.clone(), run);
        let scrolled = id.clone();
        div()
            .relative()
            .overflow_hidden()
            .on_scroll(
                cx.listener(move |home, event: &ScrollEvent, window, cx| {
                    home.slid(&scrolled, &slid, event, window, cx)
                }),
            )
            .when(offset < 0.0, |this| {
                this.child(canvas(|bounds, _, _| OPEN.set(Some(bounds)), |_, _, _, _| {}).absolute().inset_0()).child(
                    div()
                        .id(SharedString::from(format!("swipe:{id}")))
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right_0()
                        .w(px(ACTION.max(-offset)))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(theme.red)
                        .text_color(theme.background)
                        .child(label)
                        .on_click(cx.listener(move |home, _, window, cx| {
                            home.slide = None;
                            tapped(home, window, cx);
                            cx.notify();
                        })),
                )
            })
            .child(row.bg(theme.background).relative().left(px(offset)))
            .when(settled, |this| {
                this.child(
                    div()
                        .id(SharedString::from(format!("cover:{id}")))
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left_0()
                        .right(px(ACTION.max(-offset)))
                        .occlude()
                        .on_click(cx.listener(|home, _, _, cx| home.unslide(cx))),
                )
            })
    }

    pub fn unslide(&mut self, cx: &mut Context<Self>) {
        if self.slide.take().is_some() {
            cx.notify();
        }
    }

    pub fn tapped_outside(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.slide.is_some() && !OPEN.get().is_some_and(|bounds| bounds.contains(&event.position)) {
            self.unslide(cx);
            cx.stop_propagation();
        }
    }

    fn slid(&mut self, id: &SharedString, run: &Run, event: &ScrollEvent, window: &mut Window, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(px(1.0));
        if event.source != ScrollSource::Touch || delta.y != px(0.0) {
            if event.touch_phase == TouchPhase::Started {
                self.unslide(cx);
            }
            return;
        }
        let width = f32::from(window.viewport_size().width);
        let start = self.slide.as_ref().filter(|slide| slide.id == *id).map_or(0.0, |slide| slide.offset);
        let dragging = self.slide.as_ref().is_some_and(|slide| slide.id == *id && slide.dragging);
        let offset = (start + f32::from(delta.x)).clamp(-width, 0.0);
        match event.touch_phase {
            TouchPhase::Started if delta.x != px(0.0) => {
                self.slide = Some(Slide { id: id.clone(), offset, dragging: true });
            }
            TouchPhase::Moved if dragging => {
                self.slide = Some(Slide { id: id.clone(), offset, dragging: true });
            }
            TouchPhase::Ended if dragging => {
                self.slide =
                    (offset < -ACTION / 2.0).then(|| Slide { id: id.clone(), offset: -ACTION, dragging: false });
                if offset < -width * FULL {
                    self.slide = None;
                    run(self, window, cx);
                }
            }
            _ => {}
        }
        cx.stop_propagation();
        cx.notify();
    }
}

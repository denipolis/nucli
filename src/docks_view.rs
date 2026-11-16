use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, Context, Div, InteractiveElement, IntoElement, MouseButton, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, Window, canvas, div, px, relative,
};

use crate::docks::Target;
use crate::menu;
use crate::persist::{Dock, Docks, Panel, Stack, View};
use crate::resize::{Drag, handle_horizontal, handle_vertical};
use crate::settings;
use crate::sidebar::DragPreview;
use crate::store;
use crate::ui;
use crate::window::WorkspaceView;

#[derive(Clone)]
pub struct DraggedView(pub View);

pub type PanelBounds =
    std::rc::Rc<std::cell::RefCell<std::collections::HashMap<(Dock, usize, usize), gpui::Bounds<gpui::Pixels>>>>;

fn available(view: View, cx: &App) -> bool {
    view != View::Uplink || crate::uplink::Uplink::enabled(cx)
}

pub fn update(cx: &mut App, change: impl FnOnce(&mut Docks)) {
    store::store(cx).update(cx, |store, cx| {
        change(&mut store.ui.docks);
        store.changed(cx);
    });
}

pub fn show(view: View, cx: &mut App) {
    store::store(cx).update(cx, |store, cx| {
        store.ui.docks.show(view);
        if view == View::History {
            store.refresh_history(cx);
        }
        store.changed(cx);
    });
}

fn expand_chevron(dock: Dock) -> &'static str {
    match dock {
        Dock::Left => Dock::Right.chevron(),
        Dock::Right => Dock::Left.chevron(),
        Dock::Top => Dock::Bottom.chevron(),
        Dock::Bottom => Dock::Top.chevron(),
    }
}

fn strip_width(cx: &App) -> f32 {
    (settings::get(cx).ui_font_size() * 2.2).round()
}

pub fn render(
    view: &mut WorkspaceView,
    viewport: AnyElement,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    let docks = view.store(cx).ui.docks.clone();
    let left = dock_elements(view, Dock::Left, &docks, window, cx);
    let right = dock_elements(view, Dock::Right, &docks, window, cx);
    let top = dock_elements(view, Dock::Top, &docks, window, cx);
    let bottom = dock_elements(view, Dock::Bottom, &docks, window, cx);
    let viewport = div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .w_full()
        .child(viewport)
        .when(view.view_drag, |this| this.child(viewport_zones(&docks, cx)));
    let center = div().flex().flex_col().flex_1().min_w_0().h_full().children(top).child(viewport).children(bottom);
    div()
        .flex()
        .flex_row()
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .children(left)
        .child(center)
        .children(right)
        .into_any_element()
}

fn dock_elements(
    view: &mut WorkspaceView,
    dock: Dock,
    docks: &Docks,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> Vec<AnyElement> {
    let mut groups = Vec::new();
    for (index, stack) in docks.dock(dock).iter().enumerate() {
        let Some((element, collapsed)) = stack_element(view, dock, index, stack, window, cx) else { continue };
        let handle = (!collapsed).then(|| {
            if dock.side() {
                handle_horizontal(Drag::Stack(dock, index), cx)
            } else {
                handle_vertical(Drag::Stack(dock, index), cx)
            }
        });
        groups.push(match dock {
            Dock::Left | Dock::Top => std::iter::once(element).chain(handle).collect::<Vec<_>>(),
            Dock::Right | Dock::Bottom => handle.into_iter().chain(std::iter::once(element)).collect(),
        });
    }
    if matches!(dock, Dock::Right | Dock::Bottom) {
        groups.reverse();
    }
    groups.into_iter().flatten().collect()
}

fn stack_element(
    view: &mut WorkspaceView,
    dock: Dock,
    index: usize,
    stack: &Stack,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> Option<(AnyElement, bool)> {
    let theme = ui::theme(cx);
    let panels: Vec<(usize, &Panel, Vec<View>)> = stack
        .panels
        .iter()
        .enumerate()
        .map(|(position, panel)| {
            (position, panel, panel.views.iter().copied().filter(|item| available(*item, cx)).collect::<Vec<_>>())
        })
        .filter(|(_, _, views)| !views.is_empty())
        .collect();
    if panels.is_empty() {
        return None;
    }
    let base = div().flex().flex_none().bg(theme.surface).border_color(theme.border);
    let base = match dock {
        Dock::Left => base.border_r_1(),
        Dock::Right => base.border_l_1(),
        Dock::Top => base.border_b_1(),
        Dock::Bottom => base.border_t_1(),
    };
    if panels.iter().all(|(_, panel, _)| panel.collapsed) {
        return Some((strip(dock, index, &panels, base, cx).into_any_element(), true));
    }
    let mut element = if dock.side() {
        base.flex_col().h_full().w(px(stack.size))
    } else {
        base.flex_row().w_full().h(px(stack.size))
    };
    let mut previous: Option<(usize, bool)> = None;
    for (count, (position, panel, views)) in panels.iter().enumerate() {
        if let Some((before, expanded)) = previous
            && expanded
            && !panel.collapsed
            && before + 1 == *position
        {
            element = element.child(if dock.side() {
                handle_vertical(Drag::Panels(dock, index, before), cx)
            } else {
                handle_horizontal(Drag::Panels(dock, index, before), cx)
            });
        }
        let item = panel_element(view, dock, index, *position, panel, views, window, cx);
        element = element.child(
            div()
                .flex()
                .min_w_0()
                .min_h_0()
                .border_color(theme.border)
                .when(count > 0 && dock.side(), |this| this.border_t_1())
                .when(count > 0 && !dock.side(), |this| this.border_l_1())
                .when(panel.collapsed, |this| this.flex_none())
                .when(!panel.collapsed, |this| this.flex_grow(panel.weight).flex_basis(px(0.0)))
                .when(dock.side(), |this| this.w_full())
                .when(!dock.side(), |this| this.h_full())
                .child(item),
        );
        previous = Some((*position, !panel.collapsed));
    }
    Some((element.into_any_element(), false))
}

#[allow(clippy::too_many_arguments)]
fn panel_element(
    view: &mut WorkspaceView,
    dock: Dock,
    stack: usize,
    index: usize,
    panel: &Panel,
    views: &[View],
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    let active = if views.contains(&panel.active) { panel.active } else { views[0] };
    let bounds = view.dock_bounds.clone();
    let key = (dock, stack, index);
    let base = div()
        .id(SharedString::from(format!("panel-{}-{stack}-{index}", dock.title())))
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .overflow_hidden()
        .child(
            canvas(move |area, _, _| _ = bounds.borrow_mut().insert(key, area), |_, _, _, _| {})
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
        );
    if panel.collapsed && !dock.side() {
        return base.child(glyphs(dock, stack, index, views, active, cx)).into_any_element();
    }
    let header = header(view, dock, stack, index, panel, views, active, cx).into_any_element();
    if panel.collapsed {
        return base.child(header).into_any_element();
    }
    let content = if active.list() {
        crate::sidebar::render(view, active, cx)
    } else {
        crate::panel::render(view, active, window, cx)
    };
    base.child(header)
        .child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .child(content)
                .when(view.view_drag, |this| this.child(panel_zones(dock, stack, index, cx))),
        )
        .into_any_element()
}

fn drop_fill(cx: &App) -> gpui::Hsla {
    ui::theme(cx).accent.opacity(0.3)
}

fn tab(
    dock: Dock,
    stack: usize,
    index: usize,
    kind: View,
    active: bool,
    label: bool,
    cx: &mut Context<WorkspaceView>,
) -> gpui::Stateful<Div> {
    let theme = ui::theme(cx);
    let fill = drop_fill(cx);
    div()
        .id(SharedString::from(format!("view-{kind:?}")))
        .occlude()
        .flex()
        .flex_none()
        .gap(px(8.0))
        .px(px(4.0))
        .when(active, |this| this.bg(theme.accent).text_color(theme.on_accent))
        .when(!active, |this| this.text_color(theme.muted).hover(|style| style.text_color(theme.text)))
        .child(kind.glyph())
        .when(label, |this| this.child(kind.title()))
        .on_click(move |_, window, cx| {
            show(kind, cx);
            window.refresh();
        })
        .on_drag(DraggedView(kind), |drag: &DraggedView, _, _, cx| {
            cx.new(|_| DragPreview(format!("{} {}", drag.0.glyph(), drag.0.title())))
        })
        .drag_over::<DraggedView>(move |style, _, _, _| style.bg(fill))
        .on_drop(move |drag: &DraggedView, _, cx| {
            update(cx, |docks| docks.move_view(drag.0, Target::Join { dock, stack, panel: index, before: Some(kind) }))
        })
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                cx.stop_propagation();
                view.open_menu(event.position, tab_menu(kind, dock, cx), cx);
            }),
        )
}

fn tab_menu(kind: View, dock: Dock, cx: &App) -> Vec<menu::Item> {
    let mut items: Vec<menu::Item> = Dock::ALL
        .into_iter()
        .filter(|target| *target != dock)
        .map(|target| {
            menu::action(format!("Move to {}", target.title()), move |_, _, cx| {
                update(cx, |docks| docks.move_to_dock(kind, target))
            })
        })
        .collect();
    items.push(menu::Item::Separator);
    items.push(menu::command("Reset Panel Layout", "reset_layout", cx));
    items
}

#[allow(clippy::too_many_arguments)]
fn header(
    view: &WorkspaceView,
    dock: Dock,
    stack: usize,
    index: usize,
    panel: &Panel,
    views: &[View],
    active: View,
    cx: &mut Context<WorkspaceView>,
) -> impl IntoElement {
    let theme = ui::theme(cx);
    let char_width = settings::get(cx).ui_font_size() * 0.6;
    let needed = 38.0
        + views.iter().map(|item| 16.0 + char_width * (1 + item.title().len()) as f32).sum::<f32>()
        + 6.0 * views.len().saturating_sub(1) as f32;
    let width = view.dock_bounds.borrow().get(&(dock, stack, index)).map(|bounds| f32::from(bounds.size.width));
    let compact = width.is_some_and(|width| width < needed);
    let fill = drop_fill(cx);
    let tabs: Vec<AnyElement> = views
        .iter()
        .map(|item| {
            let shown = *item == active && !panel.collapsed;
            tab(dock, stack, index, *item, shown, shown || !compact, cx).into_any_element()
        })
        .collect();
    let toggle = if panel.collapsed { expand_chevron(dock) } else { dock.chevron() };
    div()
        .id(SharedString::from(format!("panel-header-{}-{stack}-{index}", dock.title())))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.0))
        .px(px(8.0))
        .py(px(4.0))
        .overflow_hidden()
        .when(!panel.collapsed, |this| this.border_b_1().border_color(theme.border))
        .drag_over::<DraggedView>(move |style, _, _, _| style.bg(fill))
        .on_drop(move |drag: &DraggedView, _, cx| {
            update(cx, |docks| docks.move_view(drag.0, Target::Join { dock, stack, panel: index, before: None }))
        })
        .children(tabs)
        .child(div().flex_1())
        .child(
            div()
                .id(SharedString::from(format!("panel-collapse-{}-{stack}-{index}", dock.title())))
                .flex_none()
                .px(px(2.0))
                .text_color(theme.faint)
                .hover(|style| style.text_color(theme.accent))
                .child(toggle)
                .on_click(move |_, _, cx| {
                    update(cx, |docks| {
                        if let Some(panel) = docks.panel_mut(dock, stack, index) {
                            panel.collapsed = !panel.collapsed;
                        }
                    })
                }),
        )
}

fn glyphs(
    dock: Dock,
    stack: usize,
    index: usize,
    views: &[View],
    active: View,
    cx: &mut Context<WorkspaceView>,
) -> impl IntoElement {
    let theme = ui::theme(cx);
    let fill = drop_fill(cx);
    let items: Vec<AnyElement> = views
        .iter()
        .map(|item| {
            tab(dock, stack, index, *item, false, false, cx)
                .when(*item == active, |this| this.text_color(theme.text))
                .into_any_element()
        })
        .collect();
    div()
        .id(SharedString::from(format!("panel-glyphs-{}-{stack}-{index}", dock.title())))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(4.0))
        .py(px(6.0))
        .w(px(strip_width(cx)))
        .h_full()
        .drag_over::<DraggedView>(move |style, _, _, _| style.bg(fill))
        .on_drop(move |drag: &DraggedView, _, cx| {
            update(cx, |docks| docks.move_view(drag.0, Target::Join { dock, stack, panel: index, before: None }))
        })
        .child(
            div()
                .id(SharedString::from(format!("panel-expand-{}-{stack}-{index}", dock.title())))
                .text_color(theme.faint)
                .hover(|style| style.text_color(theme.accent))
                .child(expand_chevron(dock))
                .on_click(move |_, _, cx| {
                    update(cx, |docks| {
                        if let Some(panel) = docks.panel_mut(dock, stack, index) {
                            panel.collapsed = false;
                        }
                    })
                }),
        )
        .children(items)
}

fn strip(
    dock: Dock,
    stack: usize,
    panels: &[(usize, &Panel, Vec<View>)],
    base: Div,
    cx: &mut Context<WorkspaceView>,
) -> Div {
    let theme = ui::theme(cx);
    if dock.side() {
        let mut column = base.flex_col().h_full().w(px(strip_width(cx)));
        for (count, (index, panel, views)) in panels.iter().enumerate() {
            column =
                column.child(
                    div()
                        .flex_none()
                        .border_color(theme.border)
                        .when(count > 0, |this| this.border_t_1())
                        .child(glyphs(dock, stack, *index, views, panel.active, cx)),
                );
        }
        return column;
    }
    let fill = drop_fill(cx);
    let mut row = base.flex_row().w_full().items_center().gap(px(6.0)).px(px(8.0)).py(px(4.0));
    for (count, (index, _, views)) in panels.iter().enumerate() {
        let index = *index;
        let tabs: Vec<AnyElement> =
            views.iter().map(|item| tab(dock, stack, index, *item, false, true, cx).into_any_element()).collect();
        row = row.when(count > 0, |this| this.child(div().w(px(1.0)).h(px(14.0)).bg(theme.border))).child(
            div()
                .id(SharedString::from(format!("panel-strip-{}-{stack}-{index}", dock.title())))
                .flex()
                .gap(px(6.0))
                .drag_over::<DraggedView>(move |style, _, _, _| style.bg(fill))
                .on_drop(move |drag: &DraggedView, _, cx| {
                    update(cx, |docks| {
                        docks.move_view(drag.0, Target::Join { dock, stack, panel: index, before: None })
                    })
                })
                .children(tabs),
        );
    }
    row.child(div().flex_1()).child(
        div()
            .id(SharedString::from(format!("stack-expand-{}-{stack}", dock.title())))
            .flex_none()
            .px(px(2.0))
            .text_color(theme.faint)
            .hover(|style| style.text_color(theme.accent))
            .child(expand_chevron(dock))
            .on_click(move |_, _, cx| {
                update(cx, |docks| {
                    if let Some(stack) = docks.dock_mut(dock).get_mut(stack) {
                        for panel in &mut stack.panels {
                            panel.collapsed = false;
                        }
                    }
                })
            }),
    )
}

fn zone(target: Target, fill: gpui::Hsla) -> Div {
    div()
        .absolute()
        .occlude()
        .drag_over::<DraggedView>(move |style, _, _, _| style.bg(fill))
        .on_drop(move |drag: &DraggedView, _, cx| update(cx, |docks| docks.move_view(drag.0, target)))
}

fn panel_zones(dock: Dock, stack: usize, index: usize, cx: &App) -> impl IntoElement {
    let fill = drop_fill(cx);
    let new_panel = |panel| Target::NewPanel { dock, stack, panel };
    let (start_stack, end_stack) = match dock {
        Dock::Left | Dock::Top => (stack, stack + 1),
        Dock::Right | Dock::Bottom => (stack + 1, stack),
    };
    let new_stack = |stack| Target::NewStack { dock, stack };
    let center = zone(Target::Join { dock, stack, panel: index, before: None }, fill).top_0().left_0().size_full();
    let (along_start, along_end, across_start, across_end) = if dock.side() {
        (
            zone(new_panel(index), fill).top_0().left_0().w_full().h(relative(0.3)),
            zone(new_panel(index + 1), fill).bottom_0().left_0().w_full().h(relative(0.3)),
            zone(new_stack(start_stack), fill).top_0().left_0().h_full().w(relative(0.2)),
            zone(new_stack(end_stack), fill).top_0().right_0().h_full().w(relative(0.2)),
        )
    } else {
        (
            zone(new_panel(index), fill).top_0().left_0().h_full().w(relative(0.3)),
            zone(new_panel(index + 1), fill).top_0().right_0().h_full().w(relative(0.3)),
            zone(new_stack(start_stack), fill).top_0().left_0().w_full().h(relative(0.2)),
            zone(new_stack(end_stack), fill).bottom_0().left_0().w_full().h(relative(0.2)),
        )
    };
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(center)
        .child(along_start)
        .child(along_end)
        .child(across_start)
        .child(across_end)
}

fn viewport_zones(docks: &Docks, cx: &App) -> impl IntoElement {
    let fill = drop_fill(cx);
    let target = |dock: Dock| Target::NewStack { dock, stack: docks.dock(dock).len() };
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(zone(target(Dock::Top), fill).top_0().left_0().w_full().h(relative(0.15)))
        .child(zone(target(Dock::Bottom), fill).bottom_0().left_0().w_full().h(relative(0.15)))
        .child(zone(target(Dock::Left), fill).top_0().left_0().h_full().w(relative(0.15)))
        .child(zone(target(Dock::Right), fill).top_0().right_0().h_full().w(relative(0.15)))
}

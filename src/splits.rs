use crate::persist::Layout;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Zone {
    Center,
    Edge(Direction),
    Outer(Direction),
}

const OUTER_BAND: f32 = 28.0;

pub fn zone(pane: Rect, area: Rect, x: f32, y: f32) -> Zone {
    let whole = pane.w >= area.w - 2.0 && pane.h >= area.h - 2.0;
    let outer = [
        (x - area.x, Direction::Left),
        (area.x + area.w - x, Direction::Right),
        (y - area.y, Direction::Up),
        (area.y + area.h - y, Direction::Down),
    ]
    .into_iter()
    .filter(|(distance, _)| !whole && *distance < OUTER_BAND)
    .min_by(|a, b| a.0.total_cmp(&b.0))
    .map(|(_, direction)| direction);
    match (outer, edge((x - pane.x) / pane.w, (y - pane.y) / pane.h)) {
        (Some(direction), _) => Zone::Outer(direction),
        (None, Some(direction)) => Zone::Edge(direction),
        (None, None) => Zone::Center,
    }
}

fn axis(direction: Direction) -> (bool, bool) {
    (matches!(direction, Direction::Left | Direction::Right), matches!(direction, Direction::Left | Direction::Up))
}

pub fn tabs(layout: &Layout) -> Vec<String> {
    match layout {
        Layout::Pane { tab } => vec![tab.clone()],
        Layout::Row { children } | Layout::Column { children } => children.iter().flat_map(tabs).collect(),
    }
}

pub fn group(layouts: &[Layout], id: &str) -> Option<usize> {
    layouts.iter().position(|layout| tabs(layout).iter().any(|tab| tab == id))
}

fn prune(layout: Layout, id: &str) -> Option<Layout> {
    match layout {
        Layout::Pane { tab } => (tab != id).then_some(Layout::Pane { tab }),
        Layout::Row { children } => collapse(children.into_iter().filter_map(|child| prune(child, id)).collect(), true),
        Layout::Column { children } => {
            collapse(children.into_iter().filter_map(|child| prune(child, id)).collect(), false)
        }
    }
}

fn collapse(mut children: Vec<Layout>, row: bool) -> Option<Layout> {
    match children.len() {
        0 => None,
        1 => children.pop(),
        _ => Some(if row { Layout::Row { children } } else { Layout::Column { children } }),
    }
}

pub fn remove(layouts: &mut Vec<Layout>, id: &str) {
    *layouts = std::mem::take(layouts)
        .into_iter()
        .filter_map(|layout| prune(layout, id))
        .filter(|layout| !matches!(layout, Layout::Pane { .. }))
        .collect();
}

fn insert(layout: Layout, target: &str, new: &str, row: bool, before: bool) -> Layout {
    let pane = Layout::Pane { tab: new.to_string() };
    match layout {
        Layout::Pane { tab } if tab == target => {
            let children = if before { vec![pane, Layout::Pane { tab }] } else { vec![Layout::Pane { tab }, pane] };
            if row { Layout::Row { children } } else { Layout::Column { children } }
        }
        Layout::Row { children }
            if row && children.iter().any(|child| matches!(child, Layout::Pane { tab } if tab == target)) =>
        {
            Layout::Row { children: with_sibling(children, target, pane, before) }
        }
        Layout::Column { children }
            if !row && children.iter().any(|child| matches!(child, Layout::Pane { tab } if tab == target)) =>
        {
            Layout::Column { children: with_sibling(children, target, pane, before) }
        }
        Layout::Row { children } => Layout::Row {
            children: children.into_iter().map(|child| insert(child, target, new, row, before)).collect(),
        },
        Layout::Column { children } => Layout::Column {
            children: children.into_iter().map(|child| insert(child, target, new, row, before)).collect(),
        },
        other => other,
    }
}

fn with_sibling(mut children: Vec<Layout>, target: &str, pane: Layout, before: bool) -> Vec<Layout> {
    let index = children.iter().position(|child| matches!(child, Layout::Pane { tab } if tab == target)).unwrap_or(0);
    children.insert(if before { index } else { index + 1 }, pane);
    children
}

fn swap(layout: &mut Layout, target: &str, new: &str) {
    match layout {
        Layout::Pane { tab } => {
            if tab == target {
                *tab = new.to_string();
            }
        }
        Layout::Row { children } | Layout::Column { children } => {
            children.iter_mut().for_each(|child| swap(child, target, new));
        }
    }
}

pub fn replace(layouts: &mut Vec<Layout>, target: &str, new: &str) {
    remove(layouts, new);
    if let Some(index) = group(layouts, target) {
        swap(&mut layouts[index], target, new);
    }
}

pub fn edge(x: f32, y: f32) -> Option<Direction> {
    let distances = [(x, Direction::Left), (1.0 - x, Direction::Right), (y, Direction::Up), (1.0 - y, Direction::Down)];
    distances
        .into_iter()
        .filter(|(distance, _)| *distance < 0.25)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, direction)| direction)
}

pub fn split(layouts: &mut Vec<Layout>, target: &str, new: &str, row: bool, before: bool) {
    remove(layouts, new);
    match group(layouts, target) {
        Some(index) => {
            let layout = layouts.remove(index);
            layouts.insert(index, insert(layout, target, new, row, before));
        }
        None => layouts.push(insert(Layout::Pane { tab: target.to_string() }, target, new, row, before)),
    }
}

fn exchange(layout: &mut Layout, a: &str, b: &str) {
    match layout {
        Layout::Pane { tab } if tab == a => *tab = b.to_string(),
        Layout::Pane { tab } if tab == b => *tab = a.to_string(),
        Layout::Pane { .. } => {}
        Layout::Row { children } | Layout::Column { children } => {
            children.iter_mut().for_each(|child| exchange(child, a, b));
        }
    }
}

pub fn wrap(layouts: &mut Vec<Layout>, target: &str, new: &str, direction: Direction) {
    remove(layouts, new);
    let (row, before) = axis(direction);
    let pane = Layout::Pane { tab: new.to_string() };
    let around = move |layout: Layout| match layout {
        Layout::Row { mut children } if row => {
            children.insert(if before { 0 } else { children.len() }, pane);
            Layout::Row { children }
        }
        Layout::Column { mut children } if !row => {
            children.insert(if before { 0 } else { children.len() }, pane);
            Layout::Column { children }
        }
        other => {
            let children = if before { vec![pane, other] } else { vec![other, pane] };
            if row { Layout::Row { children } } else { Layout::Column { children } }
        }
    };
    match group(layouts, target) {
        Some(index) => {
            let layout = layouts.remove(index);
            layouts.insert(index, around(layout));
        }
        None => layouts.push(around(Layout::Pane { tab: target.to_string() })),
    }
}

pub fn arrange(layouts: &mut Vec<Layout>, target: &str, new: &str, zone: Zone) {
    match zone {
        Zone::Edge(direction) => {
            let (row, before) = axis(direction);
            split(layouts, target, new, row, before);
        }
        Zone::Outer(direction) => wrap(layouts, target, new, direction),
        Zone::Center => match (group(layouts, target), group(layouts, new)) {
            (Some(a), Some(b)) if a == b => exchange(&mut layouts[a], target, new),
            _ => replace(layouts, target, new),
        },
    }
}

pub fn preview(layouts: &[Layout], target: &str, new: &str, zone: Zone) -> Layout {
    let mut layouts = layouts.to_vec();
    arrange(&mut layouts, target, new, zone);
    match group(&layouts, new) {
        Some(index) => layouts.swap_remove(index),
        None => Layout::Pane { tab: new.to_string() },
    }
}

pub fn permute(layouts: &mut [Layout], order: &[String]) {
    fn assign(layout: &mut Layout, order: &mut std::slice::Iter<String>) {
        match layout {
            Layout::Pane { tab } => {
                if let Some(next) = order.next() {
                    *tab = next.clone();
                }
            }
            Layout::Row { children } | Layout::Column { children } => {
                children.iter_mut().for_each(|child| assign(child, order));
            }
        }
    }
    let Some(index) = order.first().and_then(|first| group(layouts, first)) else { return };
    if tabs(&layouts[index]).len() == order.len() {
        assign(&mut layouts[index], &mut order.iter());
    }
}

pub fn unsplit(layouts: &mut Vec<Layout>, id: &str) {
    if let Some(index) = group(layouts, id) {
        layouts.remove(index);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub fn rects(layout: &Layout, area: Rect, out: &mut Vec<(String, Rect)>) {
    match layout {
        Layout::Pane { tab } => out.push((tab.clone(), area)),
        Layout::Row { children } => {
            let width = area.w / children.len() as f32;
            for (index, child) in children.iter().enumerate() {
                rects(child, Rect { x: area.x + width * index as f32, w: width, ..area }, out);
            }
        }
        Layout::Column { children } => {
            let height = area.h / children.len() as f32;
            for (index, child) in children.iter().enumerate() {
                rects(child, Rect { y: area.y + height * index as f32, h: height, ..area }, out);
            }
        }
    }
}

pub fn neighbor(layout: &Layout, id: &str, direction: Direction) -> Option<String> {
    let mut all = Vec::new();
    rects(layout, Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, &mut all);
    let current = all.iter().find(|(tab, _)| tab == id)?.1;
    let center = (current.x + current.w / 2.0, current.y + current.h / 2.0);
    let eps = 0.001;
    all.iter()
        .filter(|(tab, _)| tab != id)
        .filter(|(_, rect)| match direction {
            Direction::Left => rect.x + rect.w <= current.x + eps,
            Direction::Right => rect.x >= current.x + current.w - eps,
            Direction::Up => rect.y + rect.h <= current.y + eps,
            Direction::Down => rect.y >= current.y + current.h - eps,
        })
        .min_by(|(_, a), (_, b)| {
            let distance = |rect: &Rect| {
                let dx = rect.x + rect.w / 2.0 - center.0;
                let dy = rect.y + rect.h / 2.0 - center.1;
                dx * dx + dy * dy
            };
            distance(a).total_cmp(&distance(b))
        })
        .map(|(tab, _)| tab.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_and_removes() {
        let mut layouts = Vec::new();
        split(&mut layouts, "a", "b", true, false);
        split(&mut layouts, "b", "c", true, false);
        split(&mut layouts, "c", "d", false, false);
        assert_eq!(layouts.len(), 1);
        assert_eq!(tabs(&layouts[0]), vec!["a", "b", "c", "d"]);
        assert_eq!(neighbor(&layouts[0], "a", Direction::Right).as_deref(), Some("b"));
        assert_eq!(neighbor(&layouts[0], "c", Direction::Down).as_deref(), Some("d"));
        assert_eq!(neighbor(&layouts[0], "a", Direction::Left), None);
        remove(&mut layouts, "d");
        remove(&mut layouts, "c");
        assert_eq!(
            layouts,
            vec![Layout::Row { children: vec![Layout::Pane { tab: "a".into() }, Layout::Pane { tab: "b".into() }] }]
        );
        remove(&mut layouts, "b");
        assert!(layouts.is_empty());
    }

    #[test]
    fn places_on_edges() {
        let mut layouts = Vec::new();
        split(&mut layouts, "a", "b", true, true);
        assert_eq!(tabs(&layouts[0]), vec!["b", "a"]);
        replace(&mut layouts, "a", "c");
        assert_eq!(tabs(&layouts[0]), vec!["b", "c"]);
        assert_eq!(edge(0.1, 0.5), Some(Direction::Left));
        assert_eq!(edge(0.9, 0.5), Some(Direction::Right));
        assert_eq!(edge(0.5, 0.05), Some(Direction::Up));
        assert_eq!(edge(0.5, 0.95), Some(Direction::Down));
        assert_eq!(edge(0.5, 0.5), None);
    }

    #[test]
    fn arranges_and_permutes() {
        let mut layouts = Vec::new();
        split(&mut layouts, "a", "b", true, false);
        split(&mut layouts, "b", "c", false, false);
        arrange(&mut layouts, "a", "c", Zone::Center);
        assert_eq!(tabs(&layouts[0]), vec!["c", "b", "a"]);
        permute(&mut layouts, &["a".into(), "c".into(), "b".into()]);
        assert_eq!(tabs(&layouts[0]), vec!["a", "c", "b"]);
        assert_eq!(tabs(&preview(&layouts, "a", "d", Zone::Edge(Direction::Left))), vec!["d", "a", "c", "b"]);
        assert_eq!(tabs(&preview(&layouts, "a", "d", Zone::Center)), vec!["d", "c", "b"]);
        assert_eq!(layouts.len(), 1);
        arrange(&mut layouts, "a", "b", Zone::Edge(Direction::Down));
        assert_eq!(tabs(&layouts[0]), vec!["a", "b", "c"]);
    }

    #[test]
    fn wraps_the_whole_group() {
        let mut layouts = Vec::new();
        split(&mut layouts, "a", "b", true, false);
        split(&mut layouts, "b", "c", true, false);
        arrange(&mut layouts, "a", "c", Zone::Outer(Direction::Down));
        assert_eq!(
            layouts,
            vec![Layout::Column {
                children: vec![
                    Layout::Row { children: vec![Layout::Pane { tab: "a".into() }, Layout::Pane { tab: "b".into() }] },
                    Layout::Pane { tab: "c".into() },
                ]
            }]
        );
        arrange(&mut layouts, "b", "d", Zone::Outer(Direction::Down));
        assert_eq!(tabs(&layouts[0]), vec!["a", "b", "c", "d"]);
        assert!(matches!(&layouts[0], Layout::Column { children } if children.len() == 3));
        let area = Rect { x: 0.0, y: 0.0, w: 1000.0, h: 600.0 };
        let pane = Rect { x: 0.0, y: 0.0, w: 500.0, h: 600.0 };
        assert_eq!(zone(pane, area, 250.0, 590.0), Zone::Outer(Direction::Down));
        assert_eq!(zone(pane, area, 250.0, 520.0), Zone::Edge(Direction::Down));
        assert_eq!(zone(pane, area, 250.0, 300.0), Zone::Center);
        assert_eq!(zone(area, area, 990.0, 300.0), Zone::Edge(Direction::Right));
    }
}

use crate::persist::Layout;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
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
}

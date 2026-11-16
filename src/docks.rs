use std::collections::HashSet;

use crate::icons;
use crate::persist::{Dock, Docks, Panel, Stack, View};

impl Dock {
    pub const ALL: [Dock; 4] = [Dock::Left, Dock::Right, Dock::Top, Dock::Bottom];

    pub fn side(self) -> bool {
        matches!(self, Dock::Left | Dock::Right)
    }

    pub fn title(self) -> &'static str {
        match self {
            Dock::Left => "Left",
            Dock::Right => "Right",
            Dock::Top => "Top",
            Dock::Bottom => "Bottom",
        }
    }

    pub fn chevron(self) -> &'static str {
        match self {
            Dock::Left => icons::CHEVRON_LEFT,
            Dock::Right => icons::CHEVRON_RIGHT,
            Dock::Top => icons::CHEVRON_UP,
            Dock::Bottom => icons::CHEVRON_DOWN,
        }
    }

    fn default_size(self) -> f32 {
        if self.side() { 280.0 } else { 200.0 }
    }
}

impl View {
    pub const ALL: [View; 7] =
        [View::Projects, View::Agents, View::History, View::Uplink, View::Files, View::Changes, View::Commits];

    pub fn title(self) -> &'static str {
        match self {
            View::Projects => "Projects",
            View::Agents => "Agents",
            View::History | View::Commits => "History",
            View::Uplink => "Uplink",
            View::Files => "Files",
            View::Changes => "Changes",
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            View::Projects => icons::FOLDER,
            View::Agents => icons::AGENT,
            View::History => icons::HISTORY,
            View::Uplink => icons::UPLINK,
            View::Files => icons::FILES,
            View::Changes => icons::CHANGES,
            View::Commits => icons::COMMIT,
        }
    }

    pub fn list(self) -> bool {
        matches!(self, View::Projects | View::Agents | View::History | View::Uplink)
    }

    fn home(self) -> Dock {
        if self.list() { Dock::Left } else { Dock::Right }
    }
}

impl Panel {
    pub fn new(views: Vec<View>) -> Panel {
        Panel { active: views[0], views, collapsed: false, weight: 1.0 }
    }
}

impl Default for Docks {
    fn default() -> Self {
        Docks {
            left: vec![Stack {
                size: 290.0,
                panels: vec![Panel::new(vec![View::Projects, View::Agents, View::History, View::Uplink])],
            }],
            right: vec![Stack {
                size: 280.0,
                panels: vec![Panel::new(vec![View::Files, View::Changes, View::Commits])],
            }],
            top: Vec::new(),
            bottom: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Join { dock: Dock, stack: usize, panel: usize, before: Option<View> },
    NewPanel { dock: Dock, stack: usize, panel: usize },
    NewStack { dock: Dock, stack: usize },
}

impl Docks {
    pub fn dock(&self, dock: Dock) -> &Vec<Stack> {
        match dock {
            Dock::Left => &self.left,
            Dock::Right => &self.right,
            Dock::Top => &self.top,
            Dock::Bottom => &self.bottom,
        }
    }

    pub fn dock_mut(&mut self, dock: Dock) -> &mut Vec<Stack> {
        match dock {
            Dock::Left => &mut self.left,
            Dock::Right => &mut self.right,
            Dock::Top => &mut self.top,
            Dock::Bottom => &mut self.bottom,
        }
    }

    pub fn panel_mut(&mut self, dock: Dock, stack: usize, panel: usize) -> Option<&mut Panel> {
        self.dock_mut(dock).get_mut(stack)?.panels.get_mut(panel)
    }

    pub fn find(&self, view: View) -> Option<(Dock, usize, usize)> {
        Dock::ALL.into_iter().find_map(|dock| {
            self.dock(dock).iter().enumerate().find_map(|(stack, item)| {
                item.panels.iter().position(|panel| panel.views.contains(&view)).map(|panel| (dock, stack, panel))
            })
        })
    }

    pub fn shown(&self, view: View) -> bool {
        self.find(view)
            .and_then(|(dock, stack, panel)| self.dock(dock).get(stack)?.panels.get(panel))
            .is_some_and(|panel| panel.active == view && !panel.collapsed)
    }

    pub fn show(&mut self, view: View) {
        if let Some((dock, stack, panel)) = self.find(view)
            && let Some(panel) = self.panel_mut(dock, stack, panel)
        {
            panel.active = view;
            panel.collapsed = false;
        }
    }

    pub fn toggle_dock(&mut self, dock: Dock) {
        let stacks = self.dock_mut(dock);
        let collapse = stacks.iter().flat_map(|stack| &stack.panels).any(|panel| !panel.collapsed);
        for panel in stacks.iter_mut().flat_map(|stack| &mut stack.panels) {
            panel.collapsed = collapse;
        }
    }

    pub fn move_to_dock(&mut self, view: View, dock: Dock) {
        let stacks = self.dock(dock);
        let target = match stacks.len() {
            0 => Target::NewStack { dock, stack: 0 },
            count => Target::Join { dock, stack: count - 1, panel: 0, before: None },
        };
        self.move_view(view, target);
    }

    pub fn move_view(&mut self, view: View, target: Target) {
        if let Target::Join { before: Some(before), .. } = target
            && before == view
        {
            return;
        }
        for dock in Dock::ALL {
            for panel in self.dock_mut(dock).iter_mut().flat_map(|stack| &mut stack.panels) {
                panel.views.retain(|item| *item != view);
                if panel.active == view
                    && let Some(first) = panel.views.first()
                {
                    panel.active = *first;
                }
            }
        }
        match target {
            Target::Join { dock, stack, panel, before } => {
                if let Some(panel) = self.panel_mut(dock, stack, panel) {
                    let index = before
                        .and_then(|before| panel.views.iter().position(|item| *item == before))
                        .unwrap_or(panel.views.len());
                    panel.views.insert(index, view);
                    panel.active = view;
                }
            }
            Target::NewPanel { dock, stack, panel } => {
                if let Some(stack) = self.dock_mut(dock).get_mut(stack) {
                    let index = panel.min(stack.panels.len());
                    stack.panels.insert(index, Panel::new(vec![view]));
                }
            }
            Target::NewStack { dock, stack } => {
                let stacks = self.dock_mut(dock);
                let index = stack.min(stacks.len());
                stacks.insert(index, Stack { size: dock.default_size(), panels: vec![Panel::new(vec![view])] });
            }
        }
        self.normalize();
    }

    pub fn normalize(&mut self) {
        let mut seen = HashSet::new();
        for dock in Dock::ALL {
            let stacks = self.dock_mut(dock);
            for stack in stacks.iter_mut() {
                for panel in &mut stack.panels {
                    panel.views.retain(|view| seen.insert(*view));
                    if !panel.views.contains(&panel.active)
                        && let Some(first) = panel.views.first()
                    {
                        panel.active = *first;
                    }
                    if panel.weight.is_nan() || panel.weight <= 0.01 {
                        panel.weight = 1.0;
                    }
                }
                stack.panels.retain(|panel| !panel.views.is_empty());
                if stack.size.is_nan() || stack.size < 40.0 {
                    stack.size = dock.default_size();
                }
            }
            stacks.retain(|stack| !stack.panels.is_empty());
        }
        for view in View::ALL.into_iter().filter(|view| !seen.contains(view)) {
            let dock = view.home();
            match self.dock_mut(dock).first_mut().and_then(|stack| stack.panels.first_mut()) {
                Some(panel) => panel.views.push(view),
                None => {
                    self.dock_mut(dock).push(Stack { size: dock.default_size(), panels: vec![Panel::new(vec![view])] })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn views(docks: &Docks, dock: Dock) -> Vec<Vec<Vec<View>>> {
        docks.dock(dock).iter().map(|stack| stack.panels.iter().map(|panel| panel.views.clone()).collect()).collect()
    }

    #[test]
    fn default_is_normal() {
        let mut docks = Docks::default();
        docks.normalize();
        assert_eq!(docks, Docks::default());
    }

    #[test]
    fn moves_into_new_stack_and_prunes() {
        let mut docks = Docks::default();
        docks.move_view(View::Files, Target::NewStack { dock: Dock::Bottom, stack: 0 });
        docks.move_view(
            View::Changes,
            Target::Join { dock: Dock::Bottom, stack: 0, panel: 0, before: Some(View::Files) },
        );
        docks.move_view(View::Commits, Target::NewPanel { dock: Dock::Left, stack: 0, panel: 1 });
        assert!(docks.right.is_empty());
        assert_eq!(views(&docks, Dock::Bottom), vec![vec![vec![View::Changes, View::Files]]]);
        assert_eq!(
            views(&docks, Dock::Left),
            vec![vec![vec![View::Projects, View::Agents, View::History, View::Uplink], vec![View::Commits]]]
        );
        assert_eq!(docks.bottom[0].panels[0].active, View::Changes);
    }

    #[test]
    fn reorders_within_a_panel() {
        let mut docks = Docks::default();
        docks.move_view(
            View::Commits,
            Target::Join { dock: Dock::Right, stack: 0, panel: 0, before: Some(View::Files) },
        );
        assert_eq!(views(&docks, Dock::Right), vec![vec![vec![View::Commits, View::Files, View::Changes]]]);
    }

    #[test]
    fn moving_the_only_view_keeps_it() {
        let mut docks = Docks::default();
        docks.move_view(View::Files, Target::NewStack { dock: Dock::Top, stack: 0 });
        docks.move_view(View::Files, Target::NewPanel { dock: Dock::Top, stack: 0, panel: 1 });
        assert_eq!(views(&docks, Dock::Top), vec![vec![vec![View::Files]]]);
        docks.move_view(View::Files, Target::Join { dock: Dock::Top, stack: 0, panel: 0, before: None });
        assert_eq!(views(&docks, Dock::Top), vec![vec![vec![View::Files]]]);
    }

    #[test]
    fn normalize_drops_duplicates_and_restores_missing() {
        let mut docks = Docks {
            left: vec![Stack { size: 0.0, panels: vec![Panel::new(vec![View::Files, View::Files])] }],
            right: Vec::new(),
            top: Vec::new(),
            bottom: Vec::new(),
        };
        docks.normalize();
        assert_eq!(docks.left[0].size, 280.0);
        assert_eq!(
            views(&docks, Dock::Left),
            vec![vec![vec![View::Files, View::Projects, View::Agents, View::History, View::Uplink]]]
        );
        assert_eq!(views(&docks, Dock::Right), vec![vec![vec![View::Changes, View::Commits]]]);
    }

    #[test]
    fn toggle_and_show() {
        let mut docks = Docks::default();
        docks.toggle_dock(Dock::Left);
        assert!(docks.left[0].panels[0].collapsed);
        docks.show(View::History);
        assert!(docks.shown(View::History));
        docks.move_to_dock(View::Agents, Dock::Bottom);
        assert_eq!(views(&docks, Dock::Bottom), vec![vec![vec![View::Agents]]]);
        docks.move_to_dock(View::Files, Dock::Bottom);
        assert_eq!(views(&docks, Dock::Bottom), vec![vec![vec![View::Agents, View::Files]]]);
    }
}

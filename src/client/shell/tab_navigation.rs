//! A client-only tab cursor. Browsing never changes focus; Enter uses the existing tab API.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TabNavigationTarget {
    workspace: WorkspaceNavigationTarget,
    pub(super) tab_id: String,
}

impl ClientShellState {
    fn tab_navigation_targets(&self) -> Vec<TabNavigationTarget> {
        let Some(workspace) = self.focused_navigation_target() else {
            return Vec::new();
        };
        let Some(snapshot) = self.snapshot.as_deref() else {
            return Vec::new();
        };
        snapshot
            .tabs
            .iter()
            .filter(|tab| tab.workspace_id == workspace.workspace_id)
            .map(|tab| TabNavigationTarget {
                workspace: workspace.clone(),
                tab_id: tab.tab_id.clone(),
            })
            .collect()
    }

    fn tab_navigation_target_valid(&self, target: &TabNavigationTarget) -> bool {
        target.workspace.endpoint_id == self.active_endpoint_id
            && self.navigation_target_valid(&target.workspace)
            && self.snapshot.as_deref().is_some_and(|snapshot| {
                snapshot.focused_workspace_id.as_deref()
                    == Some(target.workspace.workspace_id.as_str())
                    && snapshot.tabs.iter().any(|tab| {
                        tab.tab_id == target.tab_id
                            && tab.workspace_id == target.workspace.workspace_id
                    })
            })
    }

    pub(super) fn reconcile_tab_navigation(&mut self) {
        if self
            .navigate_tab
            .as_ref()
            .is_some_and(|target| !self.tab_navigation_target_valid(target))
        {
            self.navigate_tab = None;
            self.reveal_focused_tab = true;
        }
    }

    pub(super) fn enter_tab_navigation(&mut self) {
        self.pending_workspace_highlight = None;
        self.navigate_workspace_id = None;
        self.navigate_agent = None;
        self.mode = ClientShellMode::TabNavigate;
        let focused = self
            .snapshot
            .as_deref()
            .and_then(|snapshot| snapshot.focused_tab_id.as_deref());
        self.navigate_tab = self
            .tab_navigation_targets()
            .into_iter()
            .find(|target| Some(target.tab_id.as_str()) == focused);
        self.reveal_focused_tab = true;
    }

    pub(super) fn cancel_tab_navigation(&mut self, outcome: &mut ClientShellInput) {
        self.mode = self.copy_or_terminal_mode();
        self.navigate_tab = None;
        self.reveal_focused_tab = true;
        outcome.repaint = true;
    }

    pub(super) fn route_tab_navigate_key(
        &mut self,
        key: &crate::input::TerminalKey,
        outcome: &mut ClientShellInput,
    ) {
        if key.code == KeyCode::Esc || self.config.keybinds.matches_prefix(key) {
            self.cancel_tab_navigation(outcome);
            return;
        }
        let (code, modifiers) = crate::config::normalize_key_combo((key.code, key.modifiers));
        let bindings = &self.config.keybinds.keybinds.tab_navigate;
        if (code == KeyCode::Left && modifiers.is_empty())
            || bindings.previous.matches_direct_key(key)
        {
            self.move_tab_navigation(-1);
            outcome.repaint = true;
        } else if (code == KeyCode::Right && modifiers.is_empty())
            || bindings.next.matches_direct_key(key)
        {
            self.move_tab_navigation(1);
            outcome.repaint = true;
        } else if code == KeyCode::Enter && modifiers.is_empty() {
            self.accept_tab_navigation(outcome);
        } else if super::input::is_ctrl_bracket_key(key) {
            self.cancel_tab_navigation(outcome);
        }
    }

    fn move_tab_navigation(&mut self, delta: isize) {
        let targets = self.tab_navigation_targets();
        if targets.is_empty() {
            self.navigate_tab = None;
            return;
        }
        let current = self
            .navigate_tab
            .as_ref()
            .and_then(|selected| targets.iter().position(|target| target == selected));
        let next = match current {
            Some(current) => (current as isize + delta).rem_euclid(targets.len() as isize) as usize,
            None if delta < 0 => targets.len() - 1,
            None => 0,
        };
        self.navigate_tab = Some(targets[next].clone());
        self.reveal_focused_tab = true;
    }

    fn accept_tab_navigation(&mut self, outcome: &mut ClientShellInput) {
        let Some(target) = self.navigate_tab.clone() else {
            self.cancel_tab_navigation(outcome);
            return;
        };
        if !self.tab_navigation_target_valid(&target) {
            self.receive_endpoint_unavailable(
                "Tab is no longer available; select an available tab".into(),
            );
            self.navigate_tab = None;
            outcome.repaint = true;
            return;
        }
        let return_mode = if self
            .snapshot
            .as_deref()
            .and_then(|snapshot| snapshot.focused_tab_id.as_deref())
            == Some(target.tab_id.as_str())
        {
            self.copy_or_terminal_mode()
        } else {
            ClientShellMode::Terminal
        };
        if self.focus_or_activate(
            target.workspace.endpoint_id,
            ClientEndpointFocusTarget::Tab(target.tab_id),
            outcome,
        ) {
            self.mode = return_mode;
            self.navigate_tab = None;
            self.reveal_focused_tab = true;
        }
        outcome.repaint = true;
    }
}

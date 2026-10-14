//! Agent navigate mode: a keyboard cursor over the agent list, separate from workspace
//! navigate mode. Moving only highlights a row; enter focuses the highlighted agent.

use super::*;
use crossterm::event::KeyModifiers;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AgentNavigationTarget {
    pub(super) endpoint_id: ClientEndpointId,
    pub(super) pane_id: String,
}

impl AgentNavigationTarget {
    pub(super) fn matches(&self, endpoint_id: &ClientEndpointId, pane_id: &str) -> bool {
        &self.endpoint_id == endpoint_id && self.pane_id == pane_id
    }
}

impl ClientShellState {
    /// Agents in the order the sidebar lists them. Across machines only online agents
    /// are selectable, matching next_agent/previous_agent.
    pub(super) fn agent_navigation_targets(&self) -> Vec<AgentNavigationTarget> {
        if self.multi_endpoint_active() {
            return super::aggregate_navigation::online_agent_targets(
                &self.endpoints,
                &self.active_endpoint_id,
                self.config.agent_panel_sort,
            )
            .into_iter()
            .map(|target| AgentNavigationTarget {
                endpoint_id: target.endpoint_id,
                pane_id: target.pane_id,
            })
            .collect();
        }
        let Some(snapshot) = self.snapshot.as_deref() else {
            return Vec::new();
        };
        super::agent_sidebar::ordered_agent_pane_ids(snapshot, self.config.agent_panel_sort)
            .into_iter()
            .map(|pane_id| AgentNavigationTarget {
                endpoint_id: self.active_endpoint_id.clone(),
                pane_id,
            })
            .collect()
    }

    pub(super) fn enter_agent_navigation(&mut self) {
        self.pending_workspace_highlight = None;
        self.navigate_workspace_id = None;
        self.mode = ClientShellMode::AgentNavigate;
        let focused = self
            .snapshot
            .as_deref()
            .and_then(|snapshot| snapshot.focused_pane_id.as_deref());
        self.navigate_agent = self.agent_navigation_targets().into_iter().find(|target| {
            target.endpoint_id == self.active_endpoint_id
                && Some(target.pane_id.as_str()) == focused
        });
    }

    pub(super) fn cancel_agent_navigation(&mut self, outcome: &mut ClientShellInput) {
        self.mode = self.copy_or_terminal_mode();
        self.navigate_agent = None;
        outcome.repaint = true;
    }

    pub(super) fn route_agent_navigate_key(
        &mut self,
        key: &crate::input::TerminalKey,
        outcome: &mut ClientShellInput,
    ) {
        use crate::input::{KeybindAction, KeybindMatch};

        if key.code == KeyCode::Esc
            || self.config.keybinds.matches_prefix(key)
            || super::input::is_ctrl_bracket_key(key)
        {
            self.cancel_agent_navigation(outcome);
            return;
        }

        let (code, modifiers) = crate::config::normalize_key_combo((key.code, key.modifiers));
        let bindings = &self.config.keybinds.keybinds.agent_navigate;
        if (code == KeyCode::Up && modifiers.is_empty()) || bindings.up.matches_direct_key(key) {
            self.move_agent_navigation(-1);
            outcome.repaint = true;
            return;
        }
        if (code == KeyCode::Down && modifiers.is_empty()) || bindings.down.matches_direct_key(key)
        {
            self.move_agent_navigation(1);
            outcome.repaint = true;
            return;
        }
        if code == KeyCode::Enter && modifiers.is_empty() {
            self.accept_agent_navigation(outcome);
            return;
        }
        if let Some(index) = ('1'..='9').position(|digit| {
            crate::config::terminal_key_matches_combo(
                key,
                (KeyCode::Char(digit), KeyModifiers::empty()),
            )
        }) {
            if index < self.agent_navigation_targets().len() {
                self.mode = ClientShellMode::Terminal;
                self.navigate_agent = None;
                self.record_binding(
                    KeybindMatch::Action(KeybindAction::FocusAgent(index)),
                    outcome,
                );
                outcome.repaint = true;
            }
        }
    }

    pub(super) fn move_agent_navigation(&mut self, delta: isize) {
        let targets = self.agent_navigation_targets();
        if targets.is_empty() {
            self.navigate_agent = None;
            return;
        }
        let current = self
            .navigate_agent
            .as_ref()
            .and_then(|selected| targets.iter().position(|target| target == selected));
        let next = match current {
            Some(current) => (current as isize + delta).rem_euclid(targets.len() as isize) as usize,
            None if delta < 0 => targets.len() - 1,
            None => 0,
        };
        let target = targets[next].clone();
        self.reveal_agent_navigation_target(&target, next);
        self.navigate_agent = Some(target);
    }

    fn reveal_agent_navigation_target(&mut self, target: &AgentNavigationTarget, index: usize) {
        if self.multi_endpoint_active() {
            self.reveal_endpoint_agent(
                &target.endpoint_id,
                &target.pane_id,
                self.hits.agent_body.height,
            );
        } else if !self
            .hits
            .agents
            .iter()
            .any(|(_, visible_pane_id)| visible_pane_id == &target.pane_id)
        {
            self.agent_scroll = index.min(self.hits.agent_max_scroll);
        }
    }

    fn accept_agent_navigation(&mut self, outcome: &mut ClientShellInput) {
        let Some(target) = self.navigate_agent.clone() else {
            self.cancel_agent_navigation(outcome);
            return;
        };
        if !self.agent_navigation_targets().contains(&target) {
            self.navigate_agent = None;
            outcome.repaint = true;
            return;
        }
        if self.focus_or_activate(
            target.endpoint_id.clone(),
            ClientEndpointFocusTarget::Pane(target.pane_id.clone()),
            outcome,
        ) {
            if target.endpoint_id != self.active_endpoint_id {
                self.pending_agent_reveal = Some((target.endpoint_id, target.pane_id));
            }
            self.mode = ClientShellMode::Terminal;
            self.navigate_agent = None;
        }
        outcome.repaint = true;
    }
}

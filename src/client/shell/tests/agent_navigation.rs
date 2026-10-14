use super::*;

fn nav_agent(pane_id: &str, focused: bool) -> ClientShellAgent {
    ClientShellAgent {
        pane_id: pane_id.into(),
        workspace_id: "ws_1".into(),
        tab_id: "tab_1".into(),
        name: Some(pane_id.into()),
        display_agent: None,
        agent: Some("pi".into()),
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: AgentStatus::Idle,
        state_change_seq: 1,
        state_labels: Vec::new(),
        tokens: Vec::new(),
        focused,
    }
}

fn agent_navigation_state(config: &Config) -> ClientShellState {
    let mut projected = snapshot();
    for pane_id in ["pane_2", "pane_3"] {
        let mut pane = projected.panes[0].clone();
        pane.pane_id = pane_id.into();
        pane.focused = false;
        projected.panes.push(pane);
    }
    projected.agents = vec![
        nav_agent("pane_1", true),
        nav_agent("pane_2", false),
        nav_agent("pane_3", false),
    ];
    let mut shell_config = ClientShellConfig::from_config(config);
    shell_config.mobile_width_threshold = 0;
    let mut state = ClientShellState::new(shell_config);
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state.compose(100, 28).unwrap();
    state
}

fn quiet_key(state: &mut ClientShellState, bytes: &[u8]) {
    let outcome = state.handle_input_bytes(bytes);
    assert!(
        outcome.actions.is_empty(),
        "{bytes:?}: {:?}",
        outcome.actions
    );
    assert!(outcome.requests.is_empty(), "{bytes:?}");
}

fn enter_agent_navigation(state: &mut ClientShellState) {
    quiet_key(state, &[0x02]);
    quiet_key(state, b"a");
    assert_eq!(state.mode, ClientShellMode::AgentNavigate);
}

fn selected_pane(state: &ClientShellState) -> Option<&str> {
    state
        .navigate_agent
        .as_ref()
        .map(|target| target.pane_id.as_str())
}

fn focused_pane_request(outcome: &ClientShellInput) -> &str {
    let [ClientShellAction::Endpoint { request, .. }] = outcome.actions.as_slice() else {
        panic!("expected one focus request, got {:?}", outcome.actions);
    };
    let crate::api::schema::Method::PaneFocus(target) = &request.method else {
        panic!("expected PaneFocus, got {:?}", request.method);
    };
    &target.pane_id
}

#[test]
fn agent_navigation_moves_a_highlight_and_focuses_on_enter() {
    let mut state = agent_navigation_state(&Config::default());
    enter_agent_navigation(&mut state);
    assert_eq!(selected_pane(&state), Some("pane_1"));

    for (key, expected) in [
        (b'j', "pane_2"),
        (b'j', "pane_3"),
        (b'j', "pane_1"),
        (b'k', "pane_3"),
    ] {
        quiet_key(&mut state, &[key]);
        assert_eq!(selected_pane(&state), Some(expected));
        assert_eq!(state.mode, ClientShellMode::AgentNavigate);
    }

    let outcome = state.handle_input_bytes(b"\r");
    assert_eq!(focused_pane_request(&outcome), "pane_3");
    assert_eq!(state.mode, ClientShellMode::Terminal);
    assert!(state.navigate_agent.is_none());
}

#[test]
fn agent_navigation_ignores_workspace_navigation_keys() {
    let config: Config =
        toml::from_str("[keys]\nnavigate_workspace_up = \"k\"\nnavigate_workspace_down = \"j\"\n")
            .unwrap();
    let mut state = agent_navigation_state(&config);
    enter_agent_navigation(&mut state);

    quiet_key(&mut state, b"j");
    assert_eq!(selected_pane(&state), Some("pane_2"));
    assert!(state.navigate_workspace_id.is_none());
    for key in [b"l".as_slice(), b"h", b"\t"] {
        quiet_key(&mut state, key);
        assert_eq!(state.mode, ClientShellMode::AgentNavigate);
        assert_eq!(selected_pane(&state), Some("pane_2"));
    }

    quiet_key(&mut state, b"\x1b");
    assert_eq!(state.mode, ClientShellMode::Terminal);
    assert!(state.navigate_agent.is_none());
}

#[test]
fn agent_navigation_digits_jump_and_custom_keys_apply() {
    let config: Config =
        toml::from_str("[keys]\nagent_picker = \"prefix+shift+a\"\nnavigate_agent_down = \"n\"\n")
            .unwrap();
    let mut state = agent_navigation_state(&config);
    quiet_key(&mut state, &[0x02]);
    quiet_key(&mut state, b"A");
    assert_eq!(state.mode, ClientShellMode::AgentNavigate);
    quiet_key(&mut state, b"n");
    assert_eq!(selected_pane(&state), Some("pane_2"));
    quiet_key(&mut state, b"j");
    assert_eq!(selected_pane(&state), Some("pane_2"));

    let outcome = state.handle_input_bytes(b"3");
    assert_eq!(focused_pane_request(&outcome), "pane_3");
    assert_eq!(state.mode, ClientShellMode::Terminal);
}

#[test]
fn agent_navigation_highlights_the_selected_row() {
    let mut state = agent_navigation_state(&Config::default());
    enter_agent_navigation(&mut state);
    quiet_key(&mut state, b"j");
    let frame = state.compose(100, 28).unwrap();
    let (rect, _) = state
        .hits
        .agents
        .iter()
        .find(|(_, pane_id)| pane_id == "pane_2")
        .cloned()
        .expect("pane_2 agent row");
    let cell = &frame.cells[usize::from(rect.y) * usize::from(frame.width) + usize::from(rect.x)];
    assert_eq!(
        cell.bg,
        crate::protocol::color_to_u32(super::render::sidebar::workspace_selection_background(
            &state.config.palette
        ))
    );

    quiet_key(&mut state, b"\x1b");
    let frame = state.compose(100, 28).unwrap();
    let cell = &frame.cells[usize::from(rect.y) * usize::from(frame.width) + usize::from(rect.x)];
    assert_ne!(
        cell.bg,
        crate::protocol::color_to_u32(super::render::sidebar::workspace_selection_background(
            &state.config.palette
        ))
    );
}

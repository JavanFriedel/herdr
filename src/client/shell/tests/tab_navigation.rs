use super::*;

fn tab_state(config: &Config, count: usize) -> ClientShellState {
    let mut projected = snapshot();
    projected.tabs = (1..=count)
        .map(|number| {
            let mut tab = projected.tabs[0].clone();
            tab.tab_id = format!("tab_{number}");
            tab.number = number;
            tab.label = format!("tab {number}");
            tab.focused = number == 1;
            tab
        })
        .collect();
    let mut config = ClientShellConfig::from_config(config);
    config.mobile_width_threshold = 0;
    let mut state = ClientShellState::new(config);
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state.compose(100, 28).unwrap();
    state
}

fn quiet(state: &mut ClientShellState, bytes: &[u8]) {
    let outcome = state.handle_input_bytes(bytes);
    assert!(
        outcome.actions.is_empty(),
        "{bytes:?}: {:?}",
        outcome.actions
    );
    assert!(
        outcome.requests.is_empty(),
        "{bytes:?}: {:?}",
        outcome.requests
    );
}

fn enter(state: &mut ClientShellState) {
    quiet(state, &[0x02]);
    quiet(state, b"t");
    assert_eq!(state.mode, ClientShellMode::TabNavigate);
}

fn selected(state: &ClientShellState) -> Option<&str> {
    state
        .navigate_tab
        .as_ref()
        .map(|target| target.tab_id.as_str())
}

#[test]
fn tab_navigation_highlights_until_enter_without_changing_the_visible_tab() {
    let mut state = tab_state(&Config::default(), 3);
    enter(&mut state);
    assert_eq!(selected(&state), Some("tab_1"));
    assert!(state.host_keyboard_report_all_requested());
    for (key, expected) in [
        (b"l".as_slice(), "tab_2"),
        (b"l", "tab_3"),
        (b"l", "tab_1"),
        (b"h", "tab_3"),
        (b"\x1b[C", "tab_1"),
        (b"\x1b[D", "tab_3"),
    ] {
        quiet(&mut state, key);
        assert_eq!(selected(&state), Some(expected));
        assert_eq!(
            state.snapshot.as_ref().unwrap().focused_tab_id.as_deref(),
            Some("tab_1")
        );
        assert_eq!(state.mode, ClientShellMode::TabNavigate);
    }
    let outcome = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = outcome.actions.as_slice() else {
        panic!(
            "expected exactly one focus request, got {:?}",
            outcome.actions
        );
    };
    assert!(
        matches!(&request.method, crate::api::schema::Method::TabFocus(target) if target.tab_id == "tab_3")
    );
    assert!(outcome.requests.is_empty());
    assert_eq!(state.mode, ClientShellMode::Terminal);
    assert!(state.navigate_tab.is_none());
}

#[test]
fn tab_navigation_cancel_and_unrelated_keys_never_focus_a_tab() {
    for cancel in [b"\x1b".as_slice(), &[0x02], b"\x1b[91;5u"] {
        let mut state = tab_state(&Config::default(), 3);
        enter(&mut state);
        quiet(&mut state, b"l");
        for key in [b"j".as_slice(), b"k", b"\t", b"2"] {
            quiet(&mut state, key);
            assert_eq!(selected(&state), Some("tab_2"));
        }
        quiet(&mut state, cancel);
        assert_eq!(state.mode, ClientShellMode::Terminal);
        assert!(state.navigate_tab.is_none());
        assert_eq!(
            state.snapshot.as_ref().unwrap().focused_tab_id.as_deref(),
            Some("tab_1")
        );
    }
}

#[test]
fn tab_navigation_custom_bindings_and_report_all_plain_text_work() {
    let config: Config = toml::from_str("[keys]\ntab_picker = \"prefix+shift+q\"\nnavigate_tab_previous = \"p\"\nnavigate_tab_next = \"n\"\n").unwrap();
    let mut state = tab_state(&config, 3);
    state.set_host_reports_key_releases(true);
    state.set_host_reports_all_keys(true);
    quiet(&mut state, &[0x02]);
    quiet(&mut state, b"Q");
    assert_eq!(state.mode, ClientShellMode::TabNavigate);
    quiet(&mut state, b"n");
    assert_eq!(selected(&state), Some("tab_2"));
    quiet(&mut state, b"l");
    assert_eq!(selected(&state), Some("tab_2"));
    quiet(&mut state, b"p");
    assert_eq!(selected(&state), Some("tab_1"));
    quiet(&mut state, b"\x1b[C");
    assert_eq!(selected(&state), Some("tab_2"));
}

#[test]
fn tab_navigation_empty_and_single_tab_are_safe() {
    let mut empty = tab_state(&Config::default(), 0);
    enter(&mut empty);
    quiet(&mut empty, b"l");
    quiet(&mut empty, b"\r");
    assert!(empty.navigate_tab.is_none());
    assert_eq!(empty.mode, ClientShellMode::Terminal);
    let mut single = tab_state(&Config::default(), 1);
    enter(&mut single);
    for key in [b"l".as_slice(), b"h", b"\x1b[C"] {
        quiet(&mut single, key);
        assert_eq!(selected(&single), Some("tab_1"));
    }
}

#[test]
fn tab_navigation_reveals_overflow_and_highlights_without_hiding_a_bottom_tab_bar() {
    let mut config = Config::default();
    config.ui.tab_bar_position = TabBarPositionConfig::Bottom;
    let mut state = tab_state(&config, 15);
    enter(&mut state);
    for _ in 0..11 {
        quiet(&mut state, b"l");
    }
    let frame = state.compose(80, 24).unwrap();
    let (rect, _) = state
        .hits
        .tabs
        .iter()
        .find(|(_, id)| id == "tab_12")
        .expect("highlighted tab must be visible");
    let cell = &frame.cells[usize::from(rect.y) * usize::from(frame.width) + usize::from(rect.x)];
    assert_eq!(
        cell.bg,
        crate::protocol::color_to_u32(super::render::sidebar::workspace_selection_background(
            &state.config.palette
        ))
    );
    assert!(state.tab_scroll > 0);
    let text: String = frame
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect();
    assert!(text.contains("TABS"));
    assert_eq!(
        state.snapshot.as_ref().unwrap().focused_tab_id.as_deref(),
        Some("tab_1")
    );
    quiet(&mut state, b"\x1b");
    state.compose(80, 24).unwrap();
    assert!(state.hits.tabs.iter().any(|(_, id)| id == "tab_1"));
}

#[test]
fn tab_navigation_rejects_removed_tabs_and_workspace_or_boot_changes() {
    for change in ["tab", "workspace", "boot"] {
        let mut state = tab_state(&Config::default(), 3);
        enter(&mut state);
        quiet(&mut state, b"l");
        let mut updated = state.snapshot.as_ref().unwrap().clone();
        updated.revision += 1;
        match change {
            "tab" => updated.tabs.retain(|tab| tab.tab_id != "tab_2"),
            "workspace" => updated.focused_workspace_id = None,
            "boot" => updated.boot_id = "boot-2".into(),
            _ => unreachable!(),
        }
        state.set_snapshot(updated);
        assert!(state.navigate_tab.is_none());
        assert_eq!(state.mode, ClientShellMode::TabNavigate);
        quiet(&mut state, b"\r");
    }
}

#[test]
fn tab_navigation_disconnection_and_projection_reset_cannot_commit_stale_targets() {
    for disconnect in ["status", "retire", "reset"] {
        let mut state = tab_state(&Config::default(), 3);
        enter(&mut state);
        quiet(&mut state, b"l");
        match disconnect {
            "status" => state
                .set_endpoint_status(&ClientEndpointId::Local, ClientEndpointStatus::Reconnecting),
            "retire" => state.retire_endpoint(&ClientEndpointId::Local),
            "reset" => state.reset_endpoint_projection(),
            _ => unreachable!(),
        }
        assert!(state.navigate_tab.is_none());
        assert_eq!(state.mode, ClientShellMode::TabNavigate);
        quiet(&mut state, b"\r");
        assert_eq!(state.mode, ClientShellMode::Terminal);
    }
}

#[test]
fn tab_navigation_mouse_click_cancels_the_keyboard_cursor() {
    let mut state = tab_state(&Config::default(), 3);
    enter(&mut state);
    quiet(&mut state, b"l");
    let mut outcome = ClientShellInput::default();
    state.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 27,
            modifiers: KeyModifiers::empty(),
        },
        &mut outcome,
    );
    assert!(state.navigate_tab.is_none());
    assert_ne!(state.mode, ClientShellMode::TabNavigate);
}

#[test]
fn tab_navigation_only_browses_the_current_workspace_and_keeps_mobile_tabs_visible() {
    let mut state = tab_state(&Config::default(), 3);
    let mut updated = state.snapshot.as_ref().unwrap().clone();
    let mut foreign = updated.tabs[0].clone();
    foreign.workspace_id = "ws_2".into();
    foreign.tab_id = "other_workspace_tab".into();
    updated.tabs.push(foreign);
    state.set_snapshot(updated);
    state.config.mobile_width_threshold = 80;
    let surface_size = state.surface_size(55, 16);
    enter(&mut state);
    assert_eq!(state.surface_size(55, 16), surface_size);
    for expected in ["tab_2", "tab_3", "tab_1", "tab_2"] {
        quiet(&mut state, b"l");
        assert_eq!(selected(&state), Some(expected));
    }
    let frame = state.compose(55, 16).unwrap();
    assert!(state.hits.tabs.iter().any(|(_, id)| id == "tab_2"));
    let text: String = frame
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect();
    assert!(text.contains("TABS"));
}

#[test]
fn tab_navigation_generation_change_and_keymap_reload_clear_the_cursor() {
    let mut state = tab_state(&Config::default(), 3);
    enter(&mut state);
    quiet(&mut state, b"l");
    state.endpoints[0].snapshot_generation = Some(9);
    quiet(&mut state, b"\r");
    assert!(state.navigate_tab.is_none());

    let mut state = tab_state(&Config::default(), 3);
    state.config.keybinding_source = ClientShellKeybindingSource::Endpoint;
    enter(&mut state);
    quiet(&mut state, b"l");
    let mut updated = state.snapshot.as_ref().unwrap().clone();
    updated.server_keybindings_toml = Some("[keys]\ntab_picker = \"prefix+alt+t\"\n".into());
    state.set_snapshot(updated);
    assert!(state.navigate_tab.is_none());
    assert_eq!(state.mode, ClientShellMode::Terminal);
}

#[test]
fn tab_navigation_cancel_returns_to_copy_mode() {
    let mut state = tab_state(&Config::default(), 3);
    state.copy_mode = Some(ClientCopyModeState {
        pane_id: "pane_1".into(),
        content_revision: 0,
        geometry: (4, 2),
        alternate_screen_active: false,
        cursor: crate::api::schema::PaneTextPoint { row: 0, col: 0 },
        offset_from_bottom: 0,
        max_offset_from_bottom: 0,
        entry_offset_from_bottom: 0,
        selection: None,
        search_prompt: None,
        search_query: String::new(),
        search_direction: None,
        search_matches: Vec::new(),
        search_total: 0,
        search_current: None,
        search_current_global: None,
        search_generation: 0,
        copy_after_search: false,
    });
    state.mode = ClientShellMode::Copy;
    enter(&mut state);
    quiet(&mut state, b"l");
    quiet(&mut state, b"\x1b");
    assert_eq!(state.mode, ClientShellMode::Copy);
    assert!(state.copy_mode.is_some());
}

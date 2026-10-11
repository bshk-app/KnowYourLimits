#[test]
fn native_startup_has_no_fabricated_quotas() {
    let state = crate::startup_state(crate::model::Settings::default());
    assert!(
        state
            .providers
            .iter()
            .all(|provider| provider.windows.is_empty() && provider.fetched_at.is_none()),
        "native startup shows fabricated subscription quotas"
    );
    assert_eq!(state.title(crate::model::now_ms()), "C — · X —");
}

#[test]
fn native_panel_can_subscribe_to_backend_state_events() {
    let mut context = crate::app_context();
    let authority = context.runtime_authority_mut();
    for command in ["plugin:event|listen", "plugin:event|unlisten"] {
        assert!(
            authority.resolve_access(command, "main", "main", &tauri::ipc::Origin::Local).is_some(),
            "native panel cannot subscribe/unsubscribe: {command} is denied by the bundled capabilities"
        );
    }
    assert!(authority
        .resolve_access(
            "plugin:shell|execute",
            "main",
            "main",
            &tauri::ipc::Origin::Local
        )
        .is_none());
}

#[test]
fn only_settings_window_can_drag_resize_and_close_itself() {
    let mut context = crate::app_context();
    let authority = context.runtime_authority_mut();
    let local = &tauri::ipc::Origin::Local;
    for command in [
        "plugin:event|listen",
        "plugin:event|unlisten",
        "plugin:window|start_dragging",
        "plugin:window|set_size",
        "plugin:window|close",
    ] {
        assert!(
            authority
                .resolve_access(command, "settings", "settings", local)
                .is_some(),
            "settings window is missing {command}"
        );
    }
    for command in ["plugin:window|set_size", "plugin:window|close"] {
        assert!(authority
            .resolve_access(command, "main", "main", local)
            .is_none());
    }
    assert!(authority
        .resolve_access("plugin:shell|execute", "settings", "settings", local)
        .is_none());
}

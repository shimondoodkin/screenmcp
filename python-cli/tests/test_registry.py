import screenmcp_cli as app

EXPECTED = {
    "screenshot", "screenshot_region", "screenshot_window", "active_window",
    "get_screen_size", "click", "right_click", "double_click", "long_click",
    "middle_click", "mouse_move", "drag", "scroll", "mouse_scroll", "type",
    "press_key", "hold_key", "release_key", "hotkey", "get_text", "select_all",
    "copy", "paste", "get_clipboard", "set_clipboard", "list_windows",
    "focus_window", "back", "home", "recents", "elevate",
    "is_elevated", "camera", "list_cameras", "play_audio",
}


def test_every_expected_command_is_registered():
    missing = EXPECTED - set(app.TOOLS)
    assert not missing, f"missing tools: {missing}"


def test_ui_tree_is_not_registered():
    assert "ui_tree" not in app.TOOLS


def test_every_tool_has_description_schema_and_callable_handler():
    for name, tool in app.TOOLS.items():
        assert tool["description"], name
        assert tool["inputSchema"]["type"] == "object", name
        assert callable(tool["handler"]), name


def test_tools_list_method_returns_all():
    listed = app._list_tools()
    assert len(listed) == len(app.TOOLS)


def test_coordinate_tools_advertise_model_param():
    for name in ("screenshot", "screenshot_region", "get_screen_size", "click",
                 "drag", "scroll", "mouse_move"):
        props = app.TOOLS[name]["inputSchema"]["properties"]
        assert "model" in props, f"{name} missing model param"


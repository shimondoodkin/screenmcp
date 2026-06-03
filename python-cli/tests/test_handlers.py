import screenmcp_cli as app


def test_primary_modifier_is_a_string():
    assert app.PRIMARY_MOD in ("ctrl", "cmd")


def test_nav_keymap_has_expected_actions():
    km = app.nav_keymap()
    assert set(km) == {"back", "home", "recents"}
    # each maps to a non-empty list of key names
    for combo in km.values():
        assert isinstance(combo, list) and combo


class _FakeShot:
    def __init__(self, w, h):
        self.size = (w, h)
        self.rgb = b"\x00\x00\x00" * (w * h)


class _FakeGrabber:
    monitors = [{"left": 0, "top": 0, "width": 2912, "height": 1638},
                {"left": 0, "top": 0, "width": 2912, "height": 1638}]

    def grab(self, region):
        return _FakeShot(region["width"], region["height"])


def test_get_screen_size_reports_scaled_and_native(monkeypatch):
    monkeypatch.setattr(app, "grabber", lambda: _FakeGrabber())
    out = app.cmd_get_screen_size({})
    text = out["content"][0]["text"]
    assert '"width": 1456' in text and '"height": 819' in text
    assert '"original_width": 2912' in text


def test_screenshot_returns_webp_image_block(monkeypatch):
    monkeypatch.setattr(app, "grabber", lambda: _FakeGrabber())
    out = app.cmd_screenshot({})
    block = out["content"][0]
    assert block["type"] == "image"
    assert block["mimeType"] == "image/webp"
    assert len(block["data"]) > 0


class _RecKeyboard:
    def __init__(self):
        self.events = []
        self.typed = []

    def press(self, k):
        self.events.append(("press", k))

    def release(self, k):
        self.events.append(("release", k))

    def type(self, s):
        self.typed.append(s)


def test_type_sends_text(monkeypatch):
    rec = _RecKeyboard()
    monkeypatch.setattr(app, "keyboard", lambda: rec)
    app.cmd_type({"text": "hi"})
    assert rec.typed == ["hi"]


def test_hotkey_presses_then_releases_in_reverse(monkeypatch):
    rec = _RecKeyboard()
    monkeypatch.setattr(app, "keyboard", lambda: rec)
    app.cmd_hotkey({"keys": ["ctrl", "c"]})
    kinds = [e[0] for e in rec.events]
    assert kinds == ["press", "press", "release", "release"]


def test_resolve_key_maps_named_keys():
    from pynput.keyboard import Key
    assert app.resolve_key("enter") == Key.enter
    assert app.resolve_key("a") == "a"


class _RecMouse:
    def __init__(self):
        self.position = (0, 0)
        self.calls = []

    def click(self, button, count=1):
        self.calls.append(("click", str(button), count))

    def press(self, button):
        self.calls.append(("press", str(button)))

    def release(self, button):
        self.calls.append(("release", str(button)))

    def scroll(self, dx, dy):
        self.calls.append(("scroll", dx, dy))


def _patch_mouse(monkeypatch, native=(1456, 819)):
    rec = _RecMouse()
    monkeypatch.setattr(app, "mouse", lambda: rec)
    monkeypatch.setattr(app, "_primary_native", lambda: native)
    return rec


def test_click_moves_to_scaled_point_and_clicks(monkeypatch):
    rec = _patch_mouse(monkeypatch, native=(2912, 1638))  # 2x
    app.cmd_click({"x": 100, "y": 200})
    assert rec.position == (200, 400)
    assert rec.calls[0][0] == "click"


def test_double_click_count_two(monkeypatch):
    rec = _patch_mouse(monkeypatch)
    app.cmd_double_click({"x": 10, "y": 10})
    assert rec.calls[0][2] == 2


def test_scroll_direction_down_is_negative_dy(monkeypatch):
    rec = _patch_mouse(monkeypatch)
    app.cmd_scroll({"x": 10, "y": 10, "direction": "down", "amount": 3})
    assert ("scroll", 0, -3) in rec.calls


def test_nav_back_uses_keymap(monkeypatch):
    rec = _RecKeyboard()
    monkeypatch.setattr(app, "keyboard", lambda: rec)
    monkeypatch.setattr(app, "nav_keymap",
                        lambda: {"back": ["alt", "left"], "home": ["a"], "recents": ["b"]})
    app.cmd_back({})
    assert [e[0] for e in rec.events] == ["press", "press", "release", "release"]


def test_focus_window_not_found_returns_error(monkeypatch):
    monkeypatch.setattr(app, "_win_list", lambda: [])
    out = app.cmd_focus_window({"title": "nope"})
    assert "error" in out["content"][0]["text"]


def test_list_windows_maps_bounds_to_scaled_space(monkeypatch):
    monkeypatch.setattr(app, "_primary_native", lambda: (2912, 1638))  # 2x
    monkeypatch.setattr(app, "_win_list", lambda: [
        {"title": "A", "x": 200, "y": 400, "width": 1000, "height": 500, "index": 0, "state": "normal"}
    ])
    out = app.cmd_list_windows({})
    text = out["content"][0]["text"]
    assert '"x": 100' in text and '"y": 200' in text  # halved into 1456x819 space


def test_set_and_get_clipboard(monkeypatch):
    store = {"v": ""}
    monkeypatch.setattr(app, "_clip_set", lambda s: store.__setitem__("v", s))
    monkeypatch.setattr(app, "_clip_get", lambda: store["v"])
    app.cmd_set_clipboard({"text": "hello"})
    out = app.cmd_get_clipboard({})
    assert "hello" in out["content"][0]["text"]


def test_get_text_restores_original_clipboard(monkeypatch):
    store = {"v": "ORIGINAL"}
    monkeypatch.setattr(app, "_clip_set", lambda s: store.__setitem__("v", s))
    monkeypatch.setattr(app, "_clip_get", lambda: store["v"])
    # select_all + copy simulated: copy writes "SELECTED" into the clipboard
    monkeypatch.setattr(app, "cmd_select_all", lambda a: app._ok())

    def fake_copy(a):
        store["v"] = "SELECTED"
        return app._ok()

    monkeypatch.setattr(app, "cmd_copy", fake_copy)
    out = app.cmd_get_text({})
    assert "SELECTED" in out["content"][0]["text"]
    assert store["v"] == "ORIGINAL"  # restored


def test_camera_unsupported_when_cv2_missing(monkeypatch):
    monkeypatch.setattr(app, "_import_cv2", lambda: None)
    out = app.cmd_camera({})
    assert '"unsupported": true' in out["content"][0]["text"]


def test_get_screen_size_with_model_uses_provider_space(monkeypatch):
    monkeypatch.setattr(app, "_primary_native", lambda: (2560, 1440))
    out = app.cmd_get_screen_size({"model": "claude"})
    text = out["content"][0]["text"]
    assert '"width": 1445' in text and '"height": 813' in text
    assert '"original_width": 2560' in text


def test_click_with_model_scales_into_provider_space(monkeypatch):
    rec = _patch_mouse(monkeypatch, native=(2560, 1440))  # claude space 1445x813
    app.cmd_click({"x": 1445, "y": 813, "model": "claude"})
    assert rec.position == (2560, 1440)  # full-space corner maps to native corner


def test_explicit_maxwh_overrides_model(monkeypatch):
    rec = _patch_mouse(monkeypatch, native=(2560, 1440))
    app.cmd_click({"x": 100, "y": 100, "model": "claude", "max_width": 1280, "max_height": 720})
    assert rec.position == (200, 200)  # 2x of the explicit 1280x720 space, model ignored


def test_screenshot_with_model_resizes_to_provider_space(monkeypatch):
    monkeypatch.setattr(app, "grabber", lambda: _FakeGrabber())  # native 2912x1638
    captured = {}
    real_encode = app._encode_webp
    monkeypatch.setattr(app, "_encode_webp",
                        lambda shot, target=None: captured.setdefault("target", target) or real_encode(shot, target))
    app.cmd_screenshot({"model": "gemini"})  # 2912x1638 -> gemini 1920x1080
    assert captured["target"] == (1920, 1080)

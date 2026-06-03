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

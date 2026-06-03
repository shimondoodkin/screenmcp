import screenmcp_cli as app

# Canonical vectors — identical across all clients (see windows/src/provider_sizing.rs
# and docs/model-sizing.md). (w, h, claude, gemini, chatgpt).
VECTORS = [
    (2560, 1440, (1445, 813), (1920, 1080), (1360, 768)),
    (1920, 1080, (1445, 813), (1920, 1080), (1360, 768)),
    (3840, 2160, (1445, 813), (1920, 1080), (1360, 768)),
    (1080, 2400, (705, 1568), (864, 1920), (768, 1712)),
    (1440, 3120, (723, 1568), (886, 1920), (768, 1664)),
    (1080, 3000, (564, 1567), (691, 1920), (736, 2048)),
    (1000, 1000, (1000, 1000), (1000, 1000), (768, 768)),
    (640, 480, (640, 480), (640, 480), (640, 480)),
]


def test_matches_canonical_table():
    for w, h, c, g, o in VECTORS:
        assert app.provider_default_size("claude", w, h) == c, f"claude {w}x{h}"
        assert app.provider_default_size("gemini", w, h) == g, f"gemini {w}x{h}"
        assert app.provider_default_size("chatgpt", w, h) == o, f"chatgpt {w}x{h}"


def test_claude_never_exceeds_caps():
    for w, h, _, _, _ in VECTORS:
        mw, mh = app.provider_default_size("claude", w, h)
        assert mw * mh <= 1_176_000, f"{w}x{h} -> {mw}x{mh}"
        assert max(mw, mh) <= 1568, f"{w}x{h} long edge"


def test_chatgpt_outputs_multiples_of_16():
    for w, h, _, _, _ in VECTORS:
        mw, mh = app.provider_default_size("chatgpt", w, h)
        assert mw % 16 == 0 and mh % 16 == 0, f"{w}x{h} -> {mw}x{mh}"


def test_unknown_model_returns_none():
    assert app.provider_default_size("gpt-5", 1920, 1080) is None

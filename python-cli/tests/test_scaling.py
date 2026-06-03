import screenmcp_cli as app


def test_scaled_space_defaults():
    assert app.scaled_space(None, None) == (1456, 819)


def test_scaled_space_override():
    assert app.scaled_space(728, 0) == (728, 0)


def test_to_native_scales_up():
    # screenshot space 1456x819, native 2912x1638 -> 2x
    nx, ny = app.to_native(100, 200, native=(2912, 1638), maxw=None, maxh=None)
    assert (nx, ny) == (200, 400)


def test_to_native_zero_dim_means_native_passthrough():
    nx, ny = app.to_native(100, 200, native=(2912, 1638), maxw=0, maxh=0)
    assert (nx, ny) == (100, 200)


def test_to_native_independent_axes():
    nx, ny = app.to_native(728, 819, native=(1456, 1638), maxw=None, maxh=None)
    assert (nx, ny) == (728, 1638)

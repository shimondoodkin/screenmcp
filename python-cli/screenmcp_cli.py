#!/usr/bin/env python3
# === IMPORTS & CONSTANTS ===
import sys
import json

VERSION = "0.1.0"
SERVER_NAME = "screenmcp-cli"
DEFAULT_W = 1456
DEFAULT_H = 819

# Populated in the REGISTRY section near the bottom of the file.
TOOLS = {}


# === COORDINATE SCALING ===
def scaled_space(maxw, maxh):
    """Resolve the screenshot-space dimensions. None -> default; 0 kept as 0 (native)."""
    w = DEFAULT_W if maxw is None else maxw
    h = DEFAULT_H if maxh is None else maxh
    return (w, h)


def _axis(coord, native_dim, scaled_dim):
    if scaled_dim == 0:           # 0 => native passthrough, no scaling
        return int(round(coord))
    return int(round(coord * native_dim / scaled_dim))


def to_native(x, y, native, maxw=None, maxh=None):
    """Map a coordinate from screenshot space to native screen pixels."""
    sw, sh = scaled_space(maxw, maxh)
    return (_axis(x, native[0], sw), _axis(y, native[1], sh))


def provider_default_size(model, w, h):
    """Model-tuned default screenshot size from real screen w x h, or None if unknown.
    Shared algorithm with the desktop clients — see docs/model-sizing.md."""
    wf, hf = float(w), float(h)
    if model == "claude":
        max_pixels, max_edge = 1_176_000.0, 1568.0
        s = min(1.0, max_edge / max(wf, hf), (max_pixels / (wf * hf)) ** 0.5)
        mw, mh = int(wf * s), int(hf * s)  # floor
        while mw * mh > max_pixels and (mw > 1 or mh > 1):
            if mw >= mh:
                mw -= 1
            else:
                mh -= 1
        return (mw, mh)
    if model == "gemini":
        short_cap, long_cap = 1080.0, 1920.0
        if w >= h:
            s = min(1.0, long_cap / wf, short_cap / hf)
        else:
            s = min(1.0, long_cap / hf, short_cap / wf)
        return (round(wf * s), round(hf * s))
    if model == "chatgpt":
        short = min(wf, hf)
        s = min(1.0, 768.0 / short)
        long = max(wf, hf)
        if long * s > 2048.0:
            s = 2048.0 / long
        return (_round16(wf * s), _round16(hf * s))
    return None


def _round16(x):
    return max(16, round(x / 16.0) * 16)


def resolve_space(args, native):
    """Effective (width, height) screenshot space for a coordinate command.
    Model applies only when neither max_width nor max_height is given; an explicit
    override or unknown model falls back to scaled_space (default 1456x819)."""
    maxw, maxh = args.get("max_width"), args.get("max_height")
    if maxw is None and maxh is None and args.get("model"):
        sized = provider_default_size(args["model"], native[0], native[1])
        if sized:
            return sized
    return scaled_space(maxw, maxh)


def to_native_space(x, y, native, space):
    """Map a coordinate from a resolved screenshot space to native screen pixels."""
    return (_axis(x, native[0], space[0]), _axis(y, native[1], space[1]))


# === PLATFORM DETECTION ===
IS_WIN = sys.platform.startswith("win")
IS_MAC = sys.platform == "darwin"
IS_LINUX = sys.platform.startswith("linux")

PRIMARY_MOD = "cmd" if IS_MAC else "ctrl"


def nav_keymap():
    if IS_MAC:
        return {"back": ["cmd", "["], "home": ["f3"], "recents": ["cmd", "tab"]}
    if IS_WIN:
        return {"back": ["alt", "left"], "home": ["cmd"], "recents": ["alt", "tab"]}
    return {"back": ["alt", "left"], "home": ["cmd"], "recents": ["alt", "tab"]}


# Lazy backend singletons (overridable in tests).
_mouse = None
_keyboard = None
_grabber = None


def mouse():
    global _mouse
    if _mouse is None:
        from pynput.mouse import Controller
        _mouse = Controller()
    return _mouse


def keyboard():
    global _keyboard
    if _keyboard is None:
        from pynput.keyboard import Controller
        _keyboard = Controller()
    return _keyboard


def grabber():
    global _grabber
    if _grabber is None:
        import mss
        _grabber = mss.mss()
    return _grabber


# === VISION ===
import base64 as _b64
from io import BytesIO


def _primary_native():
    """Native (width, height) of the primary monitor."""
    mon = grabber().monitors[1]
    return (mon["width"], mon["height"])


def _encode_webp(shot, target=None):
    """shot: mss screenshot. target: optional (w,h) to resize the encoded image to."""
    from PIL import Image
    img = Image.frombytes("RGB", shot.size, shot.rgb)
    if target and target[0] and target[1]:
        img = img.resize(target)
    buf = BytesIO()
    img.save(buf, format="WEBP")
    return _b64.b64encode(buf.getvalue()).decode("ascii")


def _image_result(b64):
    return {"content": [{"type": "image", "data": b64, "mimeType": "image/webp"}]}


def _text_result(obj):
    return {"content": [{"type": "text", "text": json.dumps(obj, indent=2)}]}


def cmd_get_screen_size(args):
    nw, nh = _primary_native()
    sw, sh = resolve_space(args, (nw, nh))
    return _text_result({"width": sw or nw, "height": sh or nh,
                         "original_width": nw, "original_height": nh})


def cmd_screenshot(args):
    native = _primary_native()
    sw, sh = resolve_space(args, native)
    mon = grabber().monitors[1]
    shot = grabber().grab(mon)
    target = None if (sw == 0 or sh == 0) else (sw, sh)
    return _image_result(_encode_webp(shot, target))


def cmd_screenshot_region(args):
    native = _primary_native()
    space = resolve_space(args, native)
    x1, y1 = to_native_space(args["min_x"], args["min_y"], native, space)
    x2, y2 = to_native_space(args["max_x"], args["max_y"], native, space)
    region = {"left": x1, "top": y1, "width": max(1, x2 - x1), "height": max(1, y2 - y1)}
    shot = grabber().grab(region)
    return _image_result(_encode_webp(shot))  # native resolution, no resize


def cmd_active_window(args):
    info = active_window_info()  # provided by the WINDOW MANAGEMENT section
    return _text_result(info)


def cmd_screenshot_window(args):
    win = find_window(args.get("title"), args.get("index"))  # WINDOW MANAGEMENT section
    if win is None:
        return _text_result({"status": "error", "error": "window not found"})
    region = {"left": win["x"], "top": win["y"], "width": win["width"], "height": win["height"]}
    shot = grabber().grab(region)
    return _image_result(_encode_webp(shot))


# === KEYBOARD ===
def resolve_key(name):
    """Map a key name to a pynput key object, or return a 1-char string as-is."""
    from pynput.keyboard import Key
    name = str(name).lower()
    special = {
        "shift": Key.shift, "ctrl": Key.ctrl, "control": Key.ctrl,
        "alt": Key.alt, "meta": Key.cmd, "win": Key.cmd, "cmd": Key.cmd,
        "super": Key.cmd, "tab": Key.tab, "enter": Key.enter, "return": Key.enter,
        "escape": Key.esc, "esc": Key.esc, "space": Key.space,
        "backspace": Key.backspace, "delete": Key.delete, "del": Key.delete,
        "home": Key.home, "end": Key.end, "pageup": Key.page_up,
        "pagedown": Key.page_down, "up": Key.up, "down": Key.down,
        "left": Key.left, "right": Key.right,
    }
    if name in special:
        return special[name]
    if len(name) > 1 and name[0] == "f" and name[1:].isdigit():
        return getattr(Key, name)  # f1..f12
    return name  # single character


def _ok(extra=None):
    obj = {"status": "ok"}
    if extra:
        obj.update(extra)
    return _text_result(obj)


def cmd_type(args):
    keyboard().type(args.get("text", ""))
    return _ok()


def cmd_press_key(args):
    k = resolve_key(args["key"])
    keyboard().press(k)
    keyboard().release(k)
    return _ok()


def cmd_hold_key(args):
    keyboard().press(resolve_key(args["key"]))
    return _ok()


def cmd_release_key(args):
    keyboard().release(resolve_key(args["key"]))
    return _ok()


def cmd_hotkey(args):
    keys = [resolve_key(k) for k in args["keys"]]
    for k in keys:
        keyboard().press(k)
    for k in reversed(keys):
        keyboard().release(k)
    return _ok()


# === MOUSE ===
import time as _time


def _move(args):
    native = _primary_native()
    space = resolve_space(args, native)
    nx, ny = to_native_space(args["x"], args["y"], native, space)
    mouse().position = (nx, ny)
    return nx, ny


def _button(name="left"):
    from pynput.mouse import Button
    return {"left": Button.left, "right": Button.right, "middle": Button.middle}[name]


def cmd_mouse_move(args):
    _move(args)
    return _ok()


def cmd_click(args):
    _move(args)
    mouse().click(_button("left"), 1)
    return _ok()


def cmd_right_click(args):
    _move(args)
    mouse().click(_button("right"), 1)
    return _ok()


def cmd_middle_click(args):
    _move(args)
    mouse().click(_button("middle"), 1)
    return _ok()


def cmd_double_click(args):
    _move(args)
    mouse().click(_button("left"), 2)
    return _ok()


def cmd_long_click(args):
    _move(args)
    mouse().press(_button("left"))
    _time.sleep(args.get("duration", 1000) / 1000.0)
    mouse().release(_button("left"))
    return _ok()


def cmd_drag(args):
    native = _primary_native()
    space = resolve_space(args, native)
    sx, sy = to_native_space(args["startX"], args["startY"], native, space)
    ex, ey = to_native_space(args["endX"], args["endY"], native, space)
    mouse().position = (sx, sy)
    mouse().press(_button("left"))
    _time.sleep(args.get("duration", 300) / 1000.0)
    mouse().position = (ex, ey)
    mouse().release(_button("left"))
    return _ok()


def cmd_scroll(args):
    _move(args)
    direction = args.get("direction")
    if direction:
        amount = args.get("amount", 3)
        dx, dy = {"up": (0, amount), "down": (0, -amount),
                  "left": (-amount, 0), "right": (amount, 0)}[direction]
    else:
        dx, dy = args.get("dx", 0), args.get("dy", 0)
    mouse().scroll(dx, dy)
    return _ok()


def cmd_mouse_scroll(args):
    return cmd_scroll(args)


# === WINDOW MANAGEMENT ===
def _win_list():
    """Return list of native-pixel window dicts: {title,x,y,width,height,index,state}."""
    if IS_WIN:
        return _win_list_windows()
    if IS_MAC:
        return _win_list_mac()
    if IS_LINUX:
        return _win_list_linux()
    return []


def _win_list_windows():
    import pygetwindow as gw
    out = []
    for i, w in enumerate(gw.getAllWindows()):
        if not w.title:
            continue
        state = "minimized" if w.isMinimized else ("maximized" if w.isMaximized else "normal")
        out.append({"title": w.title, "x": w.left, "y": w.top,
                    "width": w.width, "height": w.height, "index": i, "state": state})
    return out


def _win_list_mac():
    try:
        from Quartz import (CGWindowListCopyWindowInfo, kCGWindowListOptionOnScreenOnly,
                            kCGNullWindowID)
    except Exception:
        return []
    info = CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID)
    out = []
    for i, w in enumerate(info or []):
        name = w.get("kCGWindowName") or w.get("kCGWindowOwnerName") or ""
        b = w.get("kCGWindowBounds") or {}
        if not name:
            continue
        out.append({"title": name, "x": int(b.get("X", 0)), "y": int(b.get("Y", 0)),
                    "width": int(b.get("Width", 0)), "height": int(b.get("Height", 0)),
                    "index": i, "state": "normal"})
    return out


def _win_list_linux():
    import subprocess
    try:
        raw = subprocess.check_output(["wmctrl", "-lG"], text=True)
    except Exception:
        return []
    out = []
    for i, line in enumerate(raw.splitlines()):
        parts = line.split(None, 7)
        if len(parts) < 8:
            continue
        _, _, x, y, w, h, _, title = parts
        out.append({"title": title, "x": int(x), "y": int(y),
                    "width": int(w), "height": int(h), "index": i, "state": "normal"})
    return out


def _scale_bounds_to_space(win):
    nw, nh = _primary_native()
    fx, fy = (DEFAULT_W / nw if nw else 1), (DEFAULT_H / nh if nh else 1)
    return {**win,
            "x": int(round(win["x"] * fx)), "y": int(round(win["y"] * fy)),
            "width": int(round(win["width"] * fx)), "height": int(round(win["height"] * fy))}


def find_window(title=None, index=None):
    wins = _win_list()
    if index is not None and 0 <= index < len(wins):
        return wins[index]
    if title:
        for w in wins:
            if title.lower() in w["title"].lower():
                return w
    return None


def active_window_info():
    if IS_WIN:
        import pygetwindow as gw
        w = gw.getActiveWindow()
        if not w:
            return {"status": "error", "error": "no active window"}
        return {"title": w.title, "x": w.left, "y": w.top, "width": w.width, "height": w.height}
    if IS_MAC:
        wins = _win_list_mac()
        return wins[0] if wins else {"status": "error", "error": "no active window"}
    if IS_LINUX:
        import subprocess
        try:
            name = subprocess.check_output(
                ["xdotool", "getactivewindow", "getwindowname"], text=True).strip()
            return {"title": name}
        except Exception:
            return {"status": "ok", "unsupported": True, "reason": "xdotool not available"}
    return {"status": "ok", "unsupported": True, "reason": "unsupported platform"}


def cmd_list_windows(args):
    return _text_result({"windows": [_scale_bounds_to_space(w) for w in _win_list()]})


def cmd_focus_window(args):
    win = find_window(args.get("title"), args.get("index"))
    if win is None:
        return _text_result({"status": "error", "error": "window not found"})
    if IS_WIN:
        import pygetwindow as gw
        target = gw.getWindowsWithTitle(win["title"])
        if target:
            try:
                target[0].activate()
            except Exception:
                target[0].minimize()
                target[0].restore()
    elif IS_MAC:
        import subprocess
        subprocess.run(["osascript", "-e",
                        f'tell application "System Events" to set frontmost of '
                        f'(first process whose name contains "{win["title"]}") to true'],
                       check=False)
    elif IS_LINUX:
        import subprocess
        subprocess.run(["wmctrl", "-a", win["title"]], check=False)
    return _ok({"focused": win["title"]})


# === NAVIGATION ===
def _send_combo(keys):
    resolved = [resolve_key(k) for k in keys]
    for k in resolved:
        keyboard().press(k)
    for k in reversed(resolved):
        keyboard().release(k)


def cmd_back(args):
    _send_combo(nav_keymap()["back"])
    return _ok()


def cmd_home(args):
    _send_combo(nav_keymap()["home"])
    return _ok()


def cmd_recents(args):
    _send_combo(nav_keymap()["recents"])
    return _ok()


# === CLIPBOARD / TEXT ===
def _clip_get():
    import pyperclip
    return pyperclip.paste()


def _clip_set(text):
    import pyperclip
    pyperclip.copy(text)


def cmd_get_clipboard(args):
    return _text_result({"text": _clip_get()})


def cmd_set_clipboard(args):
    _clip_set(args.get("text", ""))
    return _ok()


def cmd_select_all(args):
    _send_combo([PRIMARY_MOD, "a"])
    return _ok()


def cmd_copy(args):
    _send_combo([PRIMARY_MOD, "c"])
    _time.sleep(0.05)
    if args.get("return_text"):
        return _text_result({"text": _clip_get()})
    return _ok()


def cmd_paste(args):
    if "text" in args:
        _clip_set(args["text"])
    _send_combo([PRIMARY_MOD, "v"])
    return _ok()


def cmd_get_text(args):
    original = _clip_get()
    try:
        cmd_select_all({})
        cmd_copy({})
        _time.sleep(0.05)
        text = _clip_get()
    finally:
        _clip_set(original)  # restore — heuristic must not clobber clipboard
    return _text_result({"text": text})


# === SYSTEM ===
import os as _os


def cmd_is_elevated(args):
    if IS_WIN:
        try:
            import ctypes
            elevated = bool(ctypes.windll.shell32.IsUserAnAdmin())
        except Exception:
            elevated = False
    else:
        elevated = (hasattr(_os, "geteuid") and _os.geteuid() == 0)
    return _text_result({"elevated": elevated})


def cmd_elevate(args):
    return _text_result({"status": "ok", "unsupported": True,
                         "reason": "cannot re-launch a live stdio process elevated"})


def _import_cv2():
    try:
        import cv2
        return cv2
    except Exception:
        return None


def cmd_list_cameras(args):
    cv2 = _import_cv2()
    if cv2 is None:
        return _text_result({"status": "ok", "unsupported": True, "reason": "opencv-python not installed"})
    found = []
    for i in range(5):
        cap = cv2.VideoCapture(i)
        if cap is not None and cap.isOpened():
            found.append(i)
            cap.release()
    return _text_result({"cameras": found})


def cmd_camera(args):
    cv2 = _import_cv2()
    if cv2 is None:
        return _text_result({"status": "ok", "unsupported": True, "reason": "opencv-python not installed"})
    index = args.get("index", 0)
    cap = cv2.VideoCapture(index)
    try:
        ok, frame = cap.read()
        if not ok:
            return _text_result({"status": "error", "error": f"camera {index} read failed"})
        ok, buf = cv2.imencode(".webp", frame)
        return _image_result(_b64.b64encode(buf.tobytes()).decode("ascii"))
    finally:
        cap.release()


def cmd_play_audio(args):
    try:
        import simpleaudio
    except Exception:
        return _text_result({"status": "ok", "unsupported": True, "reason": "simpleaudio not installed"})
    data = _b64.b64decode(args["audio"])
    wave_obj = simpleaudio.WaveObject(data, num_channels=args.get("channels", 2),
                                      bytes_per_sample=args.get("bytes_per_sample", 2),
                                      sample_rate=args.get("sample_rate", 44100))
    wave_obj.play().wait_done()
    return _ok()


# === REGISTRY ===
def _schema(props=None, required=None):
    return {"type": "object", "properties": props or {}, "required": required or []}


_XY = {"x": {"type": "number"}, "y": {"type": "number"}}
_MODEL = {"model": {"type": "string",
                    "description": "claude|gemini|chatgpt — auto-sizes the screenshot and "
                                   "coordinate space to that model's vision limits when no "
                                   "explicit max_width/max_height is given."}}
_MAXWH = {"max_width": {"type": "number"}, "max_height": {"type": "number"}, **_MODEL}

TOOLS.update({
    "screenshot": {"description": "Take a screenshot (default 1456x819, WebP).",
                   "inputSchema": _schema(_MAXWH), "handler": cmd_screenshot},
    "screenshot_region": {"description": "Capture a region at native resolution.",
                          "inputSchema": _schema({**_MAXWH, "min_x": {"type": "number"},
                                                  "min_y": {"type": "number"}, "max_x": {"type": "number"},
                                                  "max_y": {"type": "number"}},
                                                 ["min_x", "min_y", "max_x", "max_y"]),
                          "handler": cmd_screenshot_region},
    "screenshot_window": {"description": "Capture a specific window by title or index.",
                          "inputSchema": _schema({"title": {"type": "string"}, "index": {"type": "number"}}),
                          "handler": cmd_screenshot_window},
    "active_window": {"description": "Get the currently focused window info.",
                      "inputSchema": _schema(), "handler": cmd_active_window},
    "get_screen_size": {"description": "Get screen dimensions (scaled + native).",
                        "inputSchema": _schema(_MAXWH), "handler": cmd_get_screen_size},
    "click": {"description": "Left-click at (x, y).",
              "inputSchema": _schema({**_XY, **_MAXWH}, ["x", "y"]), "handler": cmd_click},
    "right_click": {"description": "Right-click at (x, y).",
                    "inputSchema": _schema({**_XY, **_MAXWH}, ["x", "y"]), "handler": cmd_right_click},
    "double_click": {"description": "Double-click at (x, y).",
                     "inputSchema": _schema({**_XY, **_MAXWH}, ["x", "y"]), "handler": cmd_double_click},
    "middle_click": {"description": "Middle-click at (x, y).",
                     "inputSchema": _schema({**_XY, **_MAXWH}, ["x", "y"]), "handler": cmd_middle_click},
    "long_click": {"description": "Long press at (x, y). Optional duration ms (default 1000).",
                   "inputSchema": _schema({**_XY, **_MAXWH, "duration": {"type": "number"}}, ["x", "y"]),
                   "handler": cmd_long_click},
    "mouse_move": {"description": "Move cursor to (x, y) without clicking.",
                   "inputSchema": _schema({**_XY, **_MAXWH}, ["x", "y"]), "handler": cmd_mouse_move},
    "drag": {"description": "Drag from (startX,startY) to (endX,endY). Optional duration ms.",
             "inputSchema": _schema({"startX": {"type": "number"}, "startY": {"type": "number"},
                                     "endX": {"type": "number"}, "endY": {"type": "number"},
                                     "duration": {"type": "number"}, **_MAXWH},
                                    ["startX", "startY", "endX", "endY"]), "handler": cmd_drag},
    "scroll": {"description": "Scroll at (x,y) with dx/dy, or direction+amount.",
               "inputSchema": _schema({**_XY, "dx": {"type": "number"}, "dy": {"type": "number"},
                                       "direction": {"type": "string"}, "amount": {"type": "number"}, **_MAXWH},
                                      ["x", "y"]), "handler": cmd_scroll},
    "mouse_scroll": {"description": "Raw mouse wheel scroll at coordinates.",
                     "inputSchema": _schema({**_XY, "dx": {"type": "number"}, "dy": {"type": "number"}, **_MAXWH},
                                            ["x", "y"]), "handler": cmd_mouse_scroll},
    "type": {"description": "Type text into the focused field.",
             "inputSchema": _schema({"text": {"type": "string"}}, ["text"]), "handler": cmd_type},
    "press_key": {"description": "Press and release a single key.",
                  "inputSchema": _schema({"key": {"type": "string"}}, ["key"]), "handler": cmd_press_key},
    "hold_key": {"description": "Hold a key down.",
                 "inputSchema": _schema({"key": {"type": "string"}}, ["key"]), "handler": cmd_hold_key},
    "release_key": {"description": "Release a held key.",
                    "inputSchema": _schema({"key": {"type": "string"}}, ["key"]), "handler": cmd_release_key},
    "hotkey": {"description": "Press a key combination atomically, e.g. [ctrl, c].",
               "inputSchema": _schema({"keys": {"type": "array", "items": {"type": "string"}}}, ["keys"]),
               "handler": cmd_hotkey},
    "get_text": {"description": "Get text from focused field (clipboard heuristic, restores clipboard).",
                 "inputSchema": _schema(), "handler": cmd_get_text},
    "select_all": {"description": "Select all text (primary modifier + A).",
                   "inputSchema": _schema(), "handler": cmd_select_all},
    "copy": {"description": "Copy selection. Set return_text:true to get the text.",
             "inputSchema": _schema({"return_text": {"type": "boolean"}}), "handler": cmd_copy},
    "paste": {"description": "Paste. Optionally pass text to set clipboard first.",
              "inputSchema": _schema({"text": {"type": "string"}}), "handler": cmd_paste},
    "get_clipboard": {"description": "Read clipboard contents.",
                      "inputSchema": _schema(), "handler": cmd_get_clipboard},
    "set_clipboard": {"description": "Set clipboard to given text.",
                      "inputSchema": _schema({"text": {"type": "string"}}, ["text"]), "handler": cmd_set_clipboard},
    "list_windows": {"description": "List visible windows (bounds in screenshot space).",
                     "inputSchema": _schema(), "handler": cmd_list_windows},
    "focus_window": {"description": "Bring a window to front by title (substring) or index.",
                     "inputSchema": _schema({"title": {"type": "string"}, "index": {"type": "number"}}),
                     "handler": cmd_focus_window},
    "back": {"description": "Back navigation (platform hotkey).",
             "inputSchema": _schema(), "handler": cmd_back},
    "home": {"description": "Home / Start (platform hotkey).",
             "inputSchema": _schema(), "handler": cmd_home},
    "recents": {"description": "Recent windows / app switcher.",
                "inputSchema": _schema(), "handler": cmd_recents},
    "is_elevated": {"description": "Check if running with admin privileges.",
                    "inputSchema": _schema(), "handler": cmd_is_elevated},
    "elevate": {"description": "Request admin privileges (unsupported in stdio mode).",
                "inputSchema": _schema(), "handler": cmd_elevate},
    "camera": {"description": "Capture a frame from a camera (requires [camera] extra).",
               "inputSchema": _schema({"index": {"type": "number"}}), "handler": cmd_camera},
    "list_cameras": {"description": "List available cameras (requires [camera] extra).",
                     "inputSchema": _schema(), "handler": cmd_list_cameras},
    "play_audio": {"description": "Play base64 WAV audio (requires [audio] extra).",
                   "inputSchema": _schema({"audio": {"type": "string"}, "sample_rate": {"type": "number"},
                                           "channels": {"type": "number"}, "bytes_per_sample": {"type": "number"}},
                                          ["audio"]), "handler": cmd_play_audio},
})


# === JSON-RPC STDIO SERVER ===
def _result(rpc_id, result):
    return {"jsonrpc": "2.0", "id": rpc_id, "result": result}


def _error(rpc_id, code, message):
    return {"jsonrpc": "2.0", "id": rpc_id, "error": {"code": code, "message": message}}


def _handle(msg):
    """Return a response dict, or None for notifications/no-reply."""
    method = msg.get("method")
    rpc_id = msg.get("id")
    if method == "initialize":
        return _result(rpc_id, {
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": SERVER_NAME, "version": VERSION},
        })
    if method == "notifications/initialized":
        return None
    if method == "ping":
        return _result(rpc_id, {})
    if method == "tools/list":
        return _result(rpc_id, {"tools": _list_tools()})
    if method == "tools/call":
        return _call_tool(rpc_id, msg.get("params") or {})
    if rpc_id is None:
        return None
    return _error(rpc_id, -32601, f"Method not found: {method}")


def _list_tools():
    return [
        {"name": n, "description": t["description"], "inputSchema": t["inputSchema"]}
        for n, t in TOOLS.items()
    ]


def _call_tool(rpc_id, params):
    name = params.get("name")
    args = params.get("arguments") or {}
    tool = TOOLS.get(name)
    if tool is None:
        return _error(rpc_id, -32602, f"Unknown tool: {name}")
    try:
        return _result(rpc_id, tool["handler"](args))
    except Exception as exc:  # surfaced as an MCP tool error, not a transport error
        return _result(rpc_id, {
            "content": [{"type": "text", "text": f"{type(exc).__name__}: {exc}"}],
            "isError": True,
        })


def serve(stdin, stdout):
    for raw in stdin:
        raw = raw.strip()
        if not raw:
            continue
        try:
            msg = json.loads(raw)
        except json.JSONDecodeError:
            stdout.write(json.dumps(_error(None, -32700, "Parse error")) + "\n")
            stdout.flush()
            continue
        resp = _handle(msg)
        if resp is not None:
            stdout.write(json.dumps(resp) + "\n")
            stdout.flush()


# === MAIN ===
def run():
    serve(sys.stdin, sys.stdout)


if __name__ == "__main__":
    run()

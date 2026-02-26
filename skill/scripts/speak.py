#!/usr/bin/env python3
import argparse
import base64
import json
import os
import subprocess
import sys
from urllib import request


def tts_mp3(text: str, api_key: str) -> bytes:
    payload = {
        "model": "gpt-4o-mini-tts",
        "voice": "alloy",
        "input": text,
        "response_format": "mp3",
    }
    req = request.Request(
        "https://api.openai.com/v1/audio/speech",
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    with request.urlopen(req, timeout=60) as resp:
        audio = resp.read()

    # ScreenMCP parser expects MP3 header pattern with 0xFFFB
    b = bytearray(audio)
    if len(b) > 1 and b[0] == 0xFF and b[1] == 0xF3:
        b[1] = 0xFB

    return bytes(b)


def play_audio(device_id: int, audio_b64: str, volume: float):
    cmd = [
        "mcporter",
        "call",
        "screenmcp.play_audio",
        f"device_id:{device_id}",
        f"audio_data:{audio_b64}",
        f"volume:{volume}",
        "--output",
        "json",
    ]
    res = subprocess.run(cmd, capture_output=True, text=True)
    ok = res.returncode == 0 and "isError" not in (res.stdout or "")
    return ok, (res.stdout or res.stderr).strip()


def list_connected_devices():
    res = subprocess.run(
        ["mcporter", "call", "screenmcp.list_devices", "--output", "json"],
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        return []
    try:
        data = json.loads(res.stdout)
        return [d["device_number"] for d in data.get("devices", []) if d.get("connected")]
    except Exception:
        return []


def main():
    p = argparse.ArgumentParser(description="Speak text on a ScreenMCP device")
    p.add_argument("device_id", type=int, help="Target device number (e.g. 2 or 3)")
    p.add_argument("text", help="Text to speak")
    p.add_argument("--volume", type=float, default=0.95)
    p.add_argument("--fallback", action="store_true", help="Try other connected devices if target fails")
    args = p.parse_args()

    api_key = os.getenv("OPENAI_API_KEY")
    if not api_key:
        print("ERROR: OPENAI_API_KEY is not set", file=sys.stderr)
        sys.exit(1)

    # Keep short so base64 arg doesn't exceed shell limits
    if len(args.text) > 220:
        print("ERROR: text too long (>220 chars). Keep messages short.", file=sys.stderr)
        sys.exit(1)

    mp3_bytes = tts_mp3(args.text, api_key)
    b64 = base64.b64encode(mp3_bytes).decode("ascii")

        targets = [args.device_id]
        if args.fallback:
            others = [x for x in list_connected_devices() if x != args.device_id]
            targets.extend(others)

        errors = {}
        for device in targets:
            ok, out = play_audio(device, b64, args.volume)
            if ok:
                print(json.dumps({"ok": True, "device_id": device, "message": "played"}))
                return
            errors[device] = out

        print(json.dumps({"ok": False, "errors": errors}, ensure_ascii=False), file=sys.stderr)
        sys.exit(2)


if __name__ == "__main__":
    main()

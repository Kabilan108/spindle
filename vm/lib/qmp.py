"""Minimal QMP client for the vm/* scripts: screendump, pointer and keyboard."""

import argparse
import json
import socket
import sys
import time

ABS_MAX = 0x7FFF

ALIASES = {
    "super": "meta_l",
    "mod": "meta_l",
    "win": "meta_l",
    "meta": "meta_l",
    "enter": "ret",
    "return": "ret",
    "space": "spc",
    "escape": "esc",
    "control": "ctrl",
    "del": "delete",
    "pageup": "pgup",
    "pagedown": "pgdn",
    "bksp": "backspace",
}

PLAIN_CHARS = {
    " ": "spc",
    "\n": "ret",
    "\t": "tab",
    "-": "minus",
    "=": "equal",
    "[": "bracket_left",
    "]": "bracket_right",
    ";": "semicolon",
    "'": "apostrophe",
    "`": "grave_accent",
    "\\": "backslash",
    ",": "comma",
    ".": "dot",
    "/": "slash",
}

SHIFTED_CHARS = {
    "!": "1",
    "@": "2",
    "#": "3",
    "$": "4",
    "%": "5",
    "^": "6",
    "&": "7",
    "*": "8",
    "(": "9",
    ")": "0",
    "_": "minus",
    "+": "equal",
    "{": "bracket_left",
    "}": "bracket_right",
    ":": "semicolon",
    '"': "apostrophe",
    "~": "grave_accent",
    "|": "backslash",
    "<": "comma",
    ">": "dot",
    "?": "slash",
}


class Qmp:
    def __init__(self, path: str) -> None:
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(30)
        self.sock.connect(path)
        self.reader = self.sock.makefile("r")
        json.loads(self.reader.readline())
        self.execute("qmp_capabilities")

    def execute(self, command: str, arguments: dict | None = None) -> object:
        message: dict = {"execute": command}
        if arguments is not None:
            message["arguments"] = arguments
        self.sock.sendall(json.dumps(message).encode() + b"\n")
        while True:
            reply = json.loads(self.reader.readline())
            if "event" in reply:
                continue
            if "error" in reply:
                raise SystemExit(f"qmp {command}: {reply['error'].get('desc', reply['error'])}")
            return reply.get("return")


def to_abs(value: int, size: int) -> int:
    if not 0 <= value < size:
        raise SystemExit(f"coordinate {value} is outside 0..{size - 1}")
    return round(value * ABS_MAX / (size - 1))


def move(qmp: Qmp, args: argparse.Namespace, x: int, y: int) -> None:
    qmp.execute(
        "input-send-event",
        {
            "events": [
                {"type": "abs", "data": {"axis": "x", "value": to_abs(x, args.width)}},
                {"type": "abs", "data": {"axis": "y", "value": to_abs(y, args.height)}},
            ]
        },
    )


def button(qmp: Qmp, name: str, down: bool) -> None:
    qmp.execute(
        "input-send-event",
        {"events": [{"type": "btn", "data": {"down": down, "button": name}}]},
    )


def qcode(name: str) -> dict:
    name = name.lower()
    return {"type": "qcode", "data": ALIASES.get(name, name)}


def char_keys(char: str) -> list[dict]:
    if char.isascii() and char.isalnum():
        keys = [qcode(char.lower())]
        return [qcode("shift"), *keys] if char.isupper() else keys
    if char in PLAIN_CHARS:
        return [qcode(PLAIN_CHARS[char])]
    if char in SHIFTED_CHARS:
        return [qcode("shift"), qcode(SHIFTED_CHARS[char])]
    raise SystemExit(f"cannot type {char!r} through QMP; use vm/run wtype instead")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--socket", required=True)
    parser.add_argument("--width", type=int, required=True)
    parser.add_argument("--height", type=int, required=True)
    sub = parser.add_subparsers(dest="command", required=True)

    shot = sub.add_parser("screendump")
    shot.add_argument("path")

    for name in ("move", "click"):
        p = sub.add_parser(name)
        p.add_argument("x", type=int)
        p.add_argument("y", type=int)
        if name == "click":
            p.add_argument("--button", default="left", choices=["left", "right", "middle"])
            p.add_argument("--count", type=int, default=1)

    scroll = sub.add_parser("scroll")
    scroll.add_argument("x", type=int)
    scroll.add_argument("y", type=int)
    scroll.add_argument("direction", choices=["up", "down"])
    scroll.add_argument("steps", type=int, nargs="?", default=1)

    key = sub.add_parser("key")
    key.add_argument("combos", nargs="+")
    key.add_argument("--hold-ms", type=int, default=80)

    typ = sub.add_parser("type")
    typ.add_argument("text")

    sub.add_parser("quit")
    sub.add_parser("status")

    args = parser.parse_args()
    qmp = Qmp(args.socket)

    if args.command == "screendump":
        qmp.execute("screendump", {"filename": args.path, "format": "png"})
    elif args.command == "move":
        move(qmp, args, args.x, args.y)
    elif args.command == "click":
        move(qmp, args, args.x, args.y)
        time.sleep(0.05)
        for _ in range(args.count):
            button(qmp, args.button, True)
            time.sleep(0.05)
            button(qmp, args.button, False)
            time.sleep(0.08)
    elif args.command == "scroll":
        move(qmp, args, args.x, args.y)
        time.sleep(0.05)
        name = "wheel-up" if args.direction == "up" else "wheel-down"
        for _ in range(args.steps):
            button(qmp, name, True)
            button(qmp, name, False)
            time.sleep(0.05)
    elif args.command == "key":
        for combo in args.combos:
            keys = [qcode(part) for part in combo.split("-") if part]
            qmp.execute("send-key", {"keys": keys, "hold-time": args.hold_ms})
            time.sleep(args.hold_ms / 1000 + 0.03)
    elif args.command == "type":
        for char in args.text:
            qmp.execute("send-key", {"keys": char_keys(char), "hold-time": 20})
            time.sleep(0.04)
    elif args.command == "quit":
        qmp.execute("quit")
    elif args.command == "status":
        print(json.dumps(qmp.execute("query-status")))


if __name__ == "__main__":
    try:
        main()
    except (ConnectionError, FileNotFoundError, socket.timeout) as error:
        print(f"qmp: {error}", file=sys.stderr)
        sys.exit(1)

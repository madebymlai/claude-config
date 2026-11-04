#!/usr/bin/env python3
"""Claude Code JSON stdin → one Catppuccin Mocha Powerline row."""

import json
import os
from pathlib import Path
import re
import subprocess
import sys
import unicodedata


BASE = (30, 30, 46)
SURFACE = (49, 50, 68)
OVERLAY = (69, 71, 90)
TEXT = (205, 214, 244)
MAUVE = (203, 166, 247)
GREEN = (166, 227, 161)
PEACH = (250, 179, 135)
RED = (243, 139, 168)


def tokens(count):
    if count < 1000:
        return str(count)
    if count < 10000:
        return f"{count / 1000:.1f}k"
    if count < 1000000:
        return f"{count / 1000:.0f}k"
    return f"{count / 1000000:.1f}M"


def style(text, foreground, background):
    fg = ";".join(map(str, foreground))
    bg = ";".join(map(str, background))
    return f"\x1b[38;2;{fg}m\x1b[48;2;{bg}m{text}"


def clean(text):
    # Remove CSI and string controls (OSC/DCS/etc.), then controls and bidi marks.
    text = re.sub(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|[\]PX^_].*?(?:\x07|\x1b\\)|[@-_])", "", text, flags=re.S)
    text = "".join(
        char for char in text
        if char.isspace() or unicodedata.category(char) not in ("Cc", "Cf", "Cs", "Cn")
    )
    return " ".join(text.split())


def cells(text):
    # Conservatively reserve a cell for emoji presentation selectors as well.
    return sum(
        1 if char == "\ufe0f" else
        0 if unicodedata.category(char) in ("Mn", "Me") else
        2 if unicodedata.east_asian_width(char) in ("W", "F") else 1
        for char in text
    )


def clip(text, width):
    result = ""
    for char in text:
        width -= cells(char)
        if width < 0:
            break
        result += char
    return result


def project_label(data):
    directory = (data.get("workspace") or {}).get("current_dir") or data.get("cwd")
    if not directory:
        return ""
    name = Path(directory).name or directory
    try:
        result = subprocess.run(
            ["git", "-C", directory, "symbolic-ref", "--quiet", "--short", "HEAD"],
            capture_output=True, text=True, timeout=.5,
        )
        branch = result.stdout.strip() if result.returncode == 0 else ""
    except (OSError, subprocess.TimeoutExpired):
        branch = ""
    return f"{name}  {branch}" if branch else name


def render(data):
    try:
        width = max(0, int(os.environ.get("COLUMNS", "80")) - 4)
    except ValueError:
        width = 76
    if width == 0:
        return ""
    context = data.get("context_window") or {}
    used = context.get("total_input_tokens") or 0
    output = context.get("total_output_tokens") or 0
    capacity = context.get("context_window_size") or 0
    # Present startup/no usage as an empty bar, like the Pi footer.
    known = used == 0 or (context.get("current_usage") is not None and capacity > 0)
    ratio = used / capacity if known and capacity > 0 else 0
    filled = max(0, min(10, int(ratio * 10)))
    color = TEXT if not known else RED if ratio >= .9 else PEACH if ratio >= .7 else GREEN
    bar = "▰" * filled + "▱" * (10 - filled) if known else "░" * 10
    label = tokens(used) if known else "?"
    model = data.get("model") or {}
    name = clean(model.get("display_name") or model.get("id") or "Claude")
    effort = clean((data.get("effort") or {}).get("level") or "")
    parts = [
        (f"{name} {effort}".strip(), BASE, MAUVE),
        (f"↑{tokens(used)} ↓{tokens(output)}", TEXT, SURFACE),
        (f"{bar} {label}", color, OVERLAY),
    ]
    size = sum(cells(text) + 3 for text, _, _ in parts)
    project = clean(project_label(data))
    if project and size + cells(project) + 3 <= width:
        parts.append((project, TEXT, SURFACE))
    if size > width:
        compact = f"{name} {effort}".strip() + f" ↑{tokens(used)} ↓{tokens(output)} {label}"
        return style(clip(compact, width), TEXT, BASE) + "\x1b[0m"
    line = ""
    for index, (text, fg, bg) in enumerate(parts):
        next_bg = parts[index + 1][2] if index + 1 < len(parts) else BASE
        line += style(f" {text} ", fg, bg) + style("", bg, next_bg)
    return line + "\x1b[0m"


if __name__ == "__main__":
    print(render(json.load(sys.stdin)))

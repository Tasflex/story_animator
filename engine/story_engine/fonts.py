"""Font setup — must run before `manim`/`manimpango` are imported.

We point Pango at a generated fontconfig file that includes the bundled
fonts (assets/fonts) plus the OS font directories, and force the fontconfig
backend. `manimpango.register_font` is deliberately NOT used: on Windows it
makes Pango rebuild its font map for every new string (~2.5s per Text).
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FONT_DIR = ROOT / "assets" / "fonts"
CACHE_DIR = Path(os.environ.get("STORY_ENGINE_CACHE", ROOT / "storage" / "cache"))


def _system_font_dirs() -> list[str]:
    if sys.platform.startswith("win"):
        dirs = [Path(os.environ.get("WINDIR", "C:/Windows")) / "Fonts"]
        local = os.environ.get("LOCALAPPDATA")
        if local:
            dirs.append(Path(local) / "Microsoft" / "Windows" / "Fonts")
    elif sys.platform == "darwin":
        dirs = [Path("/System/Library/Fonts"), Path("/Library/Fonts"), Path.home() / "Library" / "Fonts"]
    else:
        dirs = [Path("/usr/share/fonts"), Path("/usr/local/share/fonts"), Path.home() / ".fonts"]
    return [d.as_posix() for d in dirs if d.exists()]


def configure_fonts() -> None:
    if os.environ.get("STORY_ENGINE_FONTS_READY") == "1":
        return
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    conf = CACHE_DIR / "fonts.conf"
    includes = ""
    if not sys.platform.startswith("win") and Path("/etc/fonts/fonts.conf").exists():
        includes = '  <include ignore_missing="yes">/etc/fonts/fonts.conf</include>\n'
    dirs = "".join(f"  <dir>{d}</dir>\n" for d in [FONT_DIR.as_posix(), *_system_font_dirs()])
    body = (
        '<?xml version="1.0"?>\n<!DOCTYPE fontconfig SYSTEM "fonts.dtd">\n<fontconfig>\n'
        f"{includes}{dirs}  <cachedir>{(CACHE_DIR / 'fontconfig').as_posix()}</cachedir>\n</fontconfig>\n"
    )
    if not conf.exists() or conf.read_text(encoding="utf-8") != body:
        conf.write_text(body, encoding="utf-8")
    os.environ["FONTCONFIG_FILE"] = str(conf)
    os.environ["PANGOCAIRO_BACKEND"] = "fc"
    os.environ["STORY_ENGINE_FONTS_READY"] = "1"

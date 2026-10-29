"""Command line entry point.

    python -m story_engine render  --spec scene.json --out scene.mp4 [--draft]
    python -m story_engine catalog            # JSON vocabulary for the storyboard prompt
    python -m story_engine check              # environment self-test (JSON)
    python -m story_engine validate --spec scene.json   # dry-run a custom scene (JSON result)
"""

from __future__ import annotations

import argparse
import json
import shutil
import sys
import time
from pathlib import Path

import story_engine  # noqa: F401  (configures fonts before manim loads)

TRANSITIONS = ["fade", "fadeblack", "dissolve", "slideleft", "slideright", "slideup", "smoothleft",
               "smoothright", "wipeleft", "wiperight", "circleopen", "circleclose", "zoomin", "radial"]
CAMERAS = ["static", "drift", "push_in", "pull_out", "pan_left", "pan_right"]


def cmd_render(args) -> int:
    spec = json.loads(Path(args.spec).read_text(encoding="utf-8"))
    out = Path(args.out).resolve()
    out.parent.mkdir(parents=True, exist_ok=True)
    work = out.parent / f".{out.stem}_work"
    shutil.rmtree(work, ignore_errors=True)

    w, h = int(spec.get("width", 1920)), int(spec.get("height", 1080))
    if args.draft:
        w, h = w // 2, h // 2
        spec["fps"] = min(int(spec.get("fps", 30)), 15)
    from manim import tempconfig

    from .director import SpecScene
    from .stage import Stage
    from .theme import get_theme

    stage = Stage.for_pixels(w, h)
    cfg = {
        "pixel_width": w,
        "pixel_height": h,
        "frame_rate": int(spec.get("fps", 30)),
        "frame_width": stage.width,
        "frame_height": stage.height,
        "background_color": get_theme(spec.get("theme")).bg,
        "media_dir": str(work),
        "output_file": out.stem,
        "disable_caching": True,
        "verbosity": "ERROR",
        "progress_bar": "none",
        "write_to_movie": True,
        "format": "mp4",
    }
    t0 = time.time()
    with tempconfig(cfg):
        scene = SpecScene(spec)
        scene.render()
        produced = Path(scene.renderer.file_writer.movie_file_path)
    if not produced.exists():
        print(json.dumps({"ok": False, "error": f"manim produced no file at {produced}"}))
        return 2
    shutil.move(str(produced), str(out))
    shutil.rmtree(work, ignore_errors=True)
    print(json.dumps({"ok": True, "out": str(out), "seconds": round(time.time() - t0, 2)}))
    return 0


def catalog() -> dict:
    from .backgrounds import BACKGROUND_NOTES
    from .characters import EXPRESSIONS, LOOKS, POSES
    from .icons import ICONS
    from .custom import CUSTOM_API
    from .director import latex_available
    from .kit import PART_API, catalog_docs
    from .kit.parts import ALIASES
    from .layouts import LAYOUT_DOCS
    from .theme import THEMES

    return {
        "version": story_engine.__version__,
        "layouts": LAYOUT_DOCS,
        "parts": catalog_docs(),
        "part_aliases": ALIASES,
        "part_api": PART_API,
        "icons": sorted(ICONS),
        "backgrounds": BACKGROUND_NOTES,
        "themes": {k: v.to_dict() for k, v in THEMES.items()},
        "cameras": CAMERAS,
        "transitions": TRANSITIONS,
        "poses": POSES,
        "expressions": EXPRESSIONS,
        "looks": LOOKS,
        "custom_api": CUSTOM_API,
        "latex": latex_available(),
    }


def cmd_validate(args) -> int:
    """Statically check a custom scene's code, then execute the whole scene
    without drawing frames, so runtime errors surface in seconds."""
    spec = json.loads(Path(args.spec).read_text(encoding="utf-8"))
    from .custom import check_code

    code_path = spec.get("custom_code")
    if not code_path or not Path(code_path).exists():
        print(json.dumps({"ok": False, "error": "spec has no custom_code file"}))
        return 1
    problems = check_code(Path(code_path).read_text(encoding="utf-8"))
    if problems:
        print(json.dumps({"ok": False, "error": "\n".join(problems)}))
        return 1
    import tempfile
    import traceback

    from manim import tempconfig

    from .director import SpecScene
    from .stage import Stage

    spec["strict"] = True
    stage = Stage.for_pixels(int(spec.get("width", 1920)), int(spec.get("height", 1080)))
    tmp = tempfile.TemporaryDirectory(prefix="se_validate_", ignore_cleanup_errors=True)
    cfg = {"dry_run": True, "pixel_width": 320, "pixel_height": 180 if not stage.portrait else 568,
           "frame_rate": 10, "frame_width": stage.width, "frame_height": stage.height,
           "media_dir": tmp.name, "disable_caching": True,
           "verbosity": "ERROR", "progress_bar": "none"}
    try:
        with tmp, tempconfig(cfg):
            scene = SpecScene(spec)
            scene.render()
    except Exception as exc:
        tb = traceback.format_exc().splitlines()
        relevant = [ln for ln in tb if "<custom-scene>" in ln or "Error" in ln or "error" in ln][-8:]
        print(json.dumps({"ok": False, "error": f"{type(exc).__name__}: {exc}\n" + "\n".join(relevant)}))
        return 1
    print(json.dumps({"ok": True}))
    return 0


def cmd_validate_part(args) -> int:
    """Check an AI-written kit part: sandbox, PART metadata, then a dry-run
    process scene that builds it, scales it and fires every action."""
    import tempfile
    import traceback

    from manim import tempconfig

    from . import kit
    from .custom import check_code
    from .director import SpecScene
    from .stage import Stage
    from .theme import THEMES

    path = Path(args.file)
    src = path.read_text(encoding="utf-8")
    problems = check_code(src)
    if problems:
        print(json.dumps({"ok": False, "error": "\n".join(problems)}))
        return 1
    try:
        build, meta = kit.load_part_file(path)
    except Exception as exc:
        print(json.dumps({"ok": False, "error": f"{type(exc).__name__}: {exc}"}))
        return 1
    if not isinstance(meta, dict) or not meta.get("name") or not meta.get("use"):
        print(json.dumps({"ok": False, "error": "PART must be a dict with at least 'name' and 'use'"}))
        return 1
    name = kit.normalize_kind(args.name or meta["name"])
    kit.library()[name] = (build, meta)  # make it resolvable for this run only
    actions = [a for a in (meta.get("actions") or []) if isinstance(a, str)][:8]
    steps = [{"cue": "", "do": [{a: "x"}]} for a in actions] + [{"cue": "", "do": [{"highlight": "x"}]}]
    spec = {"version": 1, "index": 0, "count": 1, "duration": 2.0 + 0.9 * len(steps), "fps": 10,
            "width": 1920, "height": 1080, "theme": THEMES["blueprint"].to_dict(), "seed": 3, "words": [],
            "strict": True,
            "scene": {"narration": "", "layout": "process", "camera": "static", "background": "plain",
                      "content": {"parts": [{"id": "x", "kind": name, "label": "Test", "col": 0},
                                            {"id": "y", "kind": "tank", "col": 1}],
                                  "links": [{"from": "x", "to": "y", "kind": "pipe", "id": "l"}],
                                  "steps": steps}}}
    spec["content_end"] = spec["duration"]
    stage = Stage.for_pixels(1920, 1080)
    tmp = tempfile.TemporaryDirectory(prefix="se_part_", ignore_cleanup_errors=True)
    cfg = {"dry_run": True, "pixel_width": 320, "pixel_height": 180, "frame_rate": 10,
           "frame_width": stage.width, "frame_height": stage.height, "media_dir": tmp.name,
           "disable_caching": True, "verbosity": "ERROR", "progress_bar": "none"}
    try:
        with tmp, tempconfig(cfg):
            SpecScene(spec).render()
    except Exception as exc:
        tb = traceback.format_exc().splitlines()
        relevant = [ln for ln in tb if "<custom-scene>" in ln or "Error" in ln or "error" in ln][-8:]
        print(json.dumps({"ok": False, "error": f"{type(exc).__name__}: {exc}\n" + "\n".join(relevant)}))
        return 1
    print(json.dumps({"ok": True, "name": name, "actions": actions}))
    return 0


def cmd_check(_args) -> int:
    report = {"ok": True, "python": sys.version.split()[0], "engine": story_engine.__version__}
    try:
        import manim

        report["manim"] = manim.__version__
    except Exception as e:  # pragma: no cover
        report.update(ok=False, manim_error=str(e))
    from .theme import ensure_font

    report["font"] = ensure_font()
    try:
        from .layouts import LAYOUTS

        report["layouts"] = len(LAYOUTS)
    except Exception as e:
        report.update(ok=False, layouts_error=str(e))
    print(json.dumps(report))
    return 0 if report["ok"] else 1


def main(argv=None) -> int:
    p = argparse.ArgumentParser(prog="story_engine")
    sub = p.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("render", help="render one scene spec to mp4")
    r.add_argument("--spec", required=True)
    r.add_argument("--out", required=True)
    r.add_argument("--draft", action="store_true", help="half resolution, 15 fps (fast previews)")
    sub.add_parser("catalog", help="print the storyboard vocabulary as JSON")
    sub.add_parser("check", help="environment self-test")
    v = sub.add_parser("validate", help="dry-run a custom scene spec")
    v.add_argument("--spec", required=True)
    vp = sub.add_parser("validate-part", help="sandbox-check and dry-run an AI-written kit part file")
    vp.add_argument("--file", required=True)
    vp.add_argument("--name", default=None)
    args = p.parse_args(argv)
    if args.cmd == "validate":
        return cmd_validate(args)
    if args.cmd == "validate-part":
        return cmd_validate_part(args)
    if args.cmd == "render":
        return cmd_render(args)
    if args.cmd == "catalog":
        print(json.dumps(catalog(), indent=2))
        return 0
    return cmd_check(args)


if __name__ == "__main__":
    sys.exit(main())

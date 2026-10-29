"""`custom` layout: a one-off diagram written by the LLM for a single scene.

The LLM writes a `build(ctx)` function against the same small API the
built-in layouts use (see CUSTOM_API). Before it ever renders:

1. `check_code` statically rejects anything outside the sandbox: imports
   other than manim/numpy/math, file/process/network access, dunder
   attributes, eval/exec, and driving the scene clock directly;
2. the orchestrator runs `python -m story_engine validate` (a dry run that
   executes every animation without drawing frames) and feeds any error
   back to the LLM for another attempt;
3. if it still fails, the scene renders its `fallback` layout instead.

This is a guard against mistakes, not a security boundary for hostile
input: only run storyboards you trust.
"""

from __future__ import annotations

import ast
import builtins
import math
import random
from pathlib import Path

ALLOWED_IMPORTS = {"manim", "numpy", "math", "random"}
BANNED_NAMES = {
    "open", "exec", "eval", "compile", "__import__", "globals", "locals", "vars", "input", "breakpoint",
    "exit", "quit", "getattr", "setattr", "delattr", "memoryview", "help", "dir", "os", "sys",
    "subprocess", "shutil", "socket", "pathlib", "importlib", "builtins", "type", "object",
}
BANNED_ATTRS = {"play", "wait", "renderer", "render", "camera", "file_writer", "system", "popen"}
SAFE_BUILTINS = {
    k: getattr(builtins, k)
    for k in ["abs", "all", "any", "bool", "dict", "divmod", "enumerate", "filter", "float", "int", "isinstance",
              "len", "list", "map", "max", "min", "pow", "print", "range", "reversed", "round", "set", "sorted",
              "str", "sum", "tuple", "zip", "Exception", "ValueError", "ZeroDivisionError", "True", "False", "None"]
    if hasattr(builtins, k)
}


def check_code(src: str) -> list[str]:
    """Static sandbox check. Returns a list of problems (empty = OK)."""
    try:
        tree = ast.parse(src)
    except SyntaxError as e:
        return [f"SyntaxError: {e.msg} (line {e.lineno})"]
    problems = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for a in node.names:
                if a.name.split(".")[0] not in ALLOWED_IMPORTS:
                    problems.append(f"line {node.lineno}: import of '{a.name}' is not allowed")
        elif isinstance(node, ast.ImportFrom):
            if (node.module or "").split(".")[0] not in ALLOWED_IMPORTS:
                problems.append(f"line {node.lineno}: import from '{node.module}' is not allowed")
        elif isinstance(node, ast.Name) and node.id in BANNED_NAMES:
            problems.append(f"line {node.lineno}: '{node.id}' is not allowed")
        elif isinstance(node, ast.Attribute):
            if node.attr.startswith("__"):
                problems.append(f"line {node.lineno}: dunder attribute '{node.attr}' is not allowed")
            elif node.attr in BANNED_ATTRS:
                problems.append(f"line {node.lineno}: '.{node.attr}' is not allowed — schedule animations "
                                f"with ctx.at(t, lambda: [...], run_time); the camera is handled for you")
        elif isinstance(node, (ast.Global, ast.Nonlocal)):
            problems.append(f"line {node.lineno}: global/nonlocal is not allowed")
    if not any(isinstance(n, ast.FunctionDef) and n.name == "build" for n in tree.body):
        problems.append("the file must define a top-level function `def build(ctx):`")
    return problems


def load_namespace(src: str, extra: dict | None = None, filename: str = "<custom-scene>") -> dict:
    """Execute checked source in a restricted namespace and return it. Used for
    custom scenes and for AI-written kit parts (which get `extra` names)."""
    problems = check_code(src)
    if problems:
        raise ValueError("custom code rejected:\n" + "\n".join(problems))
    import manim
    import numpy as np

    ns = {name: getattr(manim, name) for name in dir(manim) if not name.startswith("_")}
    ns.update({"np": np, "numpy": np, "math": math, "random": random.Random(7),
               "__builtins__": {**SAFE_BUILTINS, "__import__": _guarded_import}})
    ns.update(extra or {})
    exec(compile(src, filename, "exec"), ns)  # noqa: S102 (checked above)
    return ns


def load_build(src: str):
    """Execute checked source in a restricted namespace; return its build()."""
    return load_namespace(src)["build"]


def _guarded_import(name, globals=None, locals=None, fromlist=(), level=0):
    if name.split(".")[0] not in ALLOWED_IMPORTS:
        raise ImportError(f"import of {name} is not allowed")
    return __import__(name, globals, locals, fromlist, level)


def layout_custom(ctx) -> None:
    path = ctx.spec.get("custom_code")
    if not path or not Path(path).exists():
        raise FileNotFoundError("custom scene has no generated code")
    build = load_build(Path(path).read_text(encoding="utf-8"))
    build(ctx)
    if not ctx.beats:
        raise ValueError("custom build() scheduled no animations (use ctx.at)")


CUSTOM_DOC = {
    "use": "a one-off diagram no other layout can show (max 2 per video). Describe it precisely; a programmer "
           "animates it. Always include a fallback.",
    "content": {"brief": "concrete shot list: objects, positions, colours, what moves when, which narration word "
                         "triggers each step",
                "fallback": {"layout": "another layout name", "content": {"...": "that layout's content"}}},
}

CUSTOM_API = r'''
Write ONE Python file that defines `def build(ctx):`. It builds a single scene of a
narrated animated explainer using Manim Community (Cairo). The scene clock, camera,
background and scene title are handled for you. You only add objects and schedule
animations.

Available names: everything from `manim` (Circle, Arrow, VGroup, MathTex, Axes, Create,
Transform, ...), `np` (numpy), `math`, `random`. No other imports, no file/OS access.

THE ctx API
  ctx.theme.bg / .bg2 / .ink / .muted / .accent / .accent2 / .accent3   hex colours (use ONLY these)
  ctx.stage.width, ctx.stage.height        frame size in units (landscape 14.2x8, portrait 8x14.2)
  ctx.stage.content_center                 np.array — centre of the free area (below the title)
  ctx.stage.content_w, ctx.stage.content_h usable width / height. Keep everything inside this box.
  ctx.stage.content_top                    y above which nothing may be drawn when there is a heading
  ctx.stage.caption_top                    y below which NOTHING may be drawn (captions live there)
  ctx.stage.portrait                       bool
  ctx.duration                             seconds this scene lasts
  ctx.content_end                          seconds by which all motion should be finished
  ctx.narr_start, ctx.narr_end             when narration starts / ends in this scene
  ctx.cue("word")                          seconds when that narration word is spoken (None if absent)
  ctx.schedule(["w1", "w2", ...])          increasing reveal times for a sequence, cue-aware
  ctx.at(t, lambda: [anim, ...], run_time) schedule animations to START at time t (seconds).
                                           The lambda runs at that moment, so it sees current state.
  ctx.add(mobject, ...)                    show immediately (no animation)
  ctx.text("label", size=34, color=None, weight="NORMAL"|"BOLD")   themed text (serif or sans per theme)
  ctx.tex(r"\frac{a}{b}", size=48, color=None)                      maths (LaTeX); falls back to text
  ctx.icon("name", size=1.6)                                        flat icon (sun, gear, heart, ...)
  ctx.float(mobject)                                                gentle idle bobbing
  ctx.accent(i)                                                     i-th accent colour

RULES
  1. Never call self.play / self.wait / scene methods. Only ctx.at(...) and ctx.add(...).
  2. Time every beat to the narration: t = ctx.cue("word") or a sensible fallback time.
     All beats must start before ctx.content_end - 0.5. run_time 0.5-2.0s.
  3. Stay inside the content box: centre on ctx.stage.content_center, scale groups to fit
     ctx.stage.content_w x ctx.stage.content_h, never above ctx.stage.content_top (labels
     included) and never below ctx.stage.caption_top. Build the whole composition as one
     VGroup, then `group.scale_to_fit_height(...)` if needed, before scheduling beats.
  4. Style: dark background is provided. Thin elegant strokes (2-5), ink for structure,
     accent for the hero element, accent2 for contrast/warnings, accent3 sparingly.
     One clear focus at a time; dim context with .set_opacity(0.3-0.5).
  5. Labels <= 3 words. Do not write the narration on screen.
  6. Prefer transformations that show causality: Transform/ReplacementTransform one
     shape into the next, MoveAlongPath, ValueTracker + always_redraw, GrowArrow, Indicate.
  7. Line/Arrow take exactly two points. .get_center() returns a numpy array, not a mobject.
  8. 6-18 beats. Deterministic (no randomness without random.seed).
  9. Return ONLY a ```python fenced block.

EXAMPLE
```python
def build(ctx):
    th, st = ctx.theme, ctx.stage
    c = st.content_center
    rotors = VGroup(*[Circle(radius=0.45, stroke_color=th.accent, stroke_width=3)
                      .move_to(c + np.array([x, y, 0])) for x in (-1.6, 1.6) for y in (-1.1, 1.1)])
    body = Circle(radius=0.6, stroke_color=th.ink, stroke_width=3).move_to(c)
    arms = VGroup(*[Line(body.get_center(), r.get_center(), stroke_color=th.muted, stroke_width=3)
                    for r in rotors])
    t0 = ctx.cue("drone") or 0.4
    ctx.at(t0, lambda: [Create(arms), GrowFromCenter(body)], 0.9)
    ctx.at(t0 + 0.6, lambda: LaggedStart(*[GrowFromCenter(r) for r in rotors], lag_ratio=0.15), 0.9)
    spins = VGroup(*[Arc(radius=0.6, angle=TAU * 0.6, stroke_color=th.accent2 if i % 2 else th.accent3,
                         stroke_width=4).move_arc_center_to(r.get_center()) for i, r in enumerate(rotors)])
    t1 = ctx.cue("spin") or t0 + 2
    ctx.at(t1, lambda: [Create(spins)], 1.0)
    for i, s in enumerate(spins):
        s.add_updater(lambda m, dt, i=i: m.rotate((3 if i % 2 else -3) * dt))
    label = ctx.text("balanced", size=32, color=th.accent).next_to(rotors, DOWN, buff=0.3)
    t2 = ctx.cue("cancel") or t1 + 2
    ctx.at(t2, lambda: [FadeIn(label, shift=UP * 0.2), Indicate(body, color=th.accent)], 1.0)
```
'''

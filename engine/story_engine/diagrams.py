"""Diagram layouts — the 3Blue1Brown-style vocabulary.

These explain *how* something works rather than illustrate it: equations
that morph step by step, plotted functions, vectors, populations of dots and
signal-carrying networks. Like every layout they read `ctx.content`, schedule
beats on narration cues, and tolerate missing or odd input.
"""

from __future__ import annotations

import ast
import math

import numpy as np
from manim import (
    DOWN,
    LEFT,
    RIGHT,
    UP,
    UR,
    AnimationGroup,
    Arrow,
    Axes,
    Circle,
    Circumscribe,
    Create,
    DashedLine,
    Dot,
    FadeIn,
    FadeOut,
    Flash,
    GrowArrow,
    GrowFromCenter,
    LaggedStart,
    Line,
    NumberPlane,
    Polygon,
    ShowPassingFlash,
    SurroundingRectangle,
    TransformMatchingShapes,
    ValueTracker,
    VGroup,
    VMobject,
    Write,
    always_redraw,
    rate_functions,
)

from .director import Ctx
from .stage import fit

EASE = rate_functions.ease_in_out_sine


def _items(ctx: Ctx, key: str, hi: int) -> list[dict]:
    raw = ctx.content.get(key) or []
    out = [it if isinstance(it, dict) else {"label": str(it)} for it in (raw if isinstance(raw, list) else [])]
    return out[:hi]


def _cue(it: dict, *keys) -> str:
    for k in ("cue", *keys):
        if it.get(k):
            return str(it[k])
    return ""


def _color(ctx: Ctx, key, default: str) -> str:
    th = ctx.theme
    named = {"accent": th.accent, "accent2": th.accent2, "accent3": th.accent3, "ink": th.ink, "muted": th.muted}
    k = str(key or "").strip()
    if k in named:
        return named[k]
    if k.startswith("#") and len(k) == 7:
        return k
    return default


def _num(v, default=0.0) -> float:
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def _nice_step(span: float, target_ticks: int = 8) -> float:
    raw = max(span / target_ticks, 1e-9)
    mag = 10 ** math.floor(math.log10(raw))
    for m in (1, 2, 5, 10):
        if raw <= m * mag:
            return m * mag
    return 10 * mag


# ---------------------------------------------------------------------------
# safe math expressions for `graph`
# ---------------------------------------------------------------------------

_FUNCS = {
    "sin": math.sin, "cos": math.cos, "tan": math.tan, "asin": math.asin, "acos": math.acos,
    "atan": math.atan, "sinh": math.sinh, "cosh": math.cosh, "tanh": math.tanh, "exp": math.exp,
    "log": math.log, "ln": math.log, "log10": math.log10, "log2": math.log2, "sqrt": math.sqrt,
    "abs": abs, "floor": math.floor, "ceil": math.ceil, "min": min, "max": max,
    "pi": math.pi, "e": math.e,
}
_NODES = (ast.Expression, ast.BinOp, ast.UnaryOp, ast.Call, ast.Name, ast.Constant, ast.Load,
          ast.operator, ast.unaryop)


def compile_expr(expr: str):
    """Compile a function of x like 'x**2 - 3*sin(x)' using a strict whitelist."""
    src = str(expr).replace("^", "**").strip()
    tree = ast.parse(src, mode="eval")
    for node in ast.walk(tree):
        if not isinstance(node, _NODES):
            raise ValueError(f"unsupported syntax in {expr!r}: {type(node).__name__}")
        if isinstance(node, ast.Name) and node.id != "x" and node.id not in _FUNCS:
            raise ValueError(f"unknown name {node.id!r} in {expr!r}")
        if isinstance(node, ast.Call) and not isinstance(node.func, ast.Name):
            raise ValueError(f"unsupported call in {expr!r}")
    code = compile(tree, "<expr>", "eval")

    def f(x: float) -> float:
        return float(eval(code, {"__builtins__": {}}, {**_FUNCS, "x": x}))  # noqa: S307 (whitelisted AST)

    return f


def _samples(f, a: float, b: float, n: int = 240):
    xs = np.linspace(a, b, n)
    out = []
    for x in xs:
        try:
            y = f(float(x))
            out.append((float(x), y if math.isfinite(y) else None))
        except (ValueError, ZeroDivisionError, OverflowError):
            out.append((float(x), None))
    return out


# ---------------------------------------------------------------------------
# equation — derivations that morph step by step
# ---------------------------------------------------------------------------

def layout_equation(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    steps = _items(ctx, "steps", 6) or [{"tex": str(c.get("tex") or "E = mc^2")}]
    size = 76 if not st.portrait else 60
    mobs = []
    for s in steps:
        m = ctx.tex(str(s.get("tex") or s.get("label") or ""), size=size)
        fit(m, st.content_w * 0.92, st.content_h * (0.3 if c.get("stack") else 0.45))
        mobs.append(m)
    notes = [ctx.text(s["note"], size=30, color=th.muted, max_chars=44, max_width=st.content_w) if s.get("note")
             else None for s in steps]
    times = ctx.schedule([_cue(s, "note") for s in steps], start=max(ctx.narr_start, 0.4), min_gap=0.9)
    cc = st.content_center

    if c.get("stack"):
        col = VGroup(*mobs).arrange(DOWN, buff=0.45)
        fit(col, st.content_w * 0.92, st.content_h * 0.92)
        col.move_to(cc)
        for i, (m, t) in enumerate(zip(mobs, times)):
            prev = mobs[:i]
            ctx.at(t, lambda m=m, prev=prev: [Write(m), *(p.animate.set_color(th.muted) for p in prev)], 1.2)
    else:
        for m in mobs:
            m.move_to(cc + UP * 0.35)
        ctx.at(times[0], lambda: Write(mobs[0]), 1.3)
        for i in range(1, len(mobs)):
            ctx.at(times[i], lambda a=mobs[i - 1], b=mobs[i]: TransformMatchingShapes(a, b, path_arc=0.4), 1.2)
    last = mobs[-1]
    # Notes sit below the tallest step so no step ever collides with them, and
    # each step replaces the previous note (or clears it if it has none).
    note_y = (min(m.get_bottom()[1] for m in mobs) - 0.5) if not c.get("stack") else None
    shown = [None]
    for n, t in zip(notes, times):
        if n is not None:
            if note_y is not None:
                n.move_to([0, note_y, 0])
            else:
                n.next_to(VGroup(*mobs), DOWN, buff=0.4)

        def swap(n=n):
            anims = [FadeIn(n, shift=UP * 0.15)] if n is not None else []
            if shown[0] is not None:
                anims.append(FadeOut(shown[0]))
            shown[0] = n
            return anims
        ctx.at(t + 0.35, swap, 0.6)
    te = ctx.cue(c.get("highlight_cue"), None, after=times[-1]) or min(times[-1] + 1.6, ctx.content_end - 1.1)
    ctx.at(te, lambda: Circumscribe(last, color=th.accent, buff=0.18, time_width=0.6), 1.2)


# ---------------------------------------------------------------------------
# graph — functions on axes
# ---------------------------------------------------------------------------

def layout_graph(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    fns = []
    for i, fd in enumerate(_items(ctx, "functions", 3)):
        try:
            fns.append((fd, compile_expr(fd.get("expr", "x"))))
        except (ValueError, SyntaxError) as exc:
            print(f"[story_engine] graph: skipping function: {exc}")
    if not fns:
        fns = [({"expr": "x**2"}, compile_expr("x**2"))]
    xr = c.get("x_range") if isinstance(c.get("x_range"), list) and len(c["x_range"]) >= 2 else [-4, 4]
    a, b = _num(xr[0], -4), _num(xr[1], 4)
    if b <= a:
        a, b = -4.0, 4.0
    curves_data = [(_samples(f, a, b), fd, f) for fd, f in fns]
    if isinstance(c.get("y_range"), list) and len(c["y_range"]) >= 2:
        ylo, yhi = _num(c["y_range"][0], -1), _num(c["y_range"][1], 1)
    else:
        ys = [y for data, _, _ in curves_data for _, y in data if y is not None]
        ylo, yhi = (min(ys), max(ys)) if ys else (-1.0, 1.0)
        pad = (yhi - ylo) * 0.12 or 1.0
        ylo, yhi = min(ylo - pad, 0.0 if ylo >= 0 else ylo - pad), yhi + pad
    if yhi <= ylo:
        yhi = ylo + 1
    axes = Axes(
        x_range=[a, b, _nice_step(b - a)], y_range=[ylo, yhi, _nice_step(yhi - ylo)],
        x_length=st.content_w * (0.82 if not st.portrait else 0.95), y_length=st.content_h * 0.86,
        tips=True, axis_config={"color": th.muted, "stroke_width": 2.5, "tick_size": 0.06},
    ).move_to(st.content_center)
    labels = VGroup()
    if c.get("x_label"):
        labels.add(ctx.tex(str(c["x_label"]), size=34, color=th.muted).next_to(axes.x_axis.get_end(), DOWN + LEFT, buff=0.15))
    if c.get("y_label"):
        labels.add(ctx.tex(str(c["y_label"]), size=34, color=th.muted).next_to(axes.y_axis.get_end(), LEFT, buff=0.2))

    def clipped_curve(data, color):
        segs, cur = VGroup(), []
        for x, y in data:
            if y is None or not (ylo - 1e-9 <= y <= yhi + 1e-9):
                if len(cur) > 1:
                    segs.add(VMobject(stroke_color=color, stroke_width=5).set_points_smoothly(cur))
                cur = []
            else:
                cur.append(axes.c2p(x, y))
        if len(cur) > 1:
            segs.add(VMobject(stroke_color=color, stroke_width=5).set_points_smoothly(cur))
        return segs

    ctx.at(0.2, lambda: [Create(axes), FadeIn(labels)], 1.1)
    curve_mobs = []
    cues = ctx.schedule([_cue(fd, "label") for _, fd, _ in curves_data], start=max(ctx.narr_start, 1.0), min_gap=1.0)
    for i, ((data, fd, f), t) in enumerate(zip(curves_data, cues)):
        col = _color(ctx, fd.get("color"), ctx.accent(i))
        curve = clipped_curve(data, col)
        curve_mobs.append(curve)
        anims = [Create(curve, rate_func=EASE)]
        if fd.get("label") and len(curve):
            end = curve[-1].get_end()
            # Alternate above/below the curve end so labels of curves that end
            # near each other don't collide.
            side = UP if i % 2 == 0 else DOWN
            lab = ctx.tex(str(fd["label"]), size=36, color=col).next_to(end, side + LEFT * 0.4, buff=0.2)
            fit(lab, 3.5, 1.0)
            if lab.get_right()[0] > st.width / 2 - 0.2:
                lab.shift(LEFT * (lab.get_right()[0] - st.width / 2 + 0.2))
            anims.append(FadeIn(lab, shift=UP * 0.1))
        ctx.at(t, lambda anims=anims: anims, 1.6)

    area = c.get("area") if isinstance(c.get("area"), dict) else None
    if area:
        k = min(int(_num(area.get("function"), 0)), len(curves_data) - 1)
        f = curves_data[k][2]
        x0, x1 = _num(area.get("from"), a), _num(area.get("to"), b)
        pts = [axes.c2p(x0, 0)]
        for x, y in _samples(f, x0, x1, 120):
            if y is not None:
                pts.append(axes.c2p(x, max(min(y, yhi), ylo)))
        pts.append(axes.c2p(x1, 0))
        if len(pts) > 3:
            region = Polygon(*pts, stroke_width=0, fill_color=_color(ctx, area.get("color"), th.accent), fill_opacity=0.28)
            ta = ctx.cue(area.get("cue"), None) or cues[-1] + 1.4
            ctx.at(ta, lambda: FadeIn(region), 0.9)

    for j, p in enumerate(_items(ctx, "points", 4)):
        k = min(int(_num(p.get("function"), 0)), len(curves_data) - 1)
        x = _num(p.get("x"), 0)
        try:
            y = _num(p["y"]) if "y" in p else curves_data[k][2](x)
        except (ValueError, ZeroDivisionError, OverflowError):
            continue
        if not (ylo <= y <= yhi and a <= x <= b):
            continue
        dot = Dot(axes.c2p(x, y), radius=0.1, color=_color(ctx, p.get("color"), th.accent2))
        lab = ctx.tex(str(p["label"]), size=30) .next_to(dot, UR, buff=0.12) if p.get("label") else VGroup()
        tp = ctx.cue(_cue(p, "label"), None) or cues[-1] + 1.0 + 0.8 * j
        ctx.at(tp, lambda dot=dot, lab=lab: [GrowFromCenter(dot), FadeIn(lab), Flash(dot, color=dot.get_color(),
                                                                                       flash_radius=0.3)], 0.8)

    tr = c.get("trace") if isinstance(c.get("trace"), dict) else None
    if tr:
        k = min(int(_num(tr.get("function"), 0)), len(curves_data) - 1)
        f = curves_data[k][2]
        x0, x1 = _num(tr.get("from"), a), _num(tr.get("to"), b)
        tracker = ValueTracker(x0)

        def y_at(x):
            try:
                return max(min(f(x), yhi), ylo)
            except (ValueError, ZeroDivisionError, OverflowError):
                return 0.0

        dot = always_redraw(lambda: Dot(axes.c2p(tracker.get_value(), y_at(tracker.get_value())),
                                        radius=0.11, color=th.accent3))
        vline = always_redraw(lambda: DashedLine(axes.c2p(tracker.get_value(), 0),
                                                 axes.c2p(tracker.get_value(), y_at(tracker.get_value())),
                                                 stroke_color=th.accent3, stroke_width=2, dash_length=0.08))
        tt = ctx.cue(tr.get("cue"), None) or cues[-1] + 1.2
        run = min(3.0, max(ctx.content_end - tt - 0.6, 0.8))
        ctx.at(tt, lambda: [FadeIn(dot), FadeIn(vline)], 0.4)
        ctx.at(tt + 0.45, lambda: tracker.animate(rate_func=EASE).set_value(x1), run)


# ---------------------------------------------------------------------------
# vectors — arrows on a plane, tip to tail, with a resultant
# ---------------------------------------------------------------------------

def layout_vectors(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    raw = _items(ctx, "vectors", 5) or [{"vec": [3, 0]}, {"vec": [0, 4]}]
    chain = c.get("chain", True)
    segs = []
    tip = np.array([0.0, 0.0])
    for v in raw:
        if "from" in v and "to" in v:
            p0 = np.array([_num(v["from"][0]), _num(v["from"][1])])
            p1 = np.array([_num(v["to"][0]), _num(v["to"][1])])
        else:
            vec = v.get("vec") or [1, 0]
            d = np.array([_num(vec[0]), _num(vec[1] if len(vec) > 1 else 0)])
            p0 = tip.copy() if chain else np.array([0.0, 0.0])
            p1 = p0 + d
        segs.append((p0, p1, v))
        tip = p1
    pts = [np.zeros(2)] + [p for s in segs for p in s[:2]]
    xs, ys = [p[0] for p in pts], [p[1] for p in pts]
    x0, x1 = math.floor(min(xs)) - 1, math.ceil(max(xs)) + 1
    y0, y1 = math.floor(min(ys)) - 1, math.ceil(max(ys)) + 1
    unit = min(st.content_w * 0.9 / (x1 - x0), st.content_h * 0.92 / (y1 - y0))
    plane = NumberPlane(
        x_range=[x0, x1, 1], y_range=[y0, y1, 1], x_length=(x1 - x0) * unit, y_length=(y1 - y0) * unit,
        background_line_style={"stroke_color": th.accent, "stroke_width": 1, "stroke_opacity": 0.16},
        axis_config={"stroke_color": th.muted, "stroke_width": 2, "stroke_opacity": 0.7},
    ).move_to(st.content_center)

    def label_for(p0, p1, text, color):
        if not text:
            return VGroup()
        a, b = plane.c2p(*p0), plane.c2p(*p1)
        d = b - a
        n = np.array([-d[1], d[0], 0.0])
        n = n / (np.linalg.norm(n) or 1)
        lab = ctx.tex(str(text), size=36, color=color)
        # Push the label off the arrow by its own half-extent along the normal.
        reach = 0.22 + abs(n[0]) * lab.width / 2 + abs(n[1]) * lab.height / 2
        return lab.move_to((a + b) / 2 + n * reach)

    ctx.at(0.2, lambda: Create(plane, lag_ratio=0.02), 1.2)
    times = ctx.schedule([_cue(v, "label") for _, _, v in segs], start=max(ctx.narr_start, 1.0), min_gap=0.8)
    for i, ((p0, p1, v), t) in enumerate(zip(segs, times)):
        col = _color(ctx, v.get("color"), [th.accent3, th.accent, th.ink][i % 3])
        arr = Arrow(plane.c2p(*p0), plane.c2p(*p1), buff=0, stroke_width=6, color=col,
                    max_tip_length_to_length_ratio=0.18, max_stroke_width_to_length_ratio=8)
        lab = label_for(p0, p1, v.get("label"), col)
        ctx.at(t, lambda arr=arr, lab=lab: [GrowArrow(arr), FadeIn(lab)], 0.9)
    s = c.get("sum") if isinstance(c.get("sum"), dict) else ({} if c.get("sum") else None)
    if s is not None and segs:
        start, end = segs[0][0], segs[-1][1]
        col = _color(ctx, s.get("color"), th.accent2)
        ts = ctx.cue(s.get("cue"), None, after=times[-1]) or min(times[-1] + 1.2, ctx.content_end - 1.6)
        if np.linalg.norm(end - start) < 1e-6:
            # Balanced: the resultant is zero — no arrow, just mark the point.
            pt = plane.c2p(*start)
            lab = ctx.tex(str(s.get("label") or "0"), size=36, color=col).next_to(pt, RIGHT, buff=0.35)
            ctx.at(ts, lambda: [Flash(pt, color=col, flash_radius=0.4), FadeIn(lab, shift=LEFT * 0.2)], 0.9)
        else:
            res = Arrow(plane.c2p(*start), plane.c2p(*end), buff=0, stroke_width=7, color=col,
                        max_tip_length_to_length_ratio=0.14)
            lab = label_for(end, start, s.get("label"), col)
            ctx.at(ts, lambda: [GrowArrow(res), FadeIn(lab)], 1.0)
            ctx.at(ts + 1.1, lambda: Flash(res.get_end(), color=col, flash_radius=0.35), 0.7)


# ---------------------------------------------------------------------------
# dot_grid — a population, with groups lighting up
# ---------------------------------------------------------------------------

def layout_dot_grid(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    total = int(min(max(_num(c.get("total"), 100), 4), 1000))
    groups = _items(ctx, "groups", 4)
    legend_side = not st.portrait and bool(groups)
    gw = st.content_w * (0.62 if legend_side else 0.95)
    gh = st.content_h * (0.95 if legend_side else 0.72)
    cols = max(1, round(math.sqrt(total * gw / gh)))
    rows = math.ceil(total / cols)
    step = min(gw / cols, gh / rows)
    r = max(step * 0.3, 0.018)
    dots = VGroup(*[Dot(radius=r, color=th.muted, fill_opacity=0.55) for _ in range(total)])
    dots.arrange_in_grid(rows=rows, cols=cols, buff=step - 2 * r)

    order = list(range(total))
    if c.get("shuffle", True):
        ctx.rng.shuffle(order)
    picked, k = [], 0
    for g in groups:
        n = int(min(max(_num(g.get("count"), 1), 0), total - k))
        picked.append([dots[order[j]] for j in range(k, k + n)])
        k += n

    legend = VGroup()
    rows_l = []
    for i, g in enumerate(groups):
        col = _color(ctx, g.get("color"), [th.accent2, th.accent, th.accent3, th.ink][i % 4])
        sw = Dot(radius=0.12, color=col)
        lab = ctx.text(str(g.get("label") or g.get("count", "")), size=30, color=col, max_chars=22,
                       max_width=st.content_w * 0.34 if legend_side else st.content_w * 0.8, align="left")
        row = VGroup(sw, lab).arrange(RIGHT, buff=0.25)
        rows_l.append(row)
        legend.add(row)
    legend.arrange(DOWN, aligned_edge=LEFT, buff=0.35)
    if legend_side:
        whole = VGroup(dots, legend).arrange(RIGHT, buff=0.8)
    else:
        whole = VGroup(dots, legend).arrange(DOWN, buff=0.5) if len(legend) else VGroup(dots)
    fit(whole, st.content_w, st.content_h)
    whole.move_to(st.content_center)

    ctx.at(0.25, lambda: FadeIn(dots, lag_ratio=min(0.6 / total, 0.02), scale=0.6), 1.2)
    times = ctx.schedule([_cue(g, "label") for g in groups], start=max(ctx.narr_start + 0.6, 1.4), min_gap=1.0)
    for i, (g, ds, row, t) in enumerate(zip(groups, picked, rows_l, times)):
        col = row[0].get_color()
        lag = min(0.08, 1.2 / max(len(ds), 1))

        def light(ds=ds, row=row, col=col, lag=lag):
            anims = [d.animate.set_fill(col, opacity=1).scale(1.5) for d in ds]
            return [LaggedStart(*anims, lag_ratio=lag) if anims else FadeIn(row), FadeIn(row, shift=LEFT * 0.2)]
        ctx.at(t, light, 1.0)
    tf = ctx.cue(c.get("focus_cue"), None)
    if tf is not None:
        chosen = {id(d) for ds in picked for d in ds}
        rest = [d for d in dots if id(d) not in chosen]
        ctx.at(tf, lambda: [d.animate.set_opacity(0.08) for d in rest], 0.8)
    if c.get("result"):
        res = ctx.tex(str(c["result"]), size=60, color=th.accent) if c.get("result_tex") else \
            ctx.text(str(c["result"]), size=46, weight="BOLD", color=th.accent, max_width=st.content_w * 0.9)
        box = SurroundingRectangle(res, color=th.accent, buff=0.25, corner_radius=0.12,
                                   fill_color=th.bg, fill_opacity=0.88, stroke_width=3)
        badge = VGroup(box, res).move_to(dots.get_center())
        tr = ctx.cue(c.get("result_cue"), None) or min(times[-1] + 1.6 if times else 2.0, ctx.content_end - 1.4)
        ctx.at(tr, lambda: [FadeIn(box), Write(res)], 1.0)


# ---------------------------------------------------------------------------
# network — layered nodes with signals flowing through
# ---------------------------------------------------------------------------

def layout_network(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    layers_spec = c.get("layers")
    node_labels = c.get("labels") if isinstance(c.get("labels"), list) else []
    nodes: list[list] = []
    edges_by_stage: list[list] = []
    cc = st.content_center
    W, H = st.content_w * 0.86, st.content_h * 0.8
    if isinstance(layers_spec, list) and layers_spec:
        sizes = [int(min(max(_num(n, 1), 1), 8)) for n in layers_spec[:6]]
        n_layers = len(sizes)
        r = min(0.28, H / (max(sizes) * 2.6))
        for li, n in enumerate(sizes):
            x = cc[0] - W / 2 + W * (li / max(n_layers - 1, 1))
            col = []
            for k in range(n):
                y = cc[1] + H / 2 - H * ((k + 0.5) / n)
                col.append(Circle(radius=r, stroke_color=th.accent, stroke_width=3, fill_color=th.bg2,
                                  fill_opacity=1).move_to([x, y, 0]))
            nodes.append(col)
        for li in range(n_layers - 1):
            edges_by_stage.append([Line(a.get_center(), b.get_center(), buff=r, stroke_color=th.muted,
                                        stroke_width=1.6, stroke_opacity=0.45)
                                   for a in nodes[li] for b in nodes[li + 1]])
    else:
        names = [str(n) for n in (c.get("nodes") or ["A", "B", "C", "D"])][:8]
        R = min(W, H) * 0.42
        ring = []
        for k, name in enumerate(names):
            ang = math.pi / 2 - k * 2 * math.pi / len(names)
            p = cc + R * np.array([math.cos(ang), math.sin(ang), 0])
            circ = Circle(radius=0.45, stroke_color=th.accent, stroke_width=3, fill_color=th.bg2,
                          fill_opacity=1).move_to(p)
            ring.append(circ)
            node_labels.append(name)
        nodes = [[n] for n in ring]
        pairs = c.get("edges") or [[k, (k + 1) % len(ring)] for k in range(len(ring))]
        edges_by_stage.append([Line(ring[int(i) % len(ring)].get_center(), ring[int(j) % len(ring)].get_center(),
                                    buff=0.45, stroke_color=th.muted, stroke_width=2, stroke_opacity=0.6)
                               for i, j in pairs])
    flat_nodes = [n for col in nodes for n in col]
    texts = VGroup()
    for n, lab in zip(flat_nodes, node_labels):
        if lab:
            t = ctx.text(str(lab), size=24, max_width=2.2)
            t.next_to(n, DOWN, buff=0.12) if t.width > n.width * 0.9 else t.move_to(n)
            texts.add(t)
    layer_names = c.get("layer_labels") if isinstance(c.get("layer_labels"), list) else []
    for col, name in zip(nodes, layer_names):
        if name:
            texts.add(ctx.text(str(name), size=26, color=th.muted, max_width=3).next_to(VGroup(*col), DOWN, buff=0.3))
    all_edges = [e for stage_edges in edges_by_stage for e in stage_edges]

    ctx.at(0.25, lambda: LaggedStart(*[LaggedStart(*[GrowFromCenter(n) for n in col], lag_ratio=0.1)
                                       for col in nodes], lag_ratio=0.25), 1.2)
    ctx.at(1.3, lambda: [LaggedStart(*[Create(e) for e in all_edges], lag_ratio=0.01), FadeIn(texts)], 1.0)
    pulses = _items(ctx, "pulses", 3) or [{"cue": c.get("pulse_cue", "")}]
    times = ctx.schedule([_cue(p) for p in pulses], start=max(ctx.narr_start + 1.2, 2.4), min_gap=1.8)

    def pulse():
        waves = []
        for stage_edges in edges_by_stage:
            waves.append(AnimationGroup(*[ShowPassingFlash(e.copy().set_stroke(th.accent, width=4, opacity=1),
                                                           time_width=0.7) for e in stage_edges]))
        lit = [n.animate.set_fill(th.accent, opacity=0.9) for n in nodes[-1]]
        return [LaggedStart(*waves, lag_ratio=0.75), LaggedStart(*lit, lag_ratio=0.1)]
    for t in times:
        ctx.at(t, pulse, 1.6)
    path = c.get("path")
    if isinstance(path, list) and len(path) == len(nodes) and len(nodes) > 1:
        tp = ctx.cue(c.get("path_cue"), None) or min(times[-1] + 1.8, ctx.content_end - 1.4)
        chosen = [nodes[li][int(_num(k)) % len(nodes[li])] for li, k in enumerate(path)]
        lines = [Line(a.get_center(), b.get_center(), buff=a.width / 2, stroke_color=th.accent2, stroke_width=5)
                 for a, b in zip(chosen, chosen[1:])]
        ctx.at(tp, lambda: [*(e.animate.set_stroke(opacity=0.12) for e in all_edges),
                            LaggedStart(*[Create(ln) for ln in lines], lag_ratio=0.6),
                            *(n.animate.set_stroke(th.accent2, width=5) for n in chosen)], 1.4)


DIAGRAM_LAYOUTS = {
    "equation": layout_equation,
    "graph": layout_graph,
    "vectors": layout_vectors,
    "dot_grid": layout_dot_grid,
    "network": layout_network,
}

DIAGRAM_DOCS = {
    "equation": {
        "use": "a formula or derivation that transforms step by step (LaTeX)",
        "content": {"steps": [{"tex": "LaTeX, e.g. 8 \\times 6 = 48", "note": "optional <=8 words", "cue": "word"}],
                    "stack": "optional true = keep earlier lines above (derivation)",
                    "highlight_cue": "word to circle the final line"},
    },
    "graph": {
        "use": "plotted functions on axes, with points, shaded area or a tracing dot",
        "content": {"x_range": [-3, 3], "y_range": "optional [min, max]",
                    "functions": [{"expr": "python math of x, e.g. x**2 or sin(x)", "label": "LaTeX label",
                                   "cue": "word", "color": "accent|accent2|accent3"}],
                    "points": [{"x": 1, "label": "LaTeX", "cue": "word"}],
                    "area": {"function": 0, "from": 0, "to": 2, "cue": "word"},
                    "trace": {"function": 0, "from": -2, "to": 2, "cue": "word"},
                    "x_label": "x", "y_label": "y"},
    },
    "vectors": {
        "use": "arrows on a grid: forces, velocities, directions; tip-to-tail sums",
        "content": {"vectors": [{"vec": [3, 0], "label": "LaTeX", "cue": "word", "color": "accent"}],
                    "chain": "true = tip to tail, for sums of displacements/velocities (default); false = every vector starts at the origin, for forces acting on one object", "sum": {"label": "LaTeX", "cue": "word"}},
    },
    "dot_grid": {
        "use": "a population (4-1000 dots) where groups light up: probability, statistics, 1 in N",
        "content": {"total": 100, "groups": [{"count": 1, "label": "<=4 words", "cue": "word",
                                                "color": "accent2"}],
                    "focus_cue": "word to dim everything else", "result": "optional <=4 words verdict",
                    "result_cue": "word"},
    },
    "network": {
        "use": "neural networks, graphs of connected ideas, systems with signals flowing",
        "content": {"layers": [3, 5, 5, 2], "layer_labels": ["input", "hidden", "", "output"],
                    "pulses": [{"cue": "word"}], "path": "optional node index per layer to highlight",
                    "path_cue": "word",
                    "nodes": "alternative to layers: ['A','B','C'] in a ring", "edges": [[0, 1], [1, 2]]},
    },
}

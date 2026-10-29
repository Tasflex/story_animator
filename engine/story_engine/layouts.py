"""Scene layouts — the visual vocabulary the storyboard can choose from.

Each layout reads `ctx.content` (as authored in the storyboard), builds its
mobjects, and registers beats with `ctx.at(...)`. Reveal times come from
`ctx.schedule(cues)`, so each element appears as the narrator says it.

Every layout must tolerate missing/odd content: use defaults, clamp counts,
never raise for bad input. (If one does raise, the director falls back to
`layout_title`.)
"""

from __future__ import annotations

import math

import numpy as np
from manim import (
    DOWN,
    LEFT,
    PI,
    RIGHT,
    TAU,
    UP,
    AnimationGroup,
    Arc,
    Arrow,
    Circle,
    Create,
    Dot,
    FadeIn,
    FadeOut,
    GrowArrow,
    GrowFromCenter,
    GrowFromEdge,
    LaggedStart,
    Line,
    MoveAlongPath,
    Polygon,
    Rectangle,
    Rotate,
    RoundedRectangle,
    ScaleInPlace,
    Triangle,
    VGroup,
    VMobject,
    Wiggle,
    always_redraw,
    rate_functions,
)
from manim import ValueTracker

from .characters import make_character
from .director import Ctx
from .stage import card, fill, fit

POP = rate_functions.ease_out_back
EASE = rate_functions.ease_out_cubic


# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------

def _items(ctx: Ctx, key: str = "items", hi: int = 5) -> list[dict]:
    raw = ctx.content.get(key) or []
    out = []
    for it in raw if isinstance(raw, list) else []:
        if isinstance(it, str):
            it = {"label": it}
        if isinstance(it, dict):
            out.append(it)
        if len(out) >= hi:
            break
    return out


def _cue_of(it: dict, *keys) -> str:
    for k in ("cue", *keys):
        v = it.get(k)
        if v:
            return str(v)
    return ""


def _pop(m, shift=UP * 0.3):
    return FadeIn(m, shift=shift, scale=0.7, rate_func=POP)


def _pulse(m, factor=1.1):
    return ScaleInPlace(m, factor, rate_func=rate_functions.there_and_back)


def _ring_burst(ctx: Ctx, m, color=None, grow=1.7):
    def make():
        ring = Circle(radius=max(m.width, m.height) * 0.55, stroke_color=color or ctx.theme.accent,
                      stroke_width=6).move_to(m.get_center())
        ctx.scene.add(ring)
        return [ring.animate.scale(grow).set_stroke(opacity=0), _pulse(m, 1.08)]
    return make


def _halo(ctx: Ctx, m, color=None, scale=0.78):
    r = max(m.width, m.height) * scale
    col = color or ctx.theme.accent
    return VGroup(Circle(radius=r * 1.25, stroke_width=0, fill_color=col, fill_opacity=0.06),
                  Circle(radius=r, stroke_width=0, fill_color=col, fill_opacity=0.10),
                  Circle(radius=r, stroke_color=col, stroke_width=2.5, stroke_opacity=0.35)).move_to(m.get_center())


def _pill(ctx: Ctx, text: str, color=None, size=26):
    col = color or ctx.theme.accent
    t = ctx.text(text, size=size, weight="BOLD", color=ctx.theme.bg if ctx.theme.dark else "#FFFFFF")
    box = RoundedRectangle(corner_radius=min(0.3, (t.height + 0.3) / 2), width=t.width + 0.6,
                           height=t.height + 0.32, stroke_width=0, fill_color=col, fill_opacity=1)
    return VGroup(box, t.move_to(box))


def _region_center(ctx: Ctx, use_title_area: bool = False):
    st = ctx.stage
    if use_title_area:
        return np.array([0.0, (st.title_y + 0.5 + st.caption_top) / 2, 0.0])
    return st.content_center


def _emphasis_time(ctx: Ctx, last: float, key: str = "highlight_cue") -> float:
    t = ctx.cue(ctx.content.get(key), None, after=last)
    if t is None:
        t = min(max(last + 1.0, ctx.narr_end - 1.2), ctx.content_end - 1.0)
    return t


# ---------------------------------------------------------------------------
# title — hook / outro card
# ---------------------------------------------------------------------------

def layout_title(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    heading = str(c.get("heading") or ctx.s.get("title") or "")
    sub = str(c.get("subheading") or "")
    parts = VGroup()
    icon_grp = None
    if c.get("icon"):
        icon = ctx.icon(c["icon"], size=2.6 if st.portrait else 2.1)
        icon_grp = VGroup(_halo(ctx, icon), icon)
        parts.add(icon_grp)
    head = ctx.text(heading or " ", size=64 if st.portrait else 76, weight="BOLD",
                    max_width=st.content_w, max_chars=13 if st.portrait else 22)
    bar = Rectangle(width=1.4, height=0.09, stroke_width=0, fill_color=th.accent, fill_opacity=1)
    parts.add(head, bar)
    subt = None
    if sub:
        subt = ctx.text(sub, size=34 if st.portrait else 36, color=th.muted, max_width=st.content_w,
                        max_chars=24 if st.portrait else 44)
        parts.add(subt)
    parts.arrange(DOWN, buff=0.38)
    fill(parts, st.content_w, st.content_h + 1.2, frac=0.85, cap=1.2)
    parts.move_to(_region_center(ctx, True))

    t0 = 0.2
    if icon_grp is not None:
        ctx.at(t0, lambda: [GrowFromCenter(icon_grp[0]), FadeIn(icon_grp[1], scale=0.5, rate_func=POP)], 0.9)
        ctx.float(icon_grp, 0.07)
        t0 += 0.35
    ctx.at(t0, lambda: LaggedStart(*[FadeIn(ch, shift=UP * 0.25) for ch in head.family_members_with_points()], lag_ratio=0.04), 1.3)
    ctx.at(t0 + 0.9, lambda: GrowFromCenter(bar), 0.5)
    if subt is not None:
        ts = ctx.cue(c.get("subheading_cue"), None) or max(t0 + 1.4, ctx.narr_start + 1.2)
        ctx.at(ts, lambda: FadeIn(subt, shift=UP * 0.2), 0.7)
    if icon_grp is not None:
        ctx.at(max(t0 + 2.4, ctx.narr_end - 1.5), _ring_burst(ctx, icon_grp[1]), 0.9)


# ---------------------------------------------------------------------------
# hero — one big visual
# ---------------------------------------------------------------------------

def layout_hero(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    icon = ctx.icon(c.get("icon") or "lightbulb", size=3.4 if st.portrait else 2.8, label=c.get("label", ""))
    halo = _halo(ctx, icon)
    hero = VGroup(halo, icon)
    stack = VGroup(hero)
    label = tag = None
    if c.get("label"):
        label = ctx.text(c["label"], size=54, weight="BOLD", max_width=st.content_w, max_chars=24)
        stack.add(label)
    if c.get("tag"):
        tag = _pill(ctx, str(c["tag"]), size=32)
        stack.add(tag)
    stack.arrange(DOWN, buff=0.35)
    fit(stack, st.content_w, st.content_h)
    stack.move_to(st.content_center)

    t_in = 0.25
    ctx.at(t_in, lambda: [GrowFromCenter(halo), FadeIn(icon, scale=0.4, rate_func=POP)], 0.9)
    ctx.float(hero, 0.06)
    last = t_in
    if label is not None:
        t = ctx.cue(c.get("label_cue") or c.get("label"), None) or ctx.narr_start + 0.7
        last = max(t, t_in + 0.6)
        ctx.at(last, lambda: FadeIn(label, shift=UP * 0.25), 0.6)
    if tag is not None:
        t = ctx.cue(c.get("tag_cue") or c.get("tag"), None, after=last) or last + 1.2
        last = max(t, last + 0.5)
        ctx.at(last, lambda: _pop(tag), 0.6)

    effect = str(c.get("effect", "pulse")).lower()
    te = ctx.cue(c.get("effect_cue"), None, after=last)
    if te is None:
        te = min(max(last + 1.2, (ctx.narr_start + ctx.narr_end) / 2), ctx.content_end - 1.2)
    if effect == "orbit":
        dots = VGroup(*[Dot(radius=0.09, color=ctx.accent(k + 1)) for k in range(3)])
        R = max(icon.width, icon.height) * 0.85
        state = {"a": 0.0}

        def orbit(m, dt):
            state["a"] += dt * 1.6
            for k, d in enumerate(m):
                a = state["a"] + k * TAU / 3
                d.move_to(icon.get_center() + R * np.array([math.cos(a), 0.45 * math.sin(a), 0]))
        dots.add_updater(orbit)
        ctx.at(te, lambda: FadeIn(dots), 0.6)
    elif effect == "spin":
        ctx.at(te, lambda: Rotate(icon, TAU, rate_func=rate_functions.ease_in_out_cubic), 1.2)
    elif effect == "shake":
        ctx.at(te, lambda: Wiggle(icon, scale_value=1.1, rotation_angle=0.06 * TAU), 0.9)
    elif effect == "grow":
        ctx.at(te, lambda: [icon.animate.scale(1.25), halo.animate.scale(1.35)], 1.0)
    else:
        ctx.at(te, _ring_burst(ctx, icon), 0.9)


# ---------------------------------------------------------------------------
# icon_row — 2-5 items side by side
# ---------------------------------------------------------------------------

def _icon_card(ctx: Ctx, it: dict, i: int, w: float, h: float):
    th = ctx.theme
    box = card(w, h, th)
    icon = ctx.icon(it.get("icon"), size=min(w * 0.5, h * 0.42), accent=i, label=it.get("label", ""))
    label = ctx.text(it.get("label", ""), size=30, weight="BOLD", max_width=w * 0.86, max_chars=14)
    inner = VGroup(icon, label)
    if it.get("sub"):
        inner.add(ctx.text(it["sub"], size=22, color=th.muted, max_width=w * 0.86, max_chars=18))
    inner.arrange(DOWN, buff=0.22)
    fit(inner, w * 0.9, h * 0.88)
    inner.move_to(box)
    dot = Circle(radius=0.07, stroke_width=0, fill_color=ctx.accent(i), fill_opacity=1)
    dot.move_to(box.get_corner(UP + LEFT) + np.array([0.25, -0.25, 0]))
    return VGroup(box, inner, dot)


def layout_icon_row(ctx: Ctx):
    st, c = ctx.stage, ctx.content
    items = _items(ctx, "items", 5) or [{"label": ctx.s.get("title", "Idea"), "icon": "lightbulb"}]
    n = len(items)
    if st.portrait:
        per_row = 1 if n == 1 else 2
        w = min(3.3, (st.content_w - 0.4) / per_row)
        rows = math.ceil(n / per_row)
        h = min(w * 1.05, (st.content_h - 0.3 * (rows - 1)) / rows)
    else:
        per_row = n
        w = min(3.0, (st.content_w - 0.45 * (n - 1)) / n)
        h = min(w * 1.15, st.content_h * 0.9)
    cells = [_icon_card(ctx, it, i, w, h) for i, it in enumerate(items)]
    grid = VGroup()
    for r in range(0, n, per_row):
        grid.add(VGroup(*cells[r:r + per_row]).arrange(RIGHT, buff=0.4 if st.portrait else 0.45))
    grid.arrange(DOWN, buff=0.35)
    fill(grid, st.content_w, st.content_h, cap=1.25)
    grid.move_to(st.content_center)

    times = ctx.schedule([_cue_of(it, "label") for it in items])
    for cell, t in zip(cells, times):
        ctx.at(t, lambda cell=cell: _pop(cell, UP * 0.4), 0.7)
        ctx.float(cell, 0.035, 1.1)
    hl = c.get("highlight")
    if isinstance(hl, int) and 0 <= hl < n:
        te = _emphasis_time(ctx, times[-1])
        others = [cl for k, cl in enumerate(cells) if k != hl]

        def emph():
            target = cells[hl]
            frame = RoundedRectangle(corner_radius=0.32, width=target.width + 0.22, height=target.height + 0.22,
                                     stroke_color=ctx.theme.accent, stroke_width=6).move_to(target)
            return [*(o.animate.set_opacity(0.35) for o in others), Create(frame), _pulse(target, 1.06)]
        ctx.at(te, emph, 0.8)


# ---------------------------------------------------------------------------
# list — bullets revealed one by one
# ---------------------------------------------------------------------------

def layout_list(ctx: Ctx):
    st, th = ctx.stage, ctx.theme
    items = _items(ctx, "items", 5) or [{"text": ctx.s.get("title", "")}]
    rows = []
    for i, it in enumerate(items):
        badge_c = Circle(radius=0.42, stroke_width=0, fill_color=ctx.accent(i), fill_opacity=1)
        if it.get("icon"):
            glyph = ctx.icon(it["icon"], size=0.52, accent=i + 1)
            inner_bg = Circle(radius=0.36, stroke_width=0, fill_color=th.bg, fill_opacity=0.9)
            badge = VGroup(badge_c, inner_bg, glyph)
        else:
            badge = VGroup(badge_c, ctx.text(str(i + 1), size=32, weight="BOLD", color=th.bg))
        txt = ctx.text(it.get("text") or it.get("label", ""), size=34, max_chars=18 if st.portrait else 34,
                       max_width=st.content_w - 1.5, align="left")
        rows.append(VGroup(badge, txt).arrange(RIGHT, buff=0.35))
    stack = VGroup(*rows).arrange(DOWN, aligned_edge=LEFT, buff=0.42)
    fill(stack, st.content_w - 1.0, st.content_h - 0.8, frac=1.0, cap=1.3)
    panel = card(stack.width + 1.0, stack.height + 0.8, th, opacity=0.55)
    panel.move_to(st.content_center)
    stack.move_to(panel)

    ctx.at(0.2, lambda: FadeIn(panel, scale=0.96), 0.6)
    times = ctx.schedule([_cue_of(it, "text", "label") for it in items])
    for i, (row, t) in enumerate(zip(rows, times)):
        prev = rows[:i]
        ctx.at(t, lambda row=row, prev=prev: [FadeIn(row, shift=RIGHT * 0.5),
                                               *(p.animate.set_opacity(0.5) for p in prev)], 0.6)
    end_t = max(times[-1] + 1.2, ctx.narr_end - 0.6)
    if end_t < ctx.content_end - 0.6:
        ctx.at(end_t, lambda: [r.animate.set_opacity(1) for r in rows[:-1]], 0.6)


# ---------------------------------------------------------------------------
# compare — A vs B
# ---------------------------------------------------------------------------

def layout_compare(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    sides = []
    for key in ("left", "right"):
        d = c.get(key)
        sides.append(d if isinstance(d, dict) else {"label": str(d or key.title())})
    if st.portrait:
        pw, ph = st.content_w, (st.content_h - 0.7) / 2
    else:
        pw, ph = min(5.3, (st.content_w - 1.3) / 2), st.content_h * 0.95
    panels = []
    for i, sd in enumerate(sides):
        col = th.accent if i == 0 else th.accent2
        box = card(pw, ph, th)
        icon = ctx.icon(sd.get("icon"), size=min(1.5, ph * 0.3), accent=i, label=sd.get("label", ""))
        label = ctx.text(sd.get("label", ""), size=40, weight="BOLD", color=col, max_width=pw * 0.85, max_chars=16)
        pts = VGroup(*[ctx.text("•  " + str(p), size=26, max_width=pw * 0.85, max_chars=26, align="left")
                       for p in (sd.get("points") or [])[:3]])
        pts.arrange(DOWN, aligned_edge=LEFT, buff=0.2)
        head = VGroup(icon, label).arrange(RIGHT if st.portrait else DOWN, buff=0.3)
        inner = VGroup(head, pts) if len(pts) else VGroup(head)
        inner.arrange(DOWN, buff=0.4)
        fit(inner, pw * 0.88, ph * 0.86)
        inner.move_to(box)
        panels.append(VGroup(box, inner))
    duo = VGroup(*panels).arrange(DOWN if st.portrait else RIGHT, buff=0.7 if st.portrait else 1.3)
    fill(duo, st.content_w, st.content_h, frac=0.95, cap=1.2)
    duo.move_to(st.content_center)
    vs_c = Circle(radius=0.52, stroke_color=th.bg, stroke_width=6, fill_color=th.accent3, fill_opacity=1)
    vs = VGroup(vs_c, ctx.text("VS", size=32, weight="BOLD", color=th.bg)).move_to(
        (panels[0].get_center() + panels[1].get_center()) / 2)

    t_left = min(ctx.cue(_cue_of(sides[0], "label"), None) or ctx.narr_start + 0.1, ctx.first_by)
    t_right = ctx.cue(_cue_of(sides[1], "label"), None, after=t_left + 0.4)
    if t_right is None:
        t_right = t_left + max((ctx.narr_end - t_left) * 0.45, 1.2)
    t_right = max(t_right, t_left + 0.9)
    shift_l = DOWN * 0.4 if st.portrait else RIGHT * 0.6
    ctx.at(t_left, lambda: FadeIn(panels[0][0], shift=shift_l), 0.7)
    ctx.at(t_left + 0.15, lambda: LaggedStart(*[FadeIn(m, shift=UP * 0.2) for m in panels[0][1]], lag_ratio=0.3), 0.8)
    ctx.at((t_left + t_right) / 2, lambda: GrowFromCenter(vs), 0.5)
    ctx.at(t_right, lambda: FadeIn(panels[1][0], shift=-shift_l), 0.7)
    ctx.at(t_right + 0.15, lambda: LaggedStart(*[FadeIn(m, shift=UP * 0.2) for m in panels[1][1]], lag_ratio=0.3), 0.8)

    winner = str(c.get("winner", "")).lower()
    if winner in ("left", "right"):
        wi = 0 if winner == "left" else 1
        te = _emphasis_time(ctx, t_right + 0.8, "winner_cue")

        def emph():
            w, l = panels[wi], panels[1 - wi]
            frame = RoundedRectangle(corner_radius=0.32, width=w.width + 0.2, height=w.height + 0.2,
                                     stroke_color=th.accent, stroke_width=7).move_to(w)
            badge = ctx.icon("check", size=0.9).move_to(w.get_corner(UP + RIGHT))
            return [l.animate.set_opacity(0.3), Create(frame), FadeIn(badge, scale=0.4, rate_func=POP)]
        ctx.at(te, emph, 0.8)


# ---------------------------------------------------------------------------
# stat — one big animated number
# ---------------------------------------------------------------------------

def _num(v) -> float:
    try:
        return float(str(v).replace(",", "").strip())
    except ValueError:
        return 0.0


def layout_stat(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    value = _num(c.get("value", 0))
    start = _num(c.get("from", 0))
    prefix, suffix = str(c.get("prefix", "")), str(c.get("suffix", ""))
    dec = c.get("decimals")
    dec = int(dec) if isinstance(dec, (int, float)) else (0 if float(value).is_integer() else 1)
    tracker = ValueTracker(start)
    ring_mode = suffix.strip() == "%" and 0 < value <= 100
    size = 120 if ring_mode else (150 if not st.portrait else 130)
    anchor = Dot(radius=0.001).set_opacity(0)
    ring_w = [0.0]  # filled in once the ring is sized

    def number():
        v = tracker.get_value()
        s = f"{prefix}{v:,.{dec}f}{suffix}"
        t = ctx.text(s, size=size, weight="BOLD", color=th.accent)
        limit = ring_w[0] * 0.7 if ring_mode else st.content_w * 0.9
        if t.width > limit:
            t.scale_to_fit_width(limit)
        return t.move_to(anchor.get_center())

    label = ctx.text(c.get("label", ""), size=40, max_width=st.content_w, max_chars=22 if st.portrait else 34)
    if ring_mode:
        R = min(st.content_h * 0.36, 2.1)
        track = Circle(radius=R, stroke_color=th.muted, stroke_width=18, stroke_opacity=0.25)
        top = VGroup(track)
        layout = VGroup(top, label).arrange(DOWN, buff=0.45)
        fit(layout, st.content_w, st.content_h)
        layout.move_to(st.content_center)
        anchor.move_to(track.get_center())
        ring_w[0] = track.width

        def ring():
            frac = max(tracker.get_value(), 0.0001) / 100.0
            a = Arc(radius=track.width / 2, start_angle=PI / 2, angle=-TAU * min(frac, 1.0),
                    stroke_color=th.accent, stroke_width=18).move_arc_center_to(track.get_center())
            from manim import CapStyleType
            a.set_cap_style(CapStyleType.ROUND)
            return a
        visual = VGroup(track)
        dynamic = [always_redraw(ring), always_redraw(number)]
    else:
        icon = ctx.icon(c.get("icon"), size=1.5) if c.get("icon") else None
        probe = ctx.text(f"{prefix}{value:,.{dec}f}{suffix}", size=size, weight="BOLD")
        holder = Rectangle(width=probe.width, height=probe.height, stroke_width=0, fill_opacity=0)
        parts = VGroup(*([icon] if icon else []), holder, label).arrange(DOWN, buff=0.45)
        fit(parts, st.content_w, st.content_h)
        parts.move_to(st.content_center)
        anchor.move_to(holder.get_center())
        visual = VGroup(icon) if icon else VGroup()
        dynamic = [always_redraw(number)]
        if icon:
            ctx.float(icon, 0.05)

    # The counter starts rolling early (a frozen "0" reads as broken) and lands
    # on the value as the narrator says it.
    t_say = ctx.cue(c.get("cue"), None) or ctx.narr_start + 0.4
    t0 = max(min(t_say, ctx.first_by + 0.3), 0.5)
    count_len = min(max(2.0, t_say - t0 + 0.4), max(ctx.content_end - t0 - 1.0, 1.0))
    ctx.scene.add(anchor)
    ctx.at(0.2, lambda: FadeIn(visual, scale=0.8, rate_func=POP), 0.7)
    ctx.at(t0 - 0.3, lambda: [FadeIn(d) for d in dynamic], 0.3)
    ctx.at(t0, lambda: tracker.animate(rate_func=EASE).set_value(value), count_len)
    t0 = t_say
    tl = ctx.cue(c.get("label_cue") or c.get("label"), None, after=t0 - 1)
    ctx.at(max(tl or t0 + 0.8, 0.6), lambda: FadeIn(label, shift=UP * 0.2), 0.6)
    if c.get("context"):
        ctxt = ctx.text(c["context"], size=28, color=th.muted, max_width=st.content_w, max_chars=40)
        ctxt.next_to(label, DOWN, buff=0.25)
        tc = ctx.cue(c.get("context"), None, after=t0) or t0 + 2.4
        ctx.at(tc, lambda: FadeIn(ctxt, shift=UP * 0.15), 0.6)
    ctx.at(min(t0 + 2.3, ctx.content_end - 1.0), _ring_burst(ctx, anchor.copy().scale(1).set_width(3), th.accent), 0.9)


# ---------------------------------------------------------------------------
# bar_chart
# ---------------------------------------------------------------------------

def layout_bar_chart(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    bars_in = _items(ctx, "bars", 6) or [{"label": "A", "value": 3}, {"label": "B", "value": 5}]
    n = len(bars_in)
    vals = [max(_num(b.get("value", 0)), 0.0) for b in bars_in]
    vmax = max(vals) or 1.0
    unit = str(c.get("unit", ""))
    hl = c.get("highlight")
    hl = hl if isinstance(hl, int) and 0 <= hl < n else int(np.argmax(vals))
    W = min(st.content_w * (0.95 if st.portrait else 0.85), n * 2.4)
    H = st.content_h * 0.74
    cc = st.content_center
    base_y = cc[1] - st.content_h / 2 + 0.75
    slot = W / n
    bw = min(1.5, slot * 0.62)
    baseline = Line([cc[0] - W / 2 - 0.2, base_y, 0], [cc[0] + W / 2 + 0.2, base_y, 0],
                    stroke_color=th.muted, stroke_width=4)
    bars, vlabels, clabels = [], [], []
    for i, (b, v) in enumerate(zip(bars_in, vals)):
        x = cc[0] - W / 2 + slot * (i + 0.5)
        h = max(H * v / vmax, 0.08)
        col = th.accent if i == hl else th.accent2
        bar = RoundedRectangle(corner_radius=min(0.1, bw / 4), width=bw, height=h, stroke_width=0,
                               fill_color=col, fill_opacity=1 if i == hl else 0.55)
        bar.move_to([x, base_y + h / 2, 0])
        disp = b.get("display") or f"{v:,.{0 if v.is_integer() else 1}f}{unit}"
        vl = ctx.text(disp, size=28, weight="BOLD", color=th.ink, max_width=slot * 0.95).next_to(bar, UP, buff=0.15)
        cl = ctx.text(b.get("label", ""), size=24, color=th.muted, max_width=slot * 0.95, max_chars=12)
        cl.next_to([x, base_y, 0], DOWN, buff=0.18)
        bars.append(bar), vlabels.append(vl), clabels.append(cl)

    ctx.at(0.2, lambda: [Create(baseline), LaggedStart(*[FadeIn(cl) for cl in clabels], lag_ratio=0.1)], 0.8)
    times = ctx.schedule([_cue_of(b, "label") for b in bars_in], start=max(ctx.narr_start, 0.6))
    for bar, vl, t in zip(bars, vlabels, times):
        ctx.at(t, lambda bar=bar, vl=vl: [GrowFromEdge(bar, DOWN, rate_func=EASE), FadeIn(vl, shift=UP * 0.3)], 0.8)
    te = _emphasis_time(ctx, times[-1])

    def emph():
        return [*(b.animate.set_opacity(0.3) for k, b in enumerate(bars) if k != hl), _pulse(bars[hl], 1.06),
                _pulse(vlabels[hl], 1.2)]
    ctx.at(te, emph, 0.8)


# ---------------------------------------------------------------------------
# flow — step-by-step process
# ---------------------------------------------------------------------------

def layout_flow(ctx: Ctx):
    st, th = ctx.stage, ctx.theme
    steps = _items(ctx, "steps", 5) or _items(ctx, "items", 5) or [{"label": "Start"}, {"label": "End"}]
    n = len(steps)
    nodes = []
    for i, s in enumerate(steps):
        ring = Circle(radius=0.95 if n <= 4 else 0.8, stroke_color=ctx.accent(i), stroke_width=7,
                      fill_color=th.bg2, fill_opacity=1)
        icon = ctx.icon(s.get("icon"), size=ring.width * 0.58, accent=i, label=s.get("label", ""))
        label = ctx.text(s.get("label", ""), size=32, weight="BOLD", max_chars=12,
                         max_width=2.4 if not st.portrait else 4.2)
        num = VGroup(Circle(radius=0.22, stroke_width=0, fill_color=ctx.accent(i), fill_opacity=1),
                     ctx.text(str(i + 1), size=20, weight="BOLD", color=th.bg))
        num.move_to(ring.get_corner(UP + LEFT) + np.array([0.15, -0.15, 0]))
        badge = VGroup(ring, icon.move_to(ring), num)
        if st.portrait:
            label.next_to(badge, RIGHT, buff=0.35)
        else:
            label.next_to(badge, DOWN, buff=0.25)
        nodes.append(VGroup(badge, label))
    if st.portrait:
        chain = VGroup(*nodes).arrange(DOWN, buff=0.75, aligned_edge=LEFT)
    else:
        chain = VGroup(*nodes).arrange(RIGHT, buff=max(0.6, min(1.4, (st.content_w - n * 2.2) / max(n - 1, 1))))
    fill(chain, st.content_w, st.content_h, frac=0.92, cap=1.3)
    chain.move_to(st.content_center)
    arrows = []
    for a, b in zip(nodes, nodes[1:]):
        p, q = a[0][0], b[0][0]
        arrows.append(Arrow(p.get_center(), q.get_center(), buff=p.width / 2 + 0.12, stroke_width=5,
                            color=th.muted, max_tip_length_to_length_ratio=0.2))

    times = ctx.schedule([_cue_of(s, "label") for s in steps])
    for i, (node, t) in enumerate(zip(nodes, times)):
        if i == 0:
            ctx.at(t, lambda node=node: _pop(node), 0.7)
        else:
            ar = arrows[i - 1]
            ctx.at(t - 0.25, lambda ar=ar, node=node: AnimationGroup(GrowArrow(ar), _pop(node), lag_ratio=0.45), 0.9)
    tt = times[-1] + 1.0
    if n > 1 and tt < ctx.content_end - 1.4:
        path = VMobject().set_points_as_corners([nd[0][0].get_center() for nd in nodes])
        dot = VGroup(Circle(radius=0.26, stroke_width=0, fill_color=th.accent, fill_opacity=0.25),
                     Dot(radius=0.12, color=th.accent)).move_to(nodes[0][0][0].get_center())

        def travel():
            ctx.scene.add(dot)
            return MoveAlongPath(dot, path, rate_func=rate_functions.ease_in_out_sine)
        ctx.at(tt, travel, min(0.5 * n + 0.6, ctx.content_end - tt - 0.6))
        ctx.at(tt + min(0.5 * n + 0.6, ctx.content_end - tt - 0.6), _ring_burst(ctx, nodes[-1][0][0]), 0.7)


# ---------------------------------------------------------------------------
# timeline
# ---------------------------------------------------------------------------

def layout_timeline(ctx: Ctx):
    st, th = ctx.stage, ctx.theme
    events = _items(ctx, "events", 5) or [{"date": "Then"}, {"date": "Now"}]
    n = len(events)
    cc = st.content_center
    if st.portrait:
        x0 = -st.content_w / 2 + 0.7
        top, bot = cc[1] + st.content_h / 2 - 0.4, cc[1] - st.content_h / 2 + 0.4
        line = Line([x0, top, 0], [x0, bot, 0], stroke_color=th.muted, stroke_width=5)
        pts = [np.array([x0, top - (top - bot) * (i + 0.5) / n, 0]) for i in range(n)]
    else:
        L = st.content_w * 0.94
        line = Line([-L / 2, cc[1], 0], [L / 2, cc[1], 0], stroke_color=th.muted, stroke_width=5)
        pts = [np.array([-L / 2 + L * (i + 0.5) / n, cc[1], 0]) for i in range(n)]
    slot = (st.content_w * 0.94) / n
    markers, blocks = [], []
    for i, (e, p) in enumerate(zip(events, pts)):
        mk = VGroup(Circle(radius=0.22, stroke_width=0, fill_color=ctx.accent(i), fill_opacity=0.25),
                    Circle(radius=0.13, stroke_width=0, fill_color=ctx.accent(i), fill_opacity=1)).move_to(p)
        date = ctx.text(e.get("date") or e.get("label", ""), size=34, weight="BOLD", color=ctx.accent(i),
                        max_width=slot * 0.95 if not st.portrait else 5.5)
        body = ctx.text(e.get("text", ""), size=24, max_chars=16 if not st.portrait else 26,
                        max_width=slot * 0.95 if not st.portrait else 5.8) if e.get("text") else VGroup()
        icon = ctx.icon(e["icon"], size=0.8, accent=i) if e.get("icon") else None
        if st.portrait:
            col = VGroup(*([icon] if icon else []), date).arrange(RIGHT, buff=0.2)
            blk = VGroup(col, body).arrange(DOWN, aligned_edge=LEFT, buff=0.12)
            blk.next_to(mk, RIGHT, buff=0.4)
        else:
            above = VGroup(*([icon] if icon else []), date).arrange(DOWN, buff=0.15).next_to(mk, UP, buff=0.3)
            below = body.next_to(mk, DOWN, buff=0.3) if e.get("text") else VGroup()
            blk = VGroup(above, below)
        markers.append(mk)
        blocks.append(blk)
    head = Triangle(fill_color=th.ink, fill_opacity=1, stroke_width=0).scale(0.14)
    head.rotate(PI if not st.portrait else -PI / 2)
    off = UP * 0.42 if not st.portrait else LEFT * 0.42
    head.move_to(line.get_start() + off)

    ctx.at(0.2, lambda: [Create(line), FadeIn(head)], 0.9)
    times = ctx.schedule([_cue_of(e, "date", "label") for e in events], start=max(ctx.narr_start, 0.9))
    for i, (mk, blk, t) in enumerate(zip(markers, blocks, times)):
        prev = blocks[:i]
        ctx.at(t, lambda mk=mk, blk=blk, prev=prev: [
            head.animate(rate_func=EASE).move_to(mk.get_center() + off),
            GrowFromCenter(mk), FadeIn(blk, shift=UP * 0.2), *(p.animate.set_opacity(0.55) for p in prev)], 0.8)


# ---------------------------------------------------------------------------
# character — little story moments
# ---------------------------------------------------------------------------

def _bubble(ctx: Ctx, text: str, tail_to, thought: bool = False):
    t = ctx.text(text, size=30, weight="SEMIBOLD", color="#1D1E2C", max_chars=18, max_width=4.6)
    box = RoundedRectangle(corner_radius=0.35, width=t.width + 0.7, height=t.height + 0.55, stroke_color="#1D1E2C",
                           stroke_width=3, fill_color="#FFFFFF", fill_opacity=0.97)
    t.move_to(box)
    grp = VGroup(box, t)
    return grp


def layout_character(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    specs = [s for s in (c.get("characters") or []) if isinstance(s, dict)][:2] or [{"pose": "wave"}]
    chars = [make_character(th, s, i) for i, s in enumerate(specs)]
    n = len(chars)
    crowded = bool(c.get("bubble")) and (n == 2 or st.portrait)
    target_h = min(st.content_h * (0.62 if st.portrait else 0.8), 4.2) * (0.78 if crowded else 1.0)
    cc = st.content_center
    floor_y = cc[1] - st.content_h / 2 + 0.25
    prop = None
    if c.get("prop"):
        prop = ctx.icon(c["prop"], size=1.5 if not st.portrait else 1.4)
    for ch in chars:
        ch.scale(target_h / ch.height)
    if n == 1:
        x = -2.4 if not st.portrait else 0.0
        if prop is not None and not st.portrait:
            x = -3.0
        chars[0].move_to([x, floor_y + target_h / 2, 0])
    else:
        dx = 3.0 if not st.portrait else 1.85
        for ch, s in zip(chars, (-1, 1)):
            ch.move_to([s * dx, floor_y + target_h / 2, 0])
    shadows = VGroup(*[Circle(radius=0.6, stroke_width=0, fill_color="#000000", fill_opacity=0.22)
                       .stretch(0.22, 1).scale(target_h / 3.4).move_to([ch.get_center()[0], floor_y + 0.02, 0])
                       for ch in chars])
    speaking = [bool(s.get("speaking", i == 0)) for i, s in enumerate(specs)]
    speech = [(w["s"], w["e"]) for w in ctx.words]
    for ch, talk in zip(chars, speaking):
        ch.attach_life(ctx.clock, speech, talk)

    bubble = None
    if c.get("bubble"):
        who = next((i for i, t in enumerate(speaking) if t), 0)
        spk = chars[who]
        bubble = _bubble(ctx, str(c["bubble"]), spk.get_top())
        if st.portrait or n == 2:
            bubble.next_to(spk, UP, buff=0.35)
            if n == 2:
                inward = RIGHT if spk.get_center()[0] < 0 else LEFT
                bubble.shift(inward * (bubble.width / 2 - 0.4))
        else:
            bubble.next_to(spk.get_corner(UP + RIGHT), RIGHT, buff=0.2).shift(DOWN * 0.4)
        top_lim = st.title_y - 0.55
        if bubble.get_top()[1] > top_lim:
            bubble.shift(DOWN * (bubble.get_top()[1] - top_lim))
        half_w = st.width / 2 - 0.3
        if bubble.get_right()[0] > half_w:
            bubble.shift(LEFT * (bubble.get_right()[0] - half_w))
        if bubble.get_left()[0] < -half_w:
            bubble.shift(RIGHT * (-half_w - bubble.get_left()[0]))
        anchor = spk.get_top() + DOWN * 0.3 + RIGHT * 0.25 * spk.facing
        bx = np.clip(anchor[0], bubble.get_left()[0] + 0.5, bubble.get_right()[0] - 0.5)
        base_y = bubble.get_bottom()[1] + 0.02 if anchor[1] < bubble.get_bottom()[1] else bubble.get_top()[1]
        tail = Polygon([bx - 0.22, base_y, 0], [bx + 0.22, base_y, 0], anchor + UP * 0.25,
                       fill_color="#FFFFFF", fill_opacity=0.97, stroke_color="#1D1E2C", stroke_width=3)
        bubble = VGroup(tail, *bubble)
    if prop is not None:
        if n == 1 and not st.portrait:
            prop.move_to([2.4, cc[1] + 0.2, 0])
        elif n == 1:
            prop.next_to(chars[0], UP, buff=0.3).shift(RIGHT * 1.8)
        else:
            prop.move_to([0, floor_y + target_h * 0.45, 0])
        prop_grp = VGroup(_halo(ctx, prop), prop)
    else:
        prop_grp = None

    for i, ch in enumerate(chars):
        enter = LEFT * 1.5 if ch.get_center()[0] <= 0 else RIGHT * 1.5
        ctx.at(0.15 + 0.25 * i, lambda ch=ch, enter=enter, sh=shadows[i]: [FadeIn(ch, shift=-enter), FadeIn(sh)], 0.8)
    for i, (ch, s) in enumerate(zip(chars, specs)):
        pose, expr = str(s.get("pose", "stand")), str(s.get("expression", "happy"))
        ch.pose = "stand"
        ctx.at(1.0 + 0.2 * i, lambda ch=ch, pose=pose, expr=expr: [ch.pose_to(pose), ch.express(expr)], 0.6)
    last = 1.2
    if bubble is not None:
        tb = ctx.cue(c.get("bubble_cue"), None) or max(ctx.narr_start + 0.6, 1.3)
        last = tb
        ctx.at(tb, lambda: FadeIn(bubble, scale=0.6, rate_func=POP), 0.6)
    if prop_grp is not None:
        tp = ctx.cue(c.get("prop_cue") or c.get("prop"), None) or max(last + 1.0, ctx.narr_start + 1.5)
        ctx.at(tp, lambda: [GrowFromCenter(prop_grp[0]), FadeIn(prop_grp[1], scale=0.4, rate_func=POP)], 0.8)
        ctx.float(prop_grp, 0.07)
    for b in (c.get("beats") or [])[:6]:
        if not isinstance(b, dict):
            continue
        idx = int(b.get("character", 0)) if str(b.get("character", 0)).isdigit() else 0
        ch = chars[min(idx, n - 1)]
        tb = ctx.cue(b.get("cue"), None)
        if tb is None:
            continue
        anims = []
        if b.get("pose"):
            anims.append(lambda ch=ch, p=str(b["pose"]): ch.pose_to(p))
        if b.get("expression"):
            anims.append(lambda ch=ch, e=str(b["expression"]): ch.express(e))
        ctx.at(max(tb, 1.7), lambda anims=anims: [f() for f in anims], 0.5)


# ---------------------------------------------------------------------------
# quote — big words, revealed as they are spoken
# ---------------------------------------------------------------------------

def layout_quote(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    text = " ".join(str(c.get("text") or ctx.s.get("title") or "").split())[:180]
    words = text.split() or ["…"]
    size = 54 if not st.portrait else 46
    max_chars = 26 if not st.portrait else 15
    lines, cur = [], []
    for w in words:
        if cur and len(" ".join(cur + [w])) > max_chars:
            lines.append(cur)
            cur = []
        cur.append(w)
    lines.append(cur)
    word_mobs = []
    line_grps = VGroup()
    for ln in lines:
        line_txt = ctx.text(" ".join(ln), size=size, weight="SEMIBOLD")
        glyphs = list(line_txt.submobjects)
        g, k = VGroup(), 0
        for w in ln:
            n_chars = len("".join(w.split()))
            g.add(VGroup(*glyphs[k:k + n_chars]))
            k += n_chars
        word_mobs.extend(g)
        line_grps.add(g)
    line_grps.arrange(DOWN, buff=0.3)
    mark = ctx.text("“", size=220, weight="BOLD", color=th.accent)
    block = VGroup(line_grps)
    author = None
    if c.get("author"):
        author = ctx.text("— " + str(c["author"]), size=32, color=th.muted, max_width=st.content_w)
        author.next_to(line_grps, DOWN, buff=0.5).align_to(line_grps, RIGHT)
        block.add(author)
    fit(block, st.content_w - 0.8, st.content_h)
    block.move_to(_region_center(ctx, not ctx.s.get("title")))
    mark.scale_to_fit_height(1.1).next_to(line_grps, UP, buff=0.25).align_to(line_grps, LEFT)
    mark.set_opacity(0.85)

    ctx.at(0.15, lambda: FadeIn(mark, shift=DOWN * 0.3, scale=1.3), 0.7)
    times = ctx.schedule(words, start=max(ctx.narr_start, 0.4), min_gap=0.12)
    for wm, t in zip(word_mobs, times):
        ctx.at(t, lambda wm=wm: FadeIn(wm, shift=UP * 0.15), 0.35)
    if author is not None:
        ctx.at(times[-1] + 0.5, lambda: FadeIn(author, shift=LEFT * 0.3), 0.6)
    key = c.get("highlight")
    if key:
        hits = [wm for wm, w in zip(word_mobs, words) if w.strip(".,!?;:\"'").lower() == str(key).lower()]
        if hits:
            ctx.at(times[-1] + 0.9, lambda: [h.animate.set_color(th.accent) for h in hits], 0.5)


# ---------------------------------------------------------------------------
# cycle — loops / feedback systems
# ---------------------------------------------------------------------------

def layout_cycle(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    items = _items(ctx, "items", 5) or [{"label": "A"}, {"label": "B"}, {"label": "C"}]
    if len(items) < 3:
        items = items + [{"label": "…"}] * (3 - len(items))
    n = len(items)
    cc = st.content_center
    R = min(st.content_h * 0.4, 2.3) if not st.portrait else min(st.content_w * 0.34, 2.5)
    angles = [PI / 2 - k * TAU / n for k in range(n)]
    nodes = []
    for i, (it, a) in enumerate(zip(items, angles)):
        p = cc + R * np.array([math.cos(a), math.sin(a), 0])
        ring = Circle(radius=0.62, stroke_color=ctx.accent(i), stroke_width=6, fill_color=th.bg2,
                      fill_opacity=1).move_to(p)
        icon = ctx.icon(it.get("icon"), size=0.72, accent=i, label=it.get("label", "")).move_to(p)
        label = ctx.text(it.get("label", ""), size=26, weight="BOLD", max_chars=12, max_width=2.6)
        if abs(math.cos(a)) < 0.3:
            label.next_to(ring, RIGHT, buff=0.22)
        else:
            label.next_to(ring, RIGHT if math.cos(a) > 0 else LEFT, buff=0.22)
        if abs(label.get_x()) + label.width / 2 > st.width / 2 - 0.25:
            label.next_to(ring, DOWN, buff=0.15)
        nodes.append(VGroup(ring, icon, label))
    d = 0.62 / R + 0.08
    arcs = []
    for k in range(n):
        a0 = angles[k] - d
        sweep = TAU / n - 2 * d
        arc = Arc(radius=R, start_angle=a0, angle=-sweep, stroke_color=th.muted, stroke_width=5,
                  arc_center=cc)
        arc.add_tip(tip_length=0.22, tip_width=0.22)
        arcs.append(arc)
    center = None
    if c.get("center"):
        center = ctx.text(c["center"], size=30, weight="BOLD", color=th.accent, max_chars=10,
                          max_width=max(2 * R - 2 * 0.62 - 0.5, 1.0))
        center.move_to(cc)

    if center is not None:
        ctx.at(0.25, lambda: FadeIn(center, scale=0.8), 0.6)
    times = ctx.schedule([_cue_of(it, "label") for it in items])
    for i, (nd, t) in enumerate(zip(nodes, times)):
        anims = [lambda nd=nd: _pop(nd, UP * 0.2)]
        if i > 0:
            anims.insert(0, lambda ar=arcs[i - 1]: Create(ar))
        ctx.at(t, lambda anims=anims: [f() for f in anims], 0.7)
    tl = times[-1] + 0.7
    ctx.at(tl, lambda: Create(arcs[-1]), 0.6)
    if tl + 1.0 < ctx.content_end - 0.8:
        loop = Circle(radius=R, arc_center=cc).rotate(PI / 2, about_point=cc).flip(UP)
        dot = Dot(radius=0.12, color=th.accent).move_to(loop.point_from_proportion(0))

        def spin():
            ctx.scene.add(dot)
            return MoveAlongPath(dot, loop, rate_func=rate_functions.linear)
        ctx.at(tl + 0.7, spin, min(2.4, ctx.content_end - tl - 1.3))


LAYOUTS = {
    "title": layout_title,
    "hero": layout_hero,
    "icon_row": layout_icon_row,
    "list": layout_list,
    "compare": layout_compare,
    "stat": layout_stat,
    "bar_chart": layout_bar_chart,
    "flow": layout_flow,
    "timeline": layout_timeline,
    "character": layout_character,
    "quote": layout_quote,
    "cycle": layout_cycle,
}

# Human/LLM-facing documentation of each layout's `content` object. This is
# exported via `python -m story_engine catalog` and embedded in the storyboard
# prompt, so it is the single source of truth for what the LLM may write.
LAYOUT_DOCS = {
    "title": {
        "use": "opening hook or closing card: one bold statement",
        "content": {"heading": "<=6 words", "subheading": "optional, <=10 words", "icon": "optional icon"},
    },
    "hero": {
        "use": "one big symbol for a single idea",
        "content": {"icon": "icon", "label": "<=4 words", "tag": "optional <=3 words pill",
                    "effect": "pulse|orbit|spin|shake|grow", "effect_cue": "narration word that triggers effect"},
    },
    "icon_row": {
        "use": "2-5 parallel things (ingredients, options, causes)",
        "content": {"items": [{"icon": "icon", "label": "<=3 words", "sub": "optional <=4 words",
                               "cue": "narration word"}], "highlight": "optional index to emphasise"},
    },
    "list": {
        "use": "2-5 points / steps / reasons as readable bullets",
        "content": {"items": [{"icon": "optional icon", "text": "<=7 words", "cue": "narration word"}]},
    },
    "compare": {
        "use": "A versus B, before/after, myth/fact",
        "content": {"left": {"icon": "icon", "label": "<=3 words", "points": ["<=4 words"], "cue": "word"},
                    "right": {"icon": "icon", "label": "<=3 words", "points": ["<=4 words"], "cue": "word"},
                    "winner": "optional left|right", "winner_cue": "word"},
    },
    "stat": {
        "use": "one striking number that counts up (use suffix % for a progress ring)",
        "content": {"value": 73, "prefix": "optional e.g. $", "suffix": "optional e.g. % or M",
                    "decimals": 0, "label": "<=7 words", "context": "optional <=8 words", "cue": "word"},
    },
    "bar_chart": {
        "use": "2-6 quantities compared",
        "content": {"bars": [{"label": "<=2 words", "value": 10, "cue": "word"}], "unit": "optional suffix",
                    "highlight": "optional index"},
    },
    "flow": {
        "use": "2-5 step process / cause-and-effect chain",
        "content": {"steps": [{"icon": "icon", "label": "<=3 words", "cue": "word"}]},
    },
    "timeline": {
        "use": "2-5 dated events in order",
        "content": {"events": [{"date": "year/era", "text": "<=5 words", "icon": "optional", "cue": "word"}]},
    },
    "character": {
        "use": "a human moment: someone reacting, explaining, deciding (1-2 characters)",
        "content": {"characters": [{"look": "adult|kid|elder|robot", "pose": "stand|wave|point|cheer|think|shrug|present",
                                    "expression": "happy|neutral|sad|surprised|worried|angry|excited",
                                    "color": "accent|accent2|accent3", "speaking": True}],
                    "bubble": "optional speech bubble <=8 words", "prop": "optional icon next to them",
                    "beats": [{"cue": "word", "character": 0, "pose": "pose", "expression": "expression"}]},
    },
    "quote": {
        "use": "a memorable line shown big, revealed word-by-word as spoken",
        "content": {"text": "<=18 words, ideally words the narrator says", "author": "optional",
                    "highlight": "optional single word to colour"},
    },
    "cycle": {
        "use": "3-5 stages that loop (life cycles, feedback loops)",
        "content": {"items": [{"icon": "icon", "label": "<=2 words", "cue": "word"}], "center": "optional <=2 words"},
    },
}

# Diagram layouts (3Blue1Brown-style), long-form layouts (chapters, fun facts,
# true scale) and LLM-written custom scenes.
from .custom import CUSTOM_DOC, layout_custom  # noqa: E402
from .diagrams import DIAGRAM_DOCS, DIAGRAM_LAYOUTS  # noqa: E402
from .marvels import MARVEL_DOCS, MARVEL_LAYOUTS  # noqa: E402

LAYOUTS.update(DIAGRAM_LAYOUTS)
LAYOUTS.update(MARVEL_LAYOUTS)
LAYOUTS["custom"] = layout_custom
LAYOUT_DOCS.update(DIAGRAM_DOCS)
LAYOUT_DOCS.update(MARVEL_DOCS)
LAYOUT_DOCS["custom"] = CUSTOM_DOC

"""Long-form layouts — chapter cards, fun facts and true-scale comparisons.

These carry the channel's format (engineering marvels and how things work):
five-minute videos are split into chapters, each chapter lands at least one
"wow" fact, and sizes are shown at true proportion like an engineering
drawing. Like every layout they read `ctx.content`, schedule beats on
narration cues, tolerate missing input, and also work with no narration at
all (chapter cards are silent).
"""

from __future__ import annotations

import math

import numpy as np
from manim import (
    DOWN,
    LEFT,
    RIGHT,
    UP,
    Circle,
    Create,
    DashedLine,
    DoubleArrow,
    FadeIn,
    GrowFromCenter,
    GrowFromEdge,
    LaggedStart,
    Line,
    RoundedRectangle,
    VGroup,
    rate_functions,
)

from .director import Ctx
from .stage import fill, fit

POP = rate_functions.ease_out_back
EASE = rate_functions.ease_out_cubic


def _items(ctx: Ctx, key: str, hi: int) -> list[dict]:
    raw = ctx.content.get(key) or []
    out = [it if isinstance(it, dict) else {"label": str(it)} for it in (raw if isinstance(raw, list) else [])]
    return out[:hi]


def _num(v, default=0.0) -> float:
    try:
        return float(str(v).replace(",", "").strip())
    except (TypeError, ValueError):
        return default


def _halo(ctx: Ctx, m, scale=0.78):
    r = max(m.width, m.height) * scale
    col = ctx.theme.accent
    return VGroup(Circle(radius=r * 1.25, stroke_width=0, fill_color=col, fill_opacity=0.06),
                  Circle(radius=r, stroke_width=0, fill_color=col, fill_opacity=0.10),
                  Circle(radius=r, stroke_color=col, stroke_width=2.5, stroke_opacity=0.35)).move_to(m.get_center())


def _pill(ctx: Ctx, text: str, color=None, size=26):
    th = ctx.theme
    t = ctx.text(text, size=size, weight="BOLD", color=th.bg if th.dark else "#FFFFFF")
    box = RoundedRectangle(corner_radius=min(0.3, (t.height + 0.3) / 2), width=t.width + 0.6,
                           height=t.height + 0.32, stroke_width=0, fill_color=color or th.accent, fill_opacity=1)
    return VGroup(box, t.move_to(box))


def _burst(ctx: Ctx, m, color=None):
    def make():
        ring = Circle(radius=max(m.width, m.height) * 0.55, stroke_color=color or ctx.theme.accent,
                      stroke_width=6).move_to(m.get_center())
        ctx.scene.add(ring)
        return [ring.animate.scale(1.7).set_stroke(opacity=0)]
    return make


def _spaced(s: str) -> str:
    """Letter-spaced caps for kickers ('C H A P T E R · 2'). make_text collapses
    runs of spaces, so words are separated by a middle dot instead."""
    return " · ".join(" ".join(word) for word in s.upper().split())


def _word_lines(ctx: Ctx, text: str, size: float, max_chars: int, color=None, weight="SEMIBOLD"):
    """Left-aligned lines of text, plus one mobject per word (for word reveals)."""
    words = text.split() or ["…"]
    lines, cur = [], []
    for w in words:
        if cur and len(" ".join(cur + [w])) > max_chars:
            lines.append(cur)
            cur = []
        cur.append(w)
    lines.append(cur)
    word_mobs, block = [], VGroup()
    for ln in lines:
        line_txt = ctx.text(" ".join(ln), size=size, weight=weight, color=color)
        glyphs = list(line_txt.submobjects)
        g, k = VGroup(), 0
        for w in ln:
            n = len("".join(w.split()))
            g.add(VGroup(*glyphs[k:k + n]))
            k += n
        word_mobs.extend(g)
        block.add(g)
    block.arrange(DOWN, buff=0.28, aligned_edge=LEFT)
    return block, word_mobs, words


# ---------------------------------------------------------------------------
# chapter — silent chapter card between acts of a long video
# ---------------------------------------------------------------------------

def layout_chapter(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    num = int(max(_num(c.get("number"), 1), 1))
    total = int(min(max(_num(c.get("total"), 0), 0), 9))
    name = str(c.get("name") or ctx.s.get("title") or "")

    ghost = ctx.text(f"{num:02d}", size=420 if not st.portrait else 360, weight="BOLD", color=th.accent)
    ghost.set_opacity(0.07)
    kicker = ctx.text(_spaced(f"chapter {num}"), size=28, weight="BOLD", color=th.accent)
    head = ctx.text(name or " ", size=80 if not st.portrait else 66, weight="BOLD",
                    max_width=st.content_w, max_chars=20 if not st.portrait else 12)
    rule = Line(LEFT * 1.6, RIGHT * 1.6, stroke_color=th.accent, stroke_width=3)
    parts = VGroup(kicker, head, rule)
    dots = VGroup()
    if total > 1:
        for k in range(1, total + 1):
            if k < num:
                d = Circle(radius=0.075, stroke_width=0, fill_color=th.muted, fill_opacity=0.9)
            elif k == num:
                d = Circle(radius=0.11, stroke_width=0, fill_color=th.accent, fill_opacity=1)
            else:
                d = Circle(radius=0.075, stroke_color=th.muted, stroke_width=2, fill_opacity=0)
            dots.add(d)
        dots.arrange(RIGHT, buff=0.32)
        parts.add(dots)
    icon_grp = None
    if c.get("icon"):
        icon = ctx.icon(c["icon"], size=1.5 if not st.portrait else 1.8)
        icon_grp = VGroup(_halo(ctx, icon), icon)
        parts.submobjects.insert(0, icon_grp)
    parts.arrange(DOWN, buff=0.42)
    fit(parts, st.content_w, st.height - 2.6)
    center = np.array([0.0, 0.25 if not st.portrait else 0.6, 0.0])
    parts.move_to(center)
    ghost.move_to(head.get_center() + RIGHT * (0.5 if not st.portrait else 0.2))
    ctx.float(ghost, 0.05, 0.6)

    t = 0.15
    ctx.at(t, lambda: FadeIn(ghost, scale=1.15, rate_func=EASE), 1.6)
    if icon_grp is not None:
        ctx.at(t + 0.1, lambda: [GrowFromCenter(icon_grp[0]), FadeIn(icon_grp[1], scale=0.5, rate_func=POP)], 0.8)
        ctx.float(icon_grp, 0.06)
        t += 0.3
    ctx.at(t + 0.2, lambda: FadeIn(kicker, shift=DOWN * 0.2), 0.6)
    ctx.at(t + 0.45, lambda: LaggedStart(*[FadeIn(ch, shift=UP * 0.25) for ch in head.family_members_with_points()],
                                         lag_ratio=0.035), 1.2)
    ctx.at(t + 1.2, lambda: GrowFromCenter(rule), 0.6)
    if len(dots):
        cur = dots[num - 1]
        ctx.at(t + 1.5, lambda: LaggedStart(*[FadeIn(d, scale=0.4) for d in dots], lag_ratio=0.12), 0.8)
        ctx.at(t + 2.4, _burst(ctx, cur), 0.8)


# ---------------------------------------------------------------------------
# fun_fact — the "wait, really?" moment every chapter should land
# ---------------------------------------------------------------------------

def layout_fun_fact(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    kind = str(c.get("kind") or "fun fact").strip()[:24]
    fact = " ".join(str(c.get("fact") or ctx.s.get("title") or "").split())[:140]
    hl = {w.strip(".,!?;:\"'").lower() for w in str(c.get("highlight") or "").split() if w.strip()}

    icon = ctx.icon(c.get("icon") or "lightbulb", size=2.6 if not st.portrait else 2.3)
    halo = _halo(ctx, icon)
    hero = VGroup(halo, icon)
    badge = _pill(ctx, _spaced(kind), color=th.accent3, size=22)
    block, word_mobs, words = _word_lines(ctx, fact, size=50 if not st.portrait else 44,
                                          max_chars=22 if not st.portrait else 16)
    text_col = VGroup(badge, block).arrange(DOWN, buff=0.45, aligned_edge=LEFT)
    if st.portrait:
        whole = VGroup(hero, text_col).arrange(DOWN, buff=0.7)
    else:
        whole = VGroup(hero, text_col).arrange(RIGHT, buff=1.0)
    fill(whole, st.content_w, st.content_h, frac=0.92, cap=1.2)
    whole.move_to(st.content_center)
    hits = [wm for wm, w in zip(word_mobs, words) if w.strip(".,!?;:\"'").lower() in hl]
    underline = None
    if hits:
        grp = VGroup(*hits)
        underline = Line(grp.get_corner(DOWN + LEFT), grp.get_corner(DOWN + RIGHT), stroke_color=th.accent,
                         stroke_width=6).shift(DOWN * 0.12)

    ctx.at(0.2, lambda: [GrowFromCenter(halo), FadeIn(icon, scale=0.4, rate_func=POP)], 0.9)
    ctx.float(hero, 0.06)
    ctx.at(0.5, lambda: FadeIn(badge, shift=RIGHT * 0.3, scale=0.8, rate_func=POP), 0.6)
    start = max(ctx.narr_start, 0.8)
    end = min(ctx.narr_end, start + 0.32 * len(words) + 1.0)
    times = ctx.schedule(words, start=start, end=end, min_gap=0.1)
    for wm, t in zip(word_mobs, times):
        ctx.at(t, lambda wm=wm: FadeIn(wm, shift=UP * 0.15), 0.35)
    te = ctx.cue(c.get("highlight_cue") or c.get("highlight"), None, after=start)
    te = max(te if te is not None else times[-1] + 0.4, times[-1] + 0.3)
    if hits:
        ctx.at(te, lambda: [*(h.animate.set_color(th.accent) for h in hits), Create(underline)], 0.6)
    ctx.at(min(te + 0.5, ctx.content_end - 1.0), _burst(ctx, icon), 0.9)


# ---------------------------------------------------------------------------
# scale — sizes at true proportion, engineering-drawing style
# ---------------------------------------------------------------------------

def layout_scale(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    items = _items(ctx, "items", 4) or [{"label": "A", "value": 1}, {"label": "B", "value": 3}]
    n = len(items)
    vals = [max(_num(it.get("value"), 1.0), 0.0) for it in items]
    vmax = max(vals) or 1.0
    unit = str(c.get("unit", ""))
    cc = st.content_center
    W = min(st.content_w * (0.95 if st.portrait else 0.8), n * 3.0)
    ground_y = cc[1] - st.content_h / 2 + 0.75
    slot = W / n
    cw = min(1.3, slot * 0.42)
    x0 = cc[0] - W / 2
    cap_h = min(0.85, slot * 0.4)
    top_lim = (st.content_top if ctx.s.get("title") else st.height / 2 - 0.6) - 0.1
    low = [k for k, v in enumerate(vals) if v < 0.55 * vmax]
    # Room above the tallest column for its icon and value label (and the
    # callout, when no short column leaves space for it beside the tall ones).
    H = top_lim - ground_y - cap_h - 0.12 - 0.5 - (0.75 if c.get("callout") and not low else 0.0)
    H = max(H, 1.5)

    ground = Line([x0 - 0.3, ground_y, 0], [x0 + W + 0.3, ground_y, 0], stroke_color=th.muted, stroke_width=4)
    hatch = VGroup(*[Line([x, ground_y, 0], [x - 0.18, ground_y - 0.18, 0], stroke_color=th.muted,
                          stroke_width=2, stroke_opacity=0.6)
                     for x in np.arange(x0 - 0.1, x0 + W + 0.3, 0.32)])
    cols, caps, dims, labels = [], [], [], []
    for i, (it, v) in enumerate(zip(items, vals)):
        x = x0 + slot * (i + 0.5)
        h = max(H * v / vmax, 0.18)
        col_c = ctx.accent(i)
        body = RoundedRectangle(corner_radius=min(0.08, cw / 4), width=cw, height=h, stroke_color=col_c,
                                stroke_width=3, fill_color=col_c, fill_opacity=0.16)
        body.move_to([x, ground_y + h / 2, 0])
        cap = ctx.icon(it.get("icon"), size=cap_h, accent=i, label=it.get("label", ""))
        cap.next_to(body, UP, buff=0.12)
        ax = x + cw / 2 + 0.22
        arrow = DoubleArrow([ax, ground_y, 0], [ax, ground_y + h, 0], buff=0, stroke_width=2.5, color=th.muted,
                            tip_length=0.14, max_tip_length_to_length_ratio=0.35)
        disp = str(it.get("display") or f"{v:,.{0 if float(v).is_integer() else 1}f}{unit}")
        vt = ctx.text(disp, size=28, weight="BOLD", color=th.ink, max_width=slot * 0.9)
        vt.next_to(cap, UP, buff=0.1)
        lab = ctx.text(it.get("label", ""), size=24, color=th.muted, max_width=slot * 0.92, max_chars=14)
        lab.next_to([x, ground_y - 0.2, 0], DOWN, buff=0.12)
        cols.append(body), caps.append(cap), dims.append(VGroup(arrow, vt)), labels.append(lab)
    top = max(range(n), key=lambda k: vals[k])
    guide = DashedLine([x0 - 0.3, cols[top].get_top()[1], 0], [x0 + W + 0.3, cols[top].get_top()[1], 0],
                       stroke_color=th.accent, stroke_width=2, stroke_opacity=0.5, dash_length=0.12)

    ctx.at(0.2, lambda: [Create(ground), LaggedStart(*[Create(hm) for hm in hatch], lag_ratio=0.02),
                         LaggedStart(*[FadeIn(lb) for lb in labels], lag_ratio=0.1)], 0.9)
    times = ctx.schedule([str(it.get("cue") or it.get("label") or "") for it in items],
                         start=max(ctx.narr_start, 0.6), min_gap=0.8)
    for body, cap, dm, t in zip(cols, caps, dims, times):
        ctx.at(t, lambda body=body, cap=cap, dm=dm: [GrowFromEdge(body, DOWN, rate_func=EASE),
                                                     FadeIn(cap, shift=DOWN * 0.3), FadeIn(dm)], 1.0)
    tg = times[-1] + 0.9
    ctx.at(tg, lambda: Create(guide), 0.7)
    if c.get("callout"):
        pill = _pill(ctx, str(c["callout"])[:28], size=30)
        if low:
            # Over the short columns, just under the dashed line of the tallest.
            lx = sum(cols[k].get_x() for k in low) / len(low)
            above_labels = max(dims[k][1].get_top()[1] for k in low) + pill.height / 2 + 0.2
            pill.move_to([lx, max(cols[top].get_top()[1] - 0.55, above_labels), 0])
            pill.set_x(np.clip(pill.get_x(), x0 + pill.width / 2 - 0.3, x0 + W - pill.width / 2 + 0.3))
        else:
            pill.move_to([cc[0], max(vlabel.get_top()[1] for vlabel in (d[1] for d in dims)) + 0.5, 0])
        tc = ctx.cue(c.get("callout_cue"), None, after=times[-1] - 0.5)
        tc = max(tc if tc is not None else tg + 0.4, tg + 0.3)
        ctx.at(min(tc, ctx.content_end - 1.0), lambda: [FadeIn(pill, scale=0.6, rate_func=POP),
                                                        _burst(ctx, caps[top])()[0]], 0.8)


MARVEL_LAYOUTS = {
    "chapter": layout_chapter,
    "fun_fact": layout_fun_fact,
    "scale": layout_scale,
}

MARVEL_DOCS = {
    "chapter": {
        "use": "added automatically between chapters; do not use it yourself",
        "content": {"number": 2, "total": 5, "name": "<=4 words", "icon": "optional icon"},
    },
    "fun_fact": {
        "use": "a surprising fact, record or engineering marvel shown big, with a badge",
        "content": {"kind": "fun fact|engineering marvel|did you know|world record", "icon": "icon",
                    "fact": "<=14 words, close to the narration", "highlight": "1-2 words to colour",
                    "highlight_cue": "word"},
    },
    "scale": {
        "use": "2-4 sizes / lengths / heights compared at true proportion (engineering drawing)",
        "content": {"items": [{"label": "<=2 words", "value": 330, "display": "optional e.g. 330 m",
                               "icon": "icon", "cue": "word"}],
                    "unit": "optional suffix", "callout": "optional <=4 words, e.g. 3x taller",
                    "callout_cue": "word"},
    },
}

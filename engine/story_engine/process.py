"""Mechanism layouts: show HOW something works, step by step.

`process` draws a system from kit parts (tanks, valves, pumps, gears,
lasers, generated parts...) placed on a grid and joined by pipes, wires,
rays or fibres, then plays the storyboard's steps on their cue words:
valves open, flow starts, levels rise, rotors spin, the camera moves in on
the part being explained. Consecutive `process` scenes with
`"continue": true` form a *chain*: the same diagram carries on from where
the previous scene left off (the orchestrator passes the chain; earlier
steps are replayed instantly), so a whole chapter can build one machine.

`cross_section` cuts something open: concentric rings (a cable, a pipe, a
planet) or stacked layers (a road, a dam wall, a chip), each with a real
material texture, revealed layer by layer.
"""

from __future__ import annotations

import math
import re

import numpy as np
from manim import (
    DOWN,
    LEFT,
    RIGHT,
    UP,
    Annulus,
    AnimationGroup,
    Arrow,
    Circle,
    Create,
    DashedVMobject,
    Dot,
    FadeIn,
    FadeOut,
    GrowFromCenter,
    Indicate,
    LaggedStart,
    Line,
    Rectangle,
    RoundedRectangle,
    ShowPassingFlash,
    Transform,
    VGroup,
    rate_functions,
)

from .director import Ctx
from .kit import KitEnv, Link, build_part, polyline, route
from .kit.core import colour

EASE = rate_functions.ease_in_out_sine

# ---------------------------------------------------------------------------
# process
# ---------------------------------------------------------------------------

LINK_KINDS = ("pipe", "wire", "cable", "ray", "fibre", "fiber", "belt", "shaft", "arrow", "duct", "mesh")


def _as_list(v):
    if v is None:
        return []
    return list(v) if isinstance(v, (list, tuple)) else [v]


def _merge_chain(contents: list[dict]):
    """Union of parts and links across a chain, remembering where each first appears."""
    parts, part_intro, links, link_intro = {}, {}, {}, {}
    for j, c in enumerate(contents):
        for i, p in enumerate(_as_list(c.get("parts"))):
            if not isinstance(p, dict):
                continue
            pid = str(p.get("id") or f"p{j}_{i}")
            if pid in parts:
                parts[pid] = {**parts[pid], **{k: v for k, v in p.items() if k not in ("col", "row")}}
            else:
                parts[pid] = dict(p)
                part_intro[pid] = j
        for i, ln in enumerate(_as_list(c.get("links"))):
            if not isinstance(ln, dict) or not ln.get("from") or not ln.get("to"):
                continue
            lid = str(ln.get("id") or f"{ln['from']}->{ln['to']}")
            if lid not in links:
                links[lid] = dict(ln)
                link_intro[lid] = j
    return parts, part_intro, links, link_intro


def _grid(parts: dict) -> dict:
    """id -> (col, row). Missing positions flow left to right, wrapping after 5."""
    ids = list(parts)
    explicit = any("col" in parts[i] or "row" in parts[i] for i in ids)
    out = {}
    for n, pid in enumerate(ids):
        p = parts[pid]
        if explicit:
            col = float(p.get("col", n))
            row = float(p.get("row", 0))
        else:
            per = 5 if len(ids) > 5 else max(len(ids), 1)
            col, row = float(n % per), float(n // per)
        out[pid] = (col, row)
    return out


def _parse_action(a):
    """Accept {"on": "pump"}, {"level": ["tank", 0.8]}, {"level": {"tank": 0.8}} or "open valve"."""
    out = []
    if isinstance(a, str):
        bits = a.replace(":", " ").split()
        if bits:
            val = None
            if len(bits) > 2:
                try:
                    val = float(bits[2])
                except ValueError:
                    val = " ".join(bits[2:])
            out.append((bits[0].lower(), bits[1] if len(bits) > 1 else None, val))
        return out
    if not isinstance(a, dict):
        return out
    for verb, arg in a.items():
        verb = str(verb).lower()
        if isinstance(arg, dict):
            for target, val in arg.items():
                out.append((verb, str(target), val))
        elif isinstance(arg, (list, tuple)) and len(arg) == 2 and isinstance(arg[1], (int, float, str)) \
                and verb in ("level", "label", "fill", "drain", "speed"):
            out.append((verb, str(arg[0]), arg[1]))
        else:
            for target in _as_list(arg):
                out.append((verb, None if target is None else str(target), None))
    return out


def layout_process(ctx: Ctx):
    st, th = ctx.stage, ctx.theme
    chain = ctx.spec.get("chain") if isinstance(ctx.spec.get("chain"), dict) else None
    contents = chain.get("contents") if chain and isinstance(chain.get("contents"), list) else [ctx.content]
    k = int(chain.get("index", len(contents) - 1)) if chain else 0
    k = min(max(k, 0), len(contents) - 1)
    contents = [c if isinstance(c, dict) else {} for c in contents]
    parts_spec, part_intro, links_spec, link_intro = _merge_chain(contents)
    if not parts_spec:
        raise ValueError("process scene has no parts")
    env = KitEnv(ctx)

    # Floating things (a ship on a lock chamber) ride a container's water instead of
    # taking a grid cell; they are built after everything else so they draw on top.
    floating = {pid for pid, sp in parts_spec.items() if str(sp.get("floats_on", "")).strip() in parts_spec}
    parts_spec = {**{p: s for p, s in parts_spec.items() if p not in floating},
                  **{p: s for p, s in parts_spec.items() if p in floating}}

    # ---- place parts on a grid inside the content box
    grid = _grid({p: s for p, s in parts_spec.items() if p not in floating})
    cols = max(c for c, _ in grid.values()) + 1
    rows = max(r for _, r in grid.values()) + 1
    cc = st.content_center
    W, H = st.content_w * 0.96, st.content_h * 0.96
    cw, ch = W / cols, H / rows
    base = min(cw * 0.6, ch * 0.6, 2.6)
    parts, labels = {}, {}
    for pid, spec in parts_spec.items():
        if pid in floating:
            continue
        part = build_part(env, spec)
        m = part.mob
        size = base * min(max(float(spec.get("size", 1.0) or 1.0), 0.4), 2.2)
        s = min(size * 1.35 / max(m.width, 1e-3), size / max(m.height, 1e-3))
        m.scale(s)
        col, row = grid[pid]
        m.move_to([cc[0] - W / 2 + cw * (col + 0.5), cc[1] + H / 2 - ch * (row + 0.5), 0])
        parts[pid] = part
    # Grow the whole machine to fill the stage (grid cells leave a lot of air),
    # leaving room for labels below and a note above.
    machine = VGroup(*[p.mob for p in parts.values()])
    has_labels = any(sp.get("label") for sp in parts_spec.values())
    room_w = st.content_w * 0.92
    room_h = st.content_h * 0.92 - (0.5 if has_labels else 0.0) - 0.55
    grow = min(room_w / max(machine.width, 1e-3), room_h / max(machine.height, 1e-3), 2.2)
    machine.scale(grow)
    machine.move_to(cc + DOWN * (0.25 if has_labels else 0.0) + DOWN * 0.1)
    # Meshed gears must touch: slide each driven gear against its driver so the teeth interlock.
    for ln in links_spec.values():
        if str(ln.get("kind", "")).lower() != "mesh":
            continue
        a_id, b_id = str(ln["from"]).split(".")[0], str(ln["to"]).split(".")[0]
        if a_id in parts and b_id in parts:
            A, B = parts[a_id].mob, parts[b_id].mob
            d = B.get_center() - A.get_center()
            dist = float(np.linalg.norm(d)) or 1.0
            reach = (A.width + B.width) / 2 * 0.9
            B.move_to(A.get_center() + d / dist * reach)
    # Floating parts: sized to their container, riding its liquid surface.
    for pid in floating:
        spec = parts_spec[pid]
        host = parts[str(spec["floats_on"]).strip()]
        part = build_part(env, spec)
        m = part.mob
        m.scale_to_fit_width(host.mob.width * 0.62 * min(max(float(spec.get("size", 1.0) or 1.0), 0.4), 1.4))
        m.move_to(host.mob.get_center())
        if host.surface is not None:
            ride = getattr(part, "floats", 0.6)
            m.add_updater(lambda mob, host=host, ride=ride: mob.set_y(host.surface() + mob.height * (ride - 0.5)))
            m.set_y(host.surface() + m.height * (ride - 0.5))
        parts[pid] = part

    # Labels: sized to their part, nudged above when they would collide.
    placed: list = []
    for pid, spec in parts_spec.items():
        if not spec.get("label"):
            continue
        part = parts[pid]
        size = min(max(part.mob.width * 13, 17), 26)
        lab = ctx.text(str(spec["label"])[:28], size=size, color=th.ink,
                       max_width=max(part.mob.width * 1.5, 1.4), max_chars=16)
        above = pid in floating
        lab.next_to(part.mob, UP if above else DOWN, buff=0.12)
        others = [p.mob for q, p in parts.items() if q != pid and q not in floating]
        if not above and (_hits(lab, placed) or _hits(lab, others)):
            lab.next_to(part.mob, UP, buff=0.12)
            if _hits(lab, placed) or _hits(lab, others):
                lab.scale(0.8).next_to(part.mob, DOWN, buff=0.1)
                minor = str(spec.get("kind", "")).lower() in ("valve", "node", "switch", "gate", "tap", "junction")
                if minor and (_hits(lab, placed) or _hits(lab, others)):
                    continue  # a valve reads without its label; a collision doesn't
        edge = st.width / 2 - 0.2  # labels of the outermost parts stay inside the frame
        if lab.get_left()[0] < -edge:
            lab.shift(RIGHT * (-edge - lab.get_left()[0]))
        if lab.get_right()[0] > edge:
            lab.shift(LEFT * (lab.get_right()[0] - edge))
        if pid in floating:
            lab.add_updater(lambda m, mob=part.mob: m.next_to(mob, UP, buff=0.1))
        part.label = lab
        labels[pid] = lab
        placed.append(lab)

    # ---- links
    links, meshes, link_bodies = {}, [], {}
    for lid, ln in links_spec.items():
        a_id, _, a_port = str(ln["from"]).partition(".")
        b_id, _, b_port = str(ln["to"]).partition(".")
        if a_id not in parts or b_id not in parts:
            continue
        kind = str(ln.get("kind", "pipe")).lower()
        kind = kind if kind in LINK_KINDS else "pipe"
        if kind == "mesh":
            meshes.append((a_id, b_id))
            continue
        A, B = parts[a_id], parts[b_id]
        if not a_port or not b_port:
            d = B.mob.get_center() - A.mob.get_center()
            if abs(d[0]) >= abs(d[1]) * 0.6:
                a_auto, b_auto = ("out", "in") if d[0] > 0 else ("left", "right")
            else:
                a_auto, b_auto = ("bottom", "top") if d[1] < 0 else ("top", "bottom")
            a_port, b_port = a_port or a_auto, b_port or b_auto
        pts = route(A.port(a_port), B.port(b_port), a_port, b_port)
        path = polyline(pts)
        carries = str(ln.get("carries") or {"wire": "current", "cable": "current", "ray": "light", "fibre": "light",
                                             "fiber": "light", "duct": "air", "belt": "force", "shaft": "force"
                                             }.get(kind, "water"))
        body = _link_body(ctx, path, kind, pts)
        link = Link(path, body, carries, th, reverse=bool(ln.get("reverse")), zigzag=kind in ("fibre", "fiber"))
        if ln.get("label"):
            from manim import BackgroundRectangle
            lab = ctx.text(str(ln["label"])[:20], size=20, color=th.muted, max_width=2.6)
            lab.next_to(path.point_from_proportion(0.5), UP, buff=0.14)
            if not (_hits(lab, list(labels.values())) or _hits(lab, [p.mob for p in parts.values()])):
                body.add(BackgroundRectangle(lab, color=th.bg, fill_opacity=0.8, buff=0.05), lab)
        if ln.get("flow"):
            link.start(instant=True)
        links[lid] = link
        link_bodies[lid] = body

    # meshed gears follow their driver (opposite direction, by tooth ratio)
    for a_id, b_id in meshes:
        drv, fol = parts[a_id], parts[b_id]
        ratio = (drv.teeth or 12) / (fol.teeth or 12)
        fol.speed.add_updater(lambda m, drv=drv, ratio=ratio: m.set_value(-drv.speed.get_value() * ratio))
        ctx.add(fol.speed)

    # ---- what is already on screen (earlier scenes of the chain)
    hidden = {pid for pid, sp in parts_spec.items() if sp.get("hidden")}
    shown_parts = {pid for pid, j in part_intro.items() if j < k and pid not in hidden}
    for pid in shown_parts:
        ctx.add(parts[pid].mob, *([labels[pid]] if pid in labels else []))
    for lid, link in links.items():
        a_id, b_id = str(links_spec[lid]["from"]).split(".")[0], str(links_spec[lid]["to"]).split(".")[0]
        if link_intro[lid] < k and a_id in shown_parts and b_id in shown_parts:
            ctx.add(link.body, link.particles)
    focus_used = False
    state = {"shown_parts": set(shown_parts), "note": None}
    for j in range(k):
        for step in _as_list(contents[j].get("steps")):
            if isinstance(step, dict):
                for verb, target, val in [x for a in _as_list(step.get("do")) for x in _parse_action(a)]:
                    _apply(ctx, verb, target, val, parts, labels, links, links_spec, state, instant=True)

    # ---- layering: links under parts, labels and notes on top
    for lid, link in links.items():
        link.body.set_z_index(1)
        link.particles.set_z_index(2)
    for p in parts.values():
        p.mob.set_z_index(3)
    for lab in labels.values():
        lab.set_z_index(5)

    # ---- intro: parts new in this scene build in, links draw between them
    new_parts = sorted([pid for pid, j in part_intro.items() if j == k and pid not in hidden],
                       key=lambda p: p in floating)  # hosts before what floats on them
    new_links = [lid for lid, j in link_intro.items() if j == k or (
        j < k and lid in links and not any(m is links[lid].body for m in ctx.scene.mobjects))]

    # ---- camera: frame what is on screen. A diagram that grows across a chain
    # starts on the parts already there and pulls back as new ones arrive.
    frame = ctx.scene.camera.frame
    prior_ids = [p for p in shown_parts]
    end_ids = list(dict.fromkeys(prior_ids + new_parts))
    c_end, w_end = _view_for(ctx, _group_of(parts, end_ids), min_scale=0.5)
    c_start, w_start = _view_for(ctx, _group_of(parts, prior_ids), min_scale=0.5) if prior_ids else (c_end, w_end)
    steps_have_focus = any(v == "focus" for s in _as_list(ctx.content.get("steps")) if isinstance(s, dict)
                           for a in _as_list(s.get("do")) for v, _, _ in _parse_action(a))
    camera_moves = min(w_start, w_end) < ctx.stage.width * 0.98 or steps_have_focus
    if camera_moves:
        frame.clear_updaters()
        frame.move_to(c_start).set_width(w_start)
        if ctx.title_mob is not None:
            for m in ctx.title_mob:
                m.set_z_index(7)
                ctx.pin_to_frame(m)
        if abs(w_end - w_start) > 0.05 or np.linalg.norm(c_end - c_start) > 0.05:
            ctx.at(0.3, lambda: [frame.animate(rate_func=EASE).move_to(c_end).set_width(w_end)], 1.6)
    t_intro = 0.2
    if new_parts:
        span = min(1.6, 0.25 * len(new_parts) + 0.5)
        ctx.at(t_intro, lambda: LaggedStart(*[FadeIn(VGroup(parts[p].mob, *([labels[p]] if p in labels else [])),
                                                     scale=0.85) for p in new_parts], lag_ratio=0.25), span)
        state["shown_parts"].update(new_parts)
        t_intro += span * 0.7
    drawable = [lid for lid in new_links if lid in links and all(
        x.split(".")[0] in state["shown_parts"] for x in (str(links_spec[lid]["from"]), str(links_spec[lid]["to"])))]
    if drawable:
        def draw_links(ids=drawable):
            for lid in ids:
                ctx.scene.add(links[lid].particles)
            return [LaggedStart(*[Create(links[lid].body) for lid in ids], lag_ratio=0.2)]
        ctx.at(t_intro, draw_links, 0.9)
        t_intro += 0.8

    # ---- the steps of this scene
    steps = [s for s in _as_list(ctx.content.get("steps")) if isinstance(s, dict)]
    if not steps:
        # A silent beat or a scene with no steps: just run the machine.
        def run_all():
            anims = []
            for lid, link in links.items():
                anims += link.start()
            for p in parts.values():
                anims += p.act("on")
            return anims or [Indicate(VGroup(*[p.mob for p in parts.values()]), scale_factor=1.02)]
        ctx.at(max(t_intro, 0.8), run_all, 1.2)
        return
    times = ctx.schedule([str(s.get("cue") or "") for s in steps], start=max(ctx.narr_start + 0.2, t_intro),
                         min_gap=0.9)
    for step, t in zip(steps, times):
        acts = [x for a in _as_list(step.get("do")) for x in _parse_action(a)]
        if any(v == "focus" for v, _, _ in acts):
            focus_used = True
        rt = 2.0 if any(v in ("level", "fill", "drain", "focus") for v, _, _ in acts) else 1.0

        def make(acts=acts):
            anims = []
            for verb, target, val in acts:
                anims += _apply(ctx, verb, target, val, parts, labels, links, links_spec, state, instant=False)
            return anims or [Indicate(VGroup(*[p.mob for p in parts.values()]), scale_factor=1.0)]
        ctx.at(t, make, rt)
    if focus_used:
        ctx.at(max(times[-1] + 1.6, ctx.content_end - 1.6),
               lambda: _focus(ctx, None, parts, sorted(state["shown_parts"])), 1.0)


def _link_body(ctx: Ctx, path, kind: str, pts) -> VGroup:
    th = ctx.theme
    if kind in ("pipe", "duct"):
        w = 18 if kind == "duct" else 14
        return VGroup(path.copy().set_stroke(th.muted, width=w, opacity=0.9),
                      path.copy().set_stroke(th.bg, width=w - 6, opacity=1.0))
    if kind in ("fibre", "fiber"):
        return VGroup(path.copy().set_stroke(th.ink, width=16, opacity=0.5),
                      path.copy().set_stroke("#9ED8F0", width=10, opacity=0.25))
    if kind == "ray":
        return VGroup(path.copy().set_stroke(th.accent, width=3, opacity=0.55))
    if kind == "cable":
        return VGroup(path.copy().set_stroke(th.ink, width=7, opacity=0.9))
    if kind == "belt":
        return VGroup(DashedVMobject(path.copy().set_stroke(th.ink, width=4), num_dashes=24))
    if kind == "shaft":
        return VGroup(path.copy().set_stroke(th.muted, width=9, opacity=1.0))
    if kind == "arrow":
        body = VGroup(path.copy().set_stroke(th.muted, width=4))
        if len(pts) >= 2:
            body.add(Arrow(pts[-2], pts[-1], buff=0, stroke_width=4, color=th.muted,
                           max_tip_length_to_length_ratio=0.5, tip_length=0.22).set_opacity(1))
        return body
    return VGroup(path.copy().set_stroke(th.ink, width=3))  # wire


def _hits(m, others, margin: float = 0.06) -> bool:
    """Bounding-box overlap between `m` and any of `others`."""
    l, r, b, t = m.get_left()[0], m.get_right()[0], m.get_bottom()[1], m.get_top()[1]
    for o in others:
        if o is m:
            continue
        if (l < o.get_right()[0] - margin and r > o.get_left()[0] + margin
                and b < o.get_top()[1] - margin and t > o.get_bottom()[1] + margin):
            return True
    return False


def _view_for(ctx: Ctx, mobs, min_scale: float = 0.62, pad: float = 1.12):
    """Camera (centre, width) that fits `mobs` into the content area of the
    view, so the pinned heading and the caption band stay clear."""
    st = ctx.stage
    mobs = [m for m in mobs if m is not None]
    if not mobs:
        return np.array([0.0, 0.0, 0.0]), st.width
    grp = VGroup(*mobs)
    box_h = st.content_h - NOTE_BAND  # the note strip under the heading stays clear
    box_c = st.content_center + DOWN * NOTE_BAND / 2
    s = max(grp.width * pad / st.content_w, grp.height * pad / box_h)
    s = min(max(s, min_scale), 1.0)
    w, h = st.width * s, st.height * s
    c = grp.get_center() - box_c * s
    cx = float(np.clip(c[0], -(st.width - w) / 2, (st.width - w) / 2))
    cy = float(np.clip(c[1], -(st.height - h) / 2, (st.height - h) / 2))
    return np.array([cx, cy, 0.0]), w


def _group_of(parts, ids):
    return [m for i in ids if i in parts for m in (parts[i].mob, parts[i].label)]


def _focus(ctx: Ctx, target, parts, visible=None):
    frame = ctx.scene.camera.frame
    head = ctx.title_mob
    if target is None or str(target).lower() in ("all", "none", "reset", "out"):
        c, w = _view_for(ctx, _group_of(parts, visible if visible else list(parts)), min_scale=0.5)
        back = [head.animate.set_opacity(1)] if head is not None else []
        return [frame.animate(rate_func=EASE).move_to(c).set_width(w), *back]
    ids = [t for t in re.split(r"[,\s]+", str(target)) if t in parts]
    if not ids:
        return []
    # Moderate zoom: the part plus some context, labels readable. Neighbouring
    # parts slide up under the heading in a close-up, so the heading steps aside.
    c, w = _view_for(ctx, _group_of(parts, ids), min_scale=0.62, pad=1.9)
    away = [head.animate.set_opacity(0)] if head is not None else []
    return [frame.animate(rate_func=EASE).move_to(c).set_width(w), *away]


def _apply(ctx, verb, target, val, parts, labels, links, links_spec, state, instant):
    """One step action -> animations (or instant state change)."""
    th = ctx.theme
    if verb == "focus":
        return [] if instant else _focus(ctx, target, parts, sorted(state["shown_parts"]))
    if verb == "note":
        text = str(val if val is not None else target or "")[:40]
        if not text:
            return []
        pill = _note(ctx, text)
        old, state["note"] = state["note"], pill
        if instant:
            if old is not None:
                ctx.scene.remove(old)
            ctx.add(pill)
            return []
        return [*( [FadeOut(old)] if old is not None else []), FadeIn(pill, scale=0.8)]
    ids = [t for t in re.split(r"[,\s]+", str(target or "")) if t] or (["*"] if verb in ("on", "off") else [])
    if ids == ["*"]:
        ids = list(parts) + list(links)
    anims = []
    for tid in ids:
        if tid in links:
            link = links[tid]
            if verb in ("on", "start", "flow", "run", "open"):
                anims += link.start(val if isinstance(val, (int, float)) else None, instant=instant)
            elif verb in ("off", "stop", "close"):
                anims += link.stop(instant=instant)
            elif verb == "reverse":
                link.reverse = not link.reverse
            elif verb == "highlight" and not instant:
                anims.append(ShowPassingFlash(link.path.copy().set_stroke(th.accent, width=8), time_width=0.6))
            elif verb == "show":
                if instant:
                    ctx.add(link.body, link.particles)
                else:
                    ctx.scene.add(link.particles)
                    anims.append(Create(link.body))
            continue
        if tid not in parts:
            continue
        part = parts[tid]
        lab = labels.get(tid)
        grp = VGroup(part.mob, *([lab] if lab is not None else []))
        if verb == "show":
            state["shown_parts"].add(tid)
            if instant:
                ctx.add(grp)
            else:
                anims.append(FadeIn(grp, scale=0.85))
        elif verb == "hide":
            if instant:
                ctx.scene.remove(grp)
            else:
                anims.append(FadeOut(grp))
        elif verb in ("dim", "undim"):
            op = 0.3 if verb == "dim" else 1.0
            if instant:
                grp.set_opacity(op)
            else:
                anims.append(grp.animate.set_opacity(op))
        elif verb == "label" and val is not None:
            new = ctx.text(str(val)[:28], size=24, color=th.accent, max_width=3.2)
            if lab is not None:
                new.move_to(lab)
                if instant:
                    lab.become(new)
                else:
                    anims.append(Transform(lab, new))
            else:
                new.next_to(part.mob, DOWN, buff=0.12)
                labels[tid] = new
                if instant:
                    ctx.add(new)
                else:
                    anims.append(FadeIn(new))
        elif verb == "highlight" and "highlight" not in part.actions:
            if not instant:
                anims.append(Indicate(part.mob, color=th.accent, scale_factor=1.06))
        else:
            anims += part.act(verb, val, instant=instant)
    return anims


NOTE_BAND = 0.62  # strip kept free under the heading for step notes


def _note(ctx: Ctx, text: str):
    """A callout pill in the strip under the heading, pinned to the camera."""
    th = ctx.theme
    t = ctx.text(text, size=26, weight="BOLD", color=th.bg if th.dark else "#FFFFFF")
    box = RoundedRectangle(corner_radius=0.22, width=t.width + 0.5, height=t.height + 0.3, stroke_width=0,
                           fill_color=th.accent, fill_opacity=0.95)
    pill = VGroup(box, t.move_to(box))
    top = ctx.stage.content_top if ctx.s.get("title") else ctx.stage.height / 2 - 0.5
    pill.move_to([0, top - NOTE_BAND / 2 + 0.05, 0])
    pill.set_z_index(6)
    ctx.pin_to_frame(pill)
    return pill


# ---------------------------------------------------------------------------
# cross_section
# ---------------------------------------------------------------------------

MATERIALS = {
    # name: (fill colour, opacity, texture)
    "glass": ("#9ED8F0", 0.35, "shine"), "core": ("accent", 0.85, "shine"), "fibre": ("#9ED8F0", 0.45, "shine"),
    "fiber": ("#9ED8F0", 0.45, "shine"), "steel": ("#90A4AE", 0.75, "hatch"),
    "steel_wires": ("#90A4AE", 0.2, "wires"), "armour": ("#90A4AE", 0.2, "wires"), "armor": ("#90A4AE", 0.2, "wires"),
    "copper": ("#E0884A", 0.85, "none"), "aluminium": ("#CFD8DC", 0.7, "none"), "aluminum": ("#CFD8DC", 0.7, "none"),
    "gold": ("#FFC93C", 0.85, "none"), "plastic": ("#455A64", 0.85, "none"), "polyethylene": ("#37474F", 0.9, "none"),
    "rubber": ("#263238", 0.95, "none"), "insulation": ("#FFD54F", 0.55, "dots"), "gel": ("#FFF59D", 0.3, "dots"),
    "jelly": ("#FFF59D", 0.3, "dots"), "water": ("#4FC3F7", 0.55, "none"), "air": ("muted", 0.0, "none"),
    "soil": ("#8D6E63", 0.75, "dots"), "sand": ("#D7CCA1", 0.7, "dots"), "gravel": ("#9E9E9E", 0.6, "dots"),
    "concrete": ("#B0BEC5", 0.6, "dots"), "asphalt": ("#37474F", 0.95, "dots"), "rock": ("#616161", 0.8, "hatch"),
    "clay": ("#A1887F", 0.75, "dots"), "silicon": ("#7E8FA6", 0.8, "none"), "oxide": ("#C5CAE9", 0.5, "none"),
    "magma": ("#FF7043", 0.85, "dots"), "iron": ("#B0BEC5", 0.8, "hatch"), "wood": ("#A1887F", 0.75, "hatch"),
    "foam": ("#ECEFF1", 0.35, "dots"), "tar": ("#212121", 0.95, "none"),
}


def _material(ctx: Ctx, name, i: int):
    th = ctx.theme
    key = str(name or "").strip().lower().replace(" ", "_")
    if key in MATERIALS:
        col, op, tex = MATERIALS[key]
        return colour(th, col, col if col.startswith("#") else th.accent), op, tex
    return th.accents[i % 3], 0.4, "none"


def layout_cross_section(ctx: Ctx):
    st, th, c = ctx.stage, ctx.theme, ctx.content
    layers = [ly if isinstance(ly, dict) else {"label": str(ly)} for ly in _as_list(c.get("layers"))][:8]
    if not layers:
        raise ValueError("cross_section has no layers")
    shape = str(c.get("shape", "rings")).lower()
    cc = st.content_center
    thick = [max(float(ly.get("thickness", 1) or 1), 0.15) for ly in layers]
    total = sum(thick)
    bands, textures, anchors = [], [], []
    label_col_x = cc[0] + (st.content_w * 0.08 if not st.portrait else 0)
    if shape == "stack":
        Wb = st.content_w * (0.46 if not st.portrait else 0.8)
        Hb = st.content_h * 0.86
        x0 = cc[0] - st.content_w / 2 + 0.3 + Wb / 2 if not st.portrait else cc[0]
        y = cc[1] + Hb / 2
        for i, (ly, t) in enumerate(zip(layers, thick)):  # listed top first
            h = Hb * t / total
            col, op, tex = _material(ctx, ly.get("material"), i)
            r = Rectangle(width=Wb, height=h, stroke_color=th.ink, stroke_width=2, fill_color=col, fill_opacity=op)
            r.move_to([x0, y - h / 2, 0])
            bands.append(r)
            textures.append(_texture_rect(ctx, r, tex, col))
            anchors.append(r.get_right())
            y -= h
        build_order = list(range(len(layers)))[::-1]  # foundations first
    else:
        R = min(st.content_h * 0.46, st.content_w * 0.26)
        center = np.array([cc[0] - st.content_w * 0.2, cc[1], 0]) if not st.portrait else cc + UP * 1.2
        r_in = 0.0
        for i, (ly, t) in enumerate(zip(layers, thick)):  # listed inside first
            r_out = R * sum(thick[:i + 1]) / total
            col, op, tex = _material(ctx, ly.get("material"), i)
            if r_in <= 1e-3:
                band = Circle(radius=r_out, stroke_color=th.ink, stroke_width=2, fill_color=col, fill_opacity=op)
            else:
                band = Annulus(inner_radius=r_in, outer_radius=r_out, stroke_color=th.ink, stroke_width=2,
                               fill_color=col, fill_opacity=op)
            band.move_to(center)
            bands.append(band)
            textures.append(_texture_ring(ctx, center, r_in, r_out, tex, col))
            a = math.radians(55 - 110 * i / max(len(layers) - 1, 1))
            mid = (r_in + r_out) / 2
            anchors.append(center + mid * np.array([math.cos(a), math.sin(a), 0]))
            r_in = r_out
        build_order = list(range(len(layers)))

    # labels in a column on the right, leader lines to each layer
    lab_x = (max(b.get_right()[0] for b in bands) + 0.9) if not st.portrait else None
    n = len(layers)
    top_y = cc[1] + st.content_h * 0.38
    gap = (st.content_h * 0.76) / max(n - 1, 1)
    labels, leaders = [], []
    for i, ly in enumerate(layers):
        txt = ctx.text(str(ly.get("label", ""))[:26], size=28, weight="BOLD", color=th.ink, max_width=4.2)
        note = ly.get("note")
        grp = VGroup(txt)
        if note:
            grp.add(ctx.text(str(note)[:34], size=20, color=th.muted, max_width=4.2))
            grp.arrange(DOWN, aligned_edge=LEFT, buff=0.06)
        if st.portrait:
            grp.move_to([cc[0], cc[1] - st.content_h * 0.1 - i * 0.55, 0])
        else:
            y = top_y - i * gap if shape != "stack" else anchors[i][1]
            grp.move_to([lab_x, y, 0], aligned_edge=LEFT)
            if grp.get_right()[0] > st.width / 2 - 0.3:
                grp.scale_to_fit_width(st.width / 2 - 0.3 - lab_x)
                grp.move_to([lab_x, y, 0], aligned_edge=LEFT)
        dot = Dot(anchors[i], radius=0.05, color=th.ink)
        end = grp.get_left() + LEFT * 0.12
        leader = VGroup(Line(anchors[i], end, stroke_color=th.muted, stroke_width=2), dot)
        labels.append(grp)
        leaders.append(leader)

    times = ctx.schedule([str(layers[i].get("cue") or layers[i].get("label") or "") for i in build_order],
                         start=max(ctx.narr_start, 0.4), min_gap=0.6)
    for i, t in zip(build_order, times):
        grow = GrowFromCenter(VGroup(bands[i], textures[i])) if shape != "stack" else \
            FadeIn(VGroup(bands[i], textures[i]), shift=DOWN * 0.4)
        ctx.at(t, lambda grow=grow, i=i: [grow, Create(leaders[i]), FadeIn(labels[i], shift=LEFT * 0.2)], 0.8)
    hl = c.get("highlight")
    if isinstance(hl, int) and 0 <= hl < n:
        th_t = ctx.cue(c.get("highlight_cue"), None, after=times[-1] - 0.5)
        th_t = max(th_t if th_t is not None else times[-1] + 1.0, times[-1] + 0.4)

        def emph():
            others = [VGroup(bands[j], textures[j], labels[j], leaders[j]) for j in range(n) if j != hl]
            return [*(o.animate.set_opacity(0.3) for o in others), Indicate(labels[hl], color=th.accent),
                    bands[hl].animate.set_stroke(th.accent, width=5)]
        ctx.at(min(th_t, ctx.content_end - 1.0), emph, 0.9)


def _texture_ring(ctx: Ctx, center, r_in, r_out, tex, col):
    th = ctx.theme
    g = VGroup()
    if r_out - r_in < 0.05:
        return g
    mid, w = (r_in + r_out) / 2, r_out - r_in
    if tex == "wires":
        rr = w * 0.42
        n = max(6, int(2 * math.pi * mid / (rr * 2.15)))
        for j in range(n):
            a = 2 * math.pi * j / n
            g.add(Circle(radius=rr, stroke_color=th.ink, stroke_width=1.5, fill_color=col, fill_opacity=0.85)
                  .move_to(center + mid * np.array([math.cos(a), math.sin(a), 0])))
    elif tex == "dots":
        area = math.pi * (r_out ** 2 - r_in ** 2)
        for _ in range(min(int(area * 30), 220)):
            a = ctx.rng.uniform(0, 2 * math.pi)
            r = math.sqrt(ctx.rng.uniform(r_in ** 2, r_out ** 2))
            g.add(Dot(center + r * np.array([math.cos(a), math.sin(a), 0]), radius=0.018, color=th.ink)
                  .set_opacity(0.35))
    elif tex == "hatch":
        n = max(12, int(2 * math.pi * mid / 0.18))
        for j in range(n):
            a = 2 * math.pi * j / n
            d = np.array([math.cos(a), math.sin(a), 0])
            g.add(Line(center + d * (r_in + w * 0.2), center + d * (r_out - w * 0.2), stroke_color=th.ink,
                       stroke_width=1.2, stroke_opacity=0.45))
    elif tex == "shine":
        from manim import Arc
        g.add(Arc(radius=max(r_in + w * 0.75, 0.02), start_angle=0.5, angle=1.0, stroke_color="#FFFFFF",
                  stroke_width=3, stroke_opacity=0.45).move_arc_center_to(center))
    return g


def _texture_rect(ctx: Ctx, r: Rectangle, tex, col):
    th = ctx.theme
    g = VGroup()
    x0, x1 = r.get_left()[0], r.get_right()[0]
    y0, y1 = r.get_bottom()[1], r.get_top()[1]
    if tex == "dots":
        for _ in range(min(int((x1 - x0) * (y1 - y0) * 30), 260)):
            g.add(Dot([ctx.rng.uniform(x0 + 0.05, x1 - 0.05), ctx.rng.uniform(y0 + 0.03, y1 - 0.03), 0],
                      radius=0.02, color=th.ink).set_opacity(0.35))
    elif tex == "hatch":
        step = 0.22
        x = x0 - (y1 - y0)
        while x < x1:
            a = np.array([max(x, x0), y0 + max(x0 - x, 0), 0])
            b_x = min(x + (y1 - y0), x1)
            b = np.array([b_x, y0 + (b_x - x), 0])
            if b[0] > a[0]:
                g.add(Line(a, b, stroke_color=th.ink, stroke_width=1.2, stroke_opacity=0.45))
            x += step
    elif tex == "wires":
        rr = min((y1 - y0) * 0.4, 0.14)
        n = max(3, int((x1 - x0) / (rr * 2.2)))
        for j in range(n):
            g.add(Circle(radius=rr, stroke_color=th.ink, stroke_width=1.5, fill_color=col, fill_opacity=0.85)
                  .move_to([x0 + (j + 0.5) * (x1 - x0) / n, (y0 + y1) / 2, 0]))
    return g


PROCESS_LAYOUTS = {"process": layout_process, "cross_section": layout_cross_section}

PROCESS_DOCS = {
    "process": {
        "use": "THE MAIN EXPLAINER: a working schematic of the system built from kit parts, joined by pipes/wires/"
               "rays/fibres, with steps that run it on cue (valves open, flow starts, levels rise, rotors spin, "
               "the camera moves in). Use \"continue\": true on the next process scene to keep building the same "
               "diagram (same ids) instead of starting over.",
        "content": {
            "continue": "optional true = carry on the previous process scene's diagram",
            "parts": [{"id": "short_id", "kind": "<kit part name>", "label": "<=3 words", "col": 0, "row": 0,
                       "size": "optional 0.5-2", "hidden": "optional true = appears later via show",
                       "make": "ONLY for a kind not in the kit: one line describing the drawing (it will be "
                               "built and kept for future videos)", "...": "part fields (level, teeth, open...)"}],
            "links": [{"from": "id or id.port", "to": "id or id.port",
                       "kind": "pipe|wire|cable|ray|fibre|belt|shaft|arrow|duct|mesh",
                       "carries": "water|oil|air|gas|steam|heat|current|data|light|signal|force|fuel|blood",
                       "id": "optional, to target it in steps", "label": "optional", "flow": "optional true"}],
            "steps": [{"cue": "narration word", "do": [{"open": "valve1"}, {"on": "pipe_id"},
                                                       {"level": ["tank2", 0.9]}, {"focus": "valve1"},
                                                       {"note": "<=4 words"}]}],
        },
        "actions": "on/off (parts spin or run, links flow), open/close, level [id, 0-1], fill, drain, show, hide, "
                   "highlight, dim, undim, label [id, text], focus id|all (camera), note text, reverse (link)",
    },
    "cross_section": {
        "use": "cut something open to show its layers: shape rings (cable, pipe, planet, tree) or stack (road, "
               "dam, chip, soil). Layers listed inside-out (rings) or top-down (stack).",
        "content": {"shape": "rings|stack",
                    "layers": [{"label": "<=3 words", "material": "glass|core|steel|steel_wires|copper|plastic|"
                                "polyethylene|rubber|insulation|gel|water|air|soil|sand|gravel|concrete|asphalt|"
                                "rock|clay|silicon|oxide|iron|wood|foam|aluminium|gold|tar",
                                "thickness": 1, "note": "optional <=5 words", "cue": "word"}],
                    "highlight": "optional layer index", "highlight_cue": "word"},
    },
}

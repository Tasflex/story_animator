# Kit part "pallet_lever", generated for "How a mechanical watch keeps perfect time with a spring and a wobbling wheel" and kept for reuse.
# Validated with `python -m story_engine validate-part`. Edit freely; keep PART and build(k, spec).
PART = {
    "name": "pallet_lever",
    "use": "Pivoted rocking escapement lever with synthetic ruby pallet jewels and fork; actions: on, off, highlight",
    "actions": ["on", "off", "highlight"]
}

def build(k, spec):
    th = k.theme
    accent_col = spec.get("color", th.accent)

    frame = Rectangle(width=2.2, height=2.2, stroke_width=0, fill_opacity=0)
    pivot_ref = Dot(P(0, -0.05), radius=0.01, fill_opacity=0)

    phase = {"t": 0.0}
    hl_tracker = ValueTracker(0.0)

    part = Part(
        VGroup(frame, pivot_ref),
        ports={
            "in": (-0.45, 0),
            "out": (0.45, 0),
            "pivot": (0, -0.05),
            "top": (0, 0.45),
            "bottom": (0, -0.45)
        }
    )

    def draw_lever():
        c = pivot_ref.get_center()
        s = frame.height / 2.2

        raw_wave = math.sin(phase["t"] * 7.0)
        clamped_wave = math.tanh(raw_wave * 2.5)
        angle = clamped_wave * (11.0 * math.pi / 180.0)

        g = VGroup()

        # Banking pins (limit stops)
        pin_l = Circle(radius=0.04 * s, stroke_color=th.ink, stroke_width=2, fill_color=th.muted, fill_opacity=0.8)
        pin_l.move_to(c + P(-0.32 * s, -0.68 * s))
        pin_r = Circle(radius=0.04 * s, stroke_color=th.ink, stroke_width=2, fill_color=th.muted, fill_opacity=0.8)
        pin_r.move_to(c + P(0.32 * s, -0.68 * s))
        g.add(pin_l, pin_r)

        # Rocking lever assembly constructed relative to local pivot origin P(0, 0)
        lever = VGroup()

        # 1. Central boss & arbor
        boss_outer = Circle(radius=0.20, stroke_color=th.ink, stroke_width=4, fill_color=th.bg2, fill_opacity=0.95)
        boss_inner = Circle(radius=0.07, stroke_color=th.ink, stroke_width=2.5, fill_color=th.ink, fill_opacity=1.0)

        # 2. Entry & Exit pallet arms
        arm_entry = Polygon(
            P(-0.10, 0.12), P(-0.62, 0.46), P(-0.52, 0.60), P(-0.04, 0.18),
            stroke_color=th.ink, stroke_width=3.5, fill_color=th.muted, fill_opacity=0.35
        )
        arm_exit = Polygon(
            P(0.10, 0.12), P(0.62, 0.46), P(0.52, 0.60), P(0.04, 0.18),
            stroke_color=th.ink, stroke_width=3.5, fill_color=th.muted, fill_opacity=0.35
        )

        # 3. Synthetic ruby jewels (escapement pallet stones)
        ruby_col = "#E91E63"
        jewel_entry = Polygon(
            P(-0.66, 0.40), P(-0.50, 0.52), P(-0.44, 0.44), P(-0.60, 0.32),
            stroke_color=th.ink, stroke_width=2.5, fill_color=ruby_col, fill_opacity=0.85
        )
        jewel_exit = Polygon(
            P(0.66, 0.40), P(0.50, 0.52), P(0.44, 0.44), P(0.60, 0.32),
            stroke_color=th.ink, stroke_width=2.5, fill_color=ruby_col, fill_opacity=0.85
        )

        # 4. Lever tail stem
        stem = Polygon(
            P(-0.07, -0.12), P(-0.05, -0.62), P(0.05, -0.62), P(0.07, -0.12),
            stroke_color=th.ink, stroke_width=3.5, fill_color=th.muted, fill_opacity=0.35
        )

        # 5. Lever fork horns & slot
        fork = VMobject()
        fork.set_points_as_corners([
            P(-0.16, -0.82), P(-0.14, -0.62), P(-0.05, -0.62),
            P(-0.05, -0.72), P(0.05, -0.72), P(0.05, -0.62),
            P(0.14, -0.62), P(0.16, -0.82)
        ])
        fork.set_stroke(color=th.ink, width=3.5)
        fork.set_fill(color=th.muted, opacity=0.35)

        # 6. Guard pin (safety dart)
        dart = Line(P(0, -0.62), P(0, -0.86), stroke_color=th.ink, stroke_width=2.5)

        lever.add(stem, arm_entry, arm_exit, boss_outer, boss_inner, jewel_entry, jewel_exit, fork, dart)

        # Active highlight glow on jewels & pivot center
        hl_val = hl_tracker.get_value()
        if hl_val > 0.01:
            hl_entry = jewel_entry.copy()
            hl_entry.set_stroke(color=accent_col, width=4 + 3 * hl_val, opacity=hl_val)
            hl_entry.set_fill(color=accent_col, opacity=0.5 * hl_val)

            hl_exit = jewel_exit.copy()
            hl_exit.set_stroke(color=accent_col, width=4 + 3 * hl_val, opacity=hl_val)
            hl_exit.set_fill(color=accent_col, opacity=0.5 * hl_val)

            hl_boss = Circle(radius=0.24, stroke_color=accent_col, stroke_width=3 * hl_val, stroke_opacity=hl_val, fill_opacity=0)
            lever.add(hl_entry, hl_exit, hl_boss)

        lever.scale(s)
        lever.rotate(angle, about_point=P(0, 0))
        lever.shift(c)

        g.add(lever)
        return g

    lever_mob = always_redraw(draw_lever)
    part.mob.add(lever_mob)

    def tick(m, dt):
        sp = part.speed.get_value()
        phase["t"] = phase["t"] + dt * sp

    clock = VMobject()
    clock.add_updater(tick)
    part.mob.add(clock)

    part.motor_actions()

    def do_highlight(val=True, instant=False):
        target = 1.0 if val else 0.0
        if instant:
            hl_tracker.set_value(target)
            return []
        return [hl_tracker.animate.set_value(target)]

    part.actions["highlight"] = do_highlight

    return part

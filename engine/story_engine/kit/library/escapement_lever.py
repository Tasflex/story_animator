# Kit part "escapement_lever", generated for "How a mechanical watch keeps perfect time with a spring and a wobbling wheel" and kept for reuse.
# Validated with `python -m story_engine validate-part`. Edit freely; keep PART and build(k, spec).
PART = {
    "name": "escapement_lever",
    "use": "pivoting anchor lever with synthetic ruby pallet jewels for mechanical watch escapements; actions: on, off, highlight",
    "actions": ["on", "off", "highlight"],
}


def build(k, spec):
    th = k.theme
    ruby_color = spec.get("jewel_color", "#E91E63")
    ruby_stroke = spec.get("jewel_stroke", "#880E4F")

    frame = Rectangle(width=2.4, height=2.4, stroke_width=0, fill_opacity=0)

    phase = {"x": 0.0}
    hl_tracker = ValueTracker(0.0)

    part = Part(
        VGroup(frame),
        ports={
            "center": (0, 0),
            "pivot": (0, 0),
            "entry": (-0.25, 0.2),
            "exit": (0.25, 0.2),
            "fork": (0, -0.35),
            "in": (-0.5, 0),
            "out": (0.5, 0),
            "top": (0, 0.5),
            "bottom": (0, -0.5),
        },
    )

    def redraw_components():
        c = frame.get_center()
        s = frame.height / 2.4
        hl = hl_tracker.get_value()

        # Fixed banking pins showing oscillating limits
        bp_l = Dot(
            c + P(-0.30 * s, -0.65 * s),
            radius=0.05 * s,
            color=th.muted,
        )
        bp_r = Dot(
            c + P(0.30 * s, -0.65 * s),
            radius=0.05 * s,
            color=th.muted,
        )

        # Oscillating angle driven by phase
        ang = 0.18 * math.sin(phase["x"])

        # Unrotated lever constructed relative to origin (0,0)
        hub = Circle(
            radius=0.14 * s,
            stroke_color=th.ink,
            stroke_width=4,
            fill_color=th.bg,
            fill_opacity=1.0,
        )
        pivot_dot = Dot(P(0, 0), radius=0.04 * s, color=th.ink)

        # Main arms extending to pallet stones
        arm_l = Line(
            P(0, 0), P(-0.55 * s, 0.32 * s), stroke_color=th.ink, stroke_width=4
        )
        arm_r = Line(
            P(0, 0), P(0.55 * s, 0.32 * s), stroke_color=th.ink, stroke_width=4
        )

        # Synthetic ruby pallet jewels
        jewel_l = RoundedRectangle(
            corner_radius=0.02 * s,
            width=0.22 * s,
            height=0.12 * s,
            stroke_color=ruby_stroke,
            stroke_width=2,
            fill_color=ruby_color,
            fill_opacity=0.9,
        )
        jewel_l.rotate(0.38).move_to(P(-0.60 * s, 0.35 * s))

        jewel_r = RoundedRectangle(
            corner_radius=0.02 * s,
            width=0.22 * s,
            height=0.12 * s,
            stroke_color=ruby_stroke,
            stroke_width=2,
            fill_color=ruby_color,
            fill_opacity=0.9,
        )
        jewel_r.rotate(-0.38).move_to(P(0.60 * s, 0.35 * s))

        # Fork stem and horns at impulse end
        stem = Line(
            P(0, 0), P(0, -0.68 * s), stroke_color=th.ink, stroke_width=4
        )
        horn_l = Arc(
            radius=0.15 * s,
            start_angle=math.pi * 0.7,
            angle=-math.pi * 0.6,
            stroke_color=th.ink,
            stroke_width=3.5,
        ).move_to(P(-0.08 * s, -0.76 * s))
        horn_r = Arc(
            radius=0.15 * s,
            start_angle=math.pi * 0.3,
            angle=math.pi * 0.6,
            stroke_color=th.ink,
            stroke_width=3.5,
        ).move_to(P(0.08 * s, -0.76 * s))
        guard_pin = Line(
            P(0, -0.68 * s),
            P(0, -0.84 * s),
            stroke_color=th.accent,
            stroke_width=3,
        )

        lever_body = VGroup(
            arm_l,
            arm_r,
            stem,
            jewel_l,
            jewel_r,
            horn_l,
            horn_r,
            guard_pin,
            hub,
            pivot_dot,
        )

        # Highlight glow effect around pallet jewels
        if hl > 0.01:
            glow_l = (
                jewel_l.copy()
                .set_stroke(color=th.accent, width=2 + 6 * hl, opacity=hl)
                .set_fill(opacity=0)
            )
            glow_r = (
                jewel_r.copy()
                .set_stroke(color=th.accent, width=2 + 6 * hl, opacity=hl)
                .set_fill(opacity=0)
            )
            lever_body.add(glow_l, glow_r)

        lever_body.rotate(ang, about_point=P(0, 0))
        lever_body.shift(c)

        return VGroup(bp_l, bp_r, lever_body)

    drawing = always_redraw(redraw_components)
    part.mob.add(drawing)

    def tick(m, dt):
        phase["x"] = phase["x"] + dt * 8.0 * part.speed.get_value()

    clock = VMobject()
    clock.add_updater(tick)
    part.mob.add(clock)

    def action_highlight(val=True, instant=False):
        target = 1.0 if val else 0.0
        if instant:
            hl_tracker.set_value(target)
            return []
        return [hl_tracker.animate.set_value(target)]

    part.actions["highlight"] = action_highlight
    part.motor_actions()

    return part

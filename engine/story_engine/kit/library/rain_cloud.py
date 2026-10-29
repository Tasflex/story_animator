# Kit part "rain_cloud", generated for "How Panama Canal locks lift giant ships over a mountain using only gravity" and kept for reuse.
# Validated with `python -m story_engine validate-part`. Edit freely; keep PART and build(k, spec).
PART = {
    "name": "rain_cloud",
    "use": "rain cloud producing falling raindrops; actions: on, off, highlight",
    "actions": ["on", "off", "highlight"],
}


def build(k, spec):
    th = k.theme
    rain_color = colour(th, "water", "#4FC3F7")

    frame = Rectangle(width=2.4, height=2.4, stroke_width=0, fill_opacity=0)

    # Cloud structural elements
    c_left = Circle(
        radius=0.38,
        stroke_color=th.ink,
        stroke_width=4,
        fill_color=th.bg2,
        fill_opacity=1.0,
    ).move_to(P(-0.5, 0.25))
    c_top = Circle(
        radius=0.48,
        stroke_color=th.ink,
        stroke_width=4,
        fill_color=th.bg2,
        fill_opacity=1.0,
    ).move_to(P(-0.05, 0.42))
    c_right = Circle(
        radius=0.36,
        stroke_color=th.ink,
        stroke_width=4,
        fill_color=th.bg2,
        fill_opacity=1.0,
    ).move_to(P(0.5, 0.25))
    base_rect = RoundedRectangle(
        corner_radius=0.2,
        width=1.4,
        height=0.45,
        stroke_color=th.ink,
        stroke_width=4,
        fill_color=th.bg2,
        fill_opacity=1.0,
    ).move_to(P(0, 0.15))

    # Inner patch to hide internal overlapping stroke lines
    cover = VGroup(
        Circle(radius=0.34).move_to(P(-0.5, 0.25)),
        Circle(radius=0.44).move_to(P(-0.05, 0.42)),
        Circle(radius=0.32).move_to(P(0.5, 0.25)),
        Rectangle(width=1.32, height=0.38).move_to(P(0, 0.15)),
    ).set_style(fill_color=th.bg2, fill_opacity=1.0, stroke_width=0)

    stroke_mobs = [c_left, c_top, c_right, base_rect]
    cloud_body = VGroup(c_left, c_top, c_right, base_rect, cover)

    phase = [0.0]

    part = Part(
        VGroup(frame, cloud_body),
        ports={
            "top": (0, 0.5),
            "bottom": (0, -0.5),
            "in": (0, 0.5),
            "out": (0, -0.5),
            "rain": (0, -0.5),
            "left": (-0.5, 0.15),
            "right": (0.5, 0.15),
            "cloud": (0, 0.25),
        },
    )

    def draw_drops():
        c = frame.get_center()
        s = frame.height / 2.4
        sp = part.speed.get_value()
        g = VGroup()

        cols = [-0.55, -0.28, 0.0, 0.28, 0.55]
        drops_per_col = 2
        drop_len = 0.22
        dx = -0.05

        for i, cx in enumerate(cols):
            for j in range(drops_per_col):
                offset = (phase[0] + (i * 0.2 + j * 0.5)) % 1.0
                y_top = -0.05 - offset * 0.95
                y_bot = y_top - drop_len

                if offset < 0.15:
                    alpha = offset / 0.15
                elif offset > 0.8:
                    alpha = (1.0 - offset) / 0.2
                else:
                    alpha = 1.0

                alpha *= min(1.0, sp * 1.5)
                if sp < 0.01:
                    alpha = 0.0

                p1 = c + np.array([cx * s, y_top * s, 0])
                p2 = c + np.array([(cx + dx) * s, y_bot * s, 0])

                g.add(
                    Line(
                        p1,
                        p2,
                        stroke_color=rain_color,
                        stroke_width=3.5 * s,
                        stroke_opacity=alpha,
                    )
                )
        return g

    drops = always_redraw(draw_drops)
    part.mob.add(drops)

    def update_drops(m, dt):
        phase[0] = (phase[0] + dt * 0.8 * part.speed.get_value()) % 1.0

    drops.add_updater(update_drops)

    part.motor_actions()

    def act_highlight(val=True, instant=False):
        target_color = th.accent if val else th.ink
        target_width = 6 if val else 4
        if instant:
            for m in stroke_mobs:
                m.set_stroke(color=target_color, width=target_width)
            return []
        return [
            m.animate.set_stroke(color=target_color, width=target_width)
            for m in stroke_mobs
        ]

    part.actions["highlight"] = act_highlight

    if spec.get("on", spec.get("active", True)):
        part.speed.set_value(1.0)
    else:
        part.speed.set_value(0.0)

    return part

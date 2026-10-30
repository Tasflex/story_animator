# Kit part "escape_wheel", generated for "How a mechanical watch keeps perfect time with a spring and a wobbling wheel" and kept for reuse.
# Validated with `python -m story_engine validate-part`. Edit freely; keep PART and build(k, spec).
PART = {
    "name": "escape_wheel",
    "use": "watch escapement gear with angled club teeth and central pivot; actions: on, off, highlight",
    "actions": ["on", "off", "highlight"]
}

def build(k, spec):
    th = k.theme

    # Invisible frame for relative positioning in updaters post-scaling
    ref_frame = Square(side_length=2.2, stroke_width=0, fill_opacity=0)

    # Number of club teeth (standard watch escape wheel usually has 12 to 15)
    num_teeth = spec.get("teeth", 12) if isinstance(spec, dict) else 12
    verts = []
    tip_polys = VGroup()

    for i in range(num_teeth):
        step = 2 * math.pi / num_teeth
        base_a = i * step

        # Angled club tooth profile
        p1 = P(0.68 * math.cos(base_a), 0.68 * math.sin(base_a))
        p2 = P(0.92 * math.cos(base_a + 0.08), 0.92 * math.sin(base_a + 0.08))
        p3 = P(1.00 * math.cos(base_a + 0.16), 1.00 * math.sin(base_a + 0.16))
        p4 = P(0.92 * math.cos(base_a + 0.22), 0.92 * math.sin(base_a + 0.22))
        p5 = P(0.68 * math.cos(base_a + 0.28), 0.68 * math.sin(base_a + 0.28))

        verts.extend([p1, p2, p3, p4, p5])

        # Accentuate impulse faces on tooth tips
        tip = Polygon(
            p2, p3, p4,
            stroke_width=1,
            stroke_color=th.accent,
            fill_color=th.accent,
            fill_opacity=0.5
        )
        tip_polys.add(tip)

    wheel_body = Polygon(
        *verts,
        stroke_color=th.ink,
        stroke_width=4,
        fill_color=th.ink,
        fill_opacity=0.15
    )

    # Spokes connecting rim to arbor
    spokes = VGroup()
    num_spokes = 4
    for i in range(num_spokes):
        a = i * (2 * math.pi / num_spokes)
        spokes.add(
            Line(
                P(0.20 * math.cos(a), 0.20 * math.sin(a)),
                P(0.68 * math.cos(a), 0.68 * math.sin(a)),
                stroke_color=th.ink,
                stroke_width=3
            )
        )

    # Central hub and jewel pivot
    hub_outer = Circle(radius=0.20, stroke_color=th.ink, stroke_width=4, fill_color=th.bg, fill_opacity=1.0)
    hub_inner = Circle(radius=0.09, stroke_color=th.ink, stroke_width=2, fill_color=th.ink, fill_opacity=0.3)
    pivot_dot = Circle(radius=0.04, stroke_color=th.ink, stroke_width=1, fill_color=th.ink, fill_opacity=1.0)

    wheel_rot = VGroup(wheel_body, tip_polys, spokes, hub_outer, hub_inner, pivot_dot)

    hl_tracker = ValueTracker(0.0)

    def glow_effect():
        val = hl_tracker.get_value()
        v = max(0.0, min(1.0, float(val if val is not None else 0.0)))
        if v <= 0.001:
            return VGroup()
        c = ref_frame.get_center()
        s = ref_frame.height / 2.2
        halo = Circle(radius=1.12 * s, stroke_color=th.accent, stroke_width=4 * v, stroke_opacity=v)
        glow = Circle(radius=1.12 * s, stroke_color=th.accent, stroke_width=0, fill_color=th.accent, fill_opacity=0.25 * v)
        halo.move_to(c)
        glow.move_to(c)
        return VGroup(glow, halo)

    glow_mob = always_redraw(glow_effect)

    root_mob = VGroup(ref_frame, glow_mob, wheel_rot)

    ports = {
        "in": (-0.5, 0),
        "out": (0.5, 0),
        "top": (0, 0.5),
        "bottom": (0, -0.5),
        "pallet": (0, 0.45),
        "center": (0, 0)
    }

    part = Part(root_mob, ports=ports)
    
    # Motor actions set up on and off speed transitions
    part.motor_actions()
    
    # Continuous rotation scaled by part.speed
    part.spinner(wheel_rot, rate=-2.5)

    def do_highlight(val=1.0, instant=False):
        if val is None:
            val = 1.0
        target = float(val)
        if instant:
            hl_tracker.set_value(target)
            return []
        return [hl_tracker.animate.set_value(target)]

    part.actions["highlight"] = do_highlight

    return part

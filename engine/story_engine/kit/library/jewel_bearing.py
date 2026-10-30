# Kit part "jewel_bearing", generated for "How a mechanical watch keeps perfect time with a spring and a wobbling wheel" and kept for reuse.
# Validated with `python -m story_engine validate-part`. Edit freely; keep PART and build(k, spec).
PART = {
    "name": "jewel_bearing",
    "use": "jewel bearing (synthetic ruby doughnut holding a steel shaft pivot) for low-friction pivots; actions: on, off, highlight",
    "actions": ["on", "off", "highlight"]
}

def build(k, spec):
    th = k.theme
    ruby_color = spec.get("ruby_color", "#D81B60")
    steel_color = spec.get("steel_color", th.muted)
    
    # Outer bounding frame
    frame = Rectangle(width=2.4, height=2.4, stroke_width=0, fill_opacity=0)
    
    # Outer housing / brass setting
    housing_outer = Circle(radius=1.05, stroke_color=th.ink, stroke_width=4, fill_color=th.bg2, fill_opacity=0.4)
    housing_inner = Circle(radius=0.88, stroke_color=th.muted, stroke_width=2)
    
    # Synthetic Ruby Jewel (Doughnut shape)
    ruby_jewel = Annulus(
        inner_radius=0.32,
        outer_radius=0.85,
        fill_color=ruby_color,
        fill_opacity=0.6,
        stroke_color=th.ink,
        stroke_width=3
    )
    
    # Chamfer ring / inner seat outline
    ruby_bevel = Circle(radius=0.42, stroke_color=th.ink, stroke_width=1.5, stroke_opacity=0.5)
    
    # Lubricant oil reservoir (capillary oil drop around hole)
    oil_drop = Circle(radius=0.36, fill_color="#4FC3F7", fill_opacity=0.45, stroke_color="#0288D1", stroke_width=1.5)
    
    # Incabloc shock spring retention clip (schematic arcs)
    clip_left = Arc(radius=0.75, start_angle=math.pi * 0.6, angle=math.pi * 0.8, stroke_color=th.ink, stroke_width=3)
    clip_right = Arc(radius=0.75, start_angle=-math.pi * 0.4, angle=math.pi * 0.8, stroke_color=th.ink, stroke_width=3)
    clip_center = Circle(radius=0.52, stroke_color=th.ink, stroke_width=2, stroke_opacity=0.6)
    spring_clip = VGroup(clip_left, clip_right, clip_center)
    
    # Rotating Steel Shaft Pivot
    pivot_base = Circle(radius=0.25, stroke_color=th.ink, stroke_width=3, fill_color=steel_color, fill_opacity=0.95)
    pivot_center = Dot(point=P(0, 0), radius=0.06, color=th.ink)
    
    # Crosshair / mark to visualize shaft rotation clearly
    notch_h = Line(P(-0.2, 0), P(0.2, 0), stroke_color=th.ink, stroke_width=2.5)
    notch_v = Line(P(0, -0.2), P(0, 0.2), stroke_color=th.ink, stroke_width=2.5)
    pivot_mark = Dot(point=P(0.13, 0.13), radius=0.04, color=th.bg)
    
    pivot_group = VGroup(pivot_base, notch_h, notch_v, pivot_center, pivot_mark)
    
    # Full Assembly
    bearing_assembly = VGroup(
        frame,
        housing_outer,
        housing_inner,
        ruby_jewel,
        ruby_bevel,
        oil_drop,
        spring_clip,
        pivot_group
    )
    
    part = Part(
        bearing_assembly,
        ports={
            "in": (-0.5, 0),
            "out": (0.5, 0),
            "top": (0, 0.5),
            "bottom": (0, -0.5),
            "pivot": (0, 0)
        }
    )
    
    # Smooth rotation of the central pivot shaft tied to part.speed
    part.spinner(pivot_group, rate=4.0)
    part.motor_actions()
    
    def do_highlight(val=True, instant=False):
        target_opacity = 0.95 if val else 0.6
        target_color = th.accent if val else th.ink
        if instant:
            ruby_jewel.set_fill(opacity=target_opacity)
            housing_outer.set_stroke(color=target_color)
            return []
        return [
            ruby_jewel.animate.set_fill(opacity=target_opacity),
            housing_outer.animate.set_stroke(color=target_color)
        ]
    
    part.actions["highlight"] = do_highlight
    
    return part

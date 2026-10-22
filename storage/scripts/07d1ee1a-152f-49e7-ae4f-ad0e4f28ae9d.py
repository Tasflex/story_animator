from manim import *

class StoryAnimation(MovingCameraScene):
    def construct(self):
        self.camera.background_color = "#0F172A"
        self.camera.frame.save_state()

        # ============ TITLE CARD ============
        self.next_section("scene_0")
        title = Text("How Neural Links", font_size=54, color="#38BDF8", weight=BOLD)
        title2 = Text("Read Thoughts", font_size=54, color="#F43F5E", weight=BOLD)
        title_group = VGroup(title, title2).arrange(DOWN, buff=0.3)
        underline = Line(LEFT * 3, RIGHT * 3, color=WHITE, stroke_width=2)
        underline.next_to(title_group, DOWN, buff=0.4)
        title_card = VGroup(title_group, underline)

        self.play(Write(title), run_time=0.6)
        self.play(Write(title2), run_time=0.6)
        self.play(Create(underline), run_time=0.3)
        self.wait(0.2)
        self.play(FadeOut(title_card), run_time=0.4)

        # ============ SCENE 1: Neural Network ============
        # duration ~ 10.45 - 2.1 (title) = ~8.35
        nodes = VGroup()
        positions = []
        import math
        for i in range(40):
            angle = i * 2 * math.pi / 40
            r = 2.5 + 0.6 * math.sin(i * 1.3)
            x = r * math.cos(angle)
            y = r * math.sin(angle) * 0.85
            positions.append([x, y, 0])
            dot = Dot(point=[x, y, 0], radius=0.07, color="#38BDF8")
            nodes.add(dot)

        edges = VGroup()
        for i in range(len(positions)):
            for j in range(i + 1, len(positions)):
                d = math.dist(positions[i][:2], positions[j][:2])
                if d < 1.6:
                    line = Line(positions[i], positions[j], color="#38BDF8", stroke_width=1, stroke_opacity=0.35)
                    edges.add(line)

        network = VGroup(edges, nodes)
        self.play(LaggedStart(*[Create(e) for e in edges], lag_ratio=0.005, run_time=1.2))
        self.play(LaggedStart(*[GrowFromCenter(n) for n in nodes], lag_ratio=0.01, run_time=0.8))

        # random pulses
        for _ in range(6):
            idx = int(np.random.randint(0, len(nodes)))
            pulse = nodes[idx].copy().set_color("#F43F5E")
            self.play(Transform(nodes[idx], pulse), run_time=0.15)
            self.play(Transform(nodes[idx], Dot(point=positions[idx], radius=0.07, color="#38BDF8")), run_time=0.15)

        # zoom into central pathway
        center_node = Dot(ORIGIN, radius=0.12, color="#F43F5E")
        self.play(FadeIn(center_node), run_time=0.3)
        self.camera.frame.save_state()
        self.play(
            self.camera.frame.animate.scale(0.55).move_to(center_node.get_center()),
            network.animate.set_opacity(0.25),
            run_time=1.2
        )
        highlight = Circle(radius=0.6, color="#F43F5E", stroke_width=2).move_to(center_node)
        self.play(Create(highlight), run_time=0.5)
        self.play(FadeOut(network), FadeOut(center_node), FadeOut(highlight), run_time=0.6)
        self.play(self.camera.frame.animate.restore(), run_time=0.8)

        # ============ SCENE 2: Neuron ============
        self.next_section("scene_1")
        # duration ~ 6.78
        soma = Circle(radius=0.5, color="#38BDF8", stroke_width=3)
        nucleus = Circle(radius=0.2, color="#E2E8F0", stroke_width=2)
        dendrites = VGroup()
        for i in range(6):
            a = i * TAU / 6
            end = [1.4 * math.cos(a), 1.4 * math.sin(a), 0]
            dendrites.add(Line([0.5 * math.cos(a), 0.5 * math.sin(a), 0], end, color="#38BDF8", stroke_width=2))
        axon = Line([0.5, 0, 0], [2.5, 0, 0], color="#E2E8F0", stroke_width=3)
        terminals = VGroup()
        for dy in [-0.3, 0, 0.3]:
            terminals.add(Line([2.5, 0, 0], [2.9, dy, 0], color="#E2E8F0", stroke_width=2))
        neuron = VGroup(soma, nucleus, dendrites, axon, terminals)

        self.play(Create(soma), Create(nucleus), run_time=0.6)
        self.play(LaggedStart(*[Create(d) for d in dendrites], lag_ratio=0.1, run_time=0.8))
        self.play(Create(axon), run_time=0.4)
        self.play(LaggedStart(*[Create(t) for t in terminals], lag_ratio=0.1, run_time=0.5))

        # radiating arcs
        for _ in range(3):
            arcs = VGroup()
            for k in range(3):
                arc = Circle(radius=0.7 + k * 0.4, color="#F43F5E", stroke_width=2, stroke_opacity=0.8)
                arcs.add(arc)
            self.play(
                LaggedStart(*[a.animate.scale(1.8).set_opacity(0) for a in arcs], lag_ratio=0.2),
                run_time=0.9
            )
            self.remove(arcs)

        self.play(FadeOut(neuron), run_time=0.5)

        # ============ SCENE 3: Two panels ============
        self.next_section("scene_2")
        # duration ~ 11.19
        left_panel = RoundedRectangle(width=5, height=3.5, corner_radius=0.15, color="#38BDF8", stroke_width=2).shift(LEFT * 3.2)
        right_panel = RoundedRectangle(width=5, height=3.5, corner_radius=0.15, color="#F43F5E", stroke_width=2).shift(RIGHT * 3.2)
        left_label = Text("Scalp EEG", font_size=22, color="#38BDF8").next_to(left_panel, UP, buff=0.2)
        right_label = Text("Neural Link", font_size=22, color="#F43F5E").next_to(right_panel, UP, buff=0.2)

        self.play(Create(left_panel), Create(right_panel), run_time=0.6)
        self.play(Write(left_label), Write(right_label), run_time=0.4)

        # noisy flat line
        noisy_pts = [left_panel.get_left() + RIGHT * 0.3 + UP * (0.05 * np.random.randn()) + RIGHT * (i * 0.08) for i in range(55)]
        noisy_line = VMobject(color="#38BDF8", stroke_width=2).set_points_as_corners(noisy_pts)
        self.play(Create(noisy_line), run_time=1.0)

        # right: three glowing points + needle
        pts = [right_panel.get_center() + RIGHT * 0.8 + UP * 0.8,
               right_panel.get_center() + RIGHT * 0.8,
               right_panel.get_center() + RIGHT * 0.8 + DOWN * 0.8]
        glow_pts = VGroup(*[Dot(p, radius=0.1, color="#F43F5E") for p in pts])
        self.play(LaggedStart(*[GrowFromCenter(g) for g in glow_pts], lag_ratio=0.2, run_time=0.6))

        needle_start = right_panel.get_top() + LEFT * 1.2
        needle = Line(needle_start, needle_start + DOWN * 3, color="#38BDF8", stroke_width=3)
        self.play(Create(needle), run_time=0.6)
        self.play(needle.animate.shift(RIGHT * 2.4), run_time=1.0)

        # crisp spikes
        spikes = VGroup()
        base = right_panel.get_bottom() + UP * 0.4 + LEFT * 2
        for i in range(15):
            x = i * 0.25
            h = 0.6 if i % 3 == 0 else 0.1
            spikes.add(Line(base + RIGHT * x, base + RIGHT * x + UP * h, color="#38BDF8", stroke_width=2))
        self.play(LaggedStart(*[Create(s) for s in spikes], lag_ratio=0.05, run_time=1.2))
        self.wait(0.3)

        self.play(FadeOut(left_panel), FadeOut(right_panel), FadeOut(left_label), FadeOut(right_label),
                  FadeOut(noisy_line), FadeOut(glow_pts), FadeOut(needle), FadeOut(spikes), run_time=0.8)

        # ============ SCENE 4: Microchip ============
        self.next_section("scene_3")
        # duration ~ 8.36
        chip = RoundedRectangle(width=2.5, height=2.5, corner_radius=0.15, color="#38BDF8", stroke_width=3)
        chip_label = Text("ADC", font_size=28, color=WHITE).move_to(chip)
        chip_group = VGroup(chip, chip_label)
        self.play(Create(chip), Write(chip_label), run_time=0.6)

        # incoming analog wave
        wave_pts = []
        for i in range(60):
            x = -5 + i * 0.1
            y = 0.5 * math.sin(i * 0.7)
            wave_pts.append([x, y, 0])
        wave = VMobject(color="#F43F5E", stroke_width=3).set_points_as_corners(wave_pts)
        self.play(Create(wave), run_time=1.0)
        self.play(wave.animate.shift(RIGHT * 2.5), run_time=1.0)
        self.play(FadeOut(wave), run_time=0.3)

        # digital grid
        grid = VGroup()
        for i in range(4):
            for j in range(4):
                n = Text(str(int(np.random.randint(0, 9))), font_size=20, color="#10B981")
                n.move_to(chip.get_center() + RIGHT * (1.8 + i * 0.5) + UP * (0.75 - j * 0.5))
                grid.add(n)
        self.play(LaggedStart(*[FadeIn(n) for n in grid], lag_ratio=0.05, run_time=1.0))

        # collapse into vertical vector
        vector = VGroup()
        for j in range(4):
            n = Text(str(int(np.random.randint(0, 9))), font_size=20, color="#10B981")
            n.move_to(chip.get_center() + RIGHT * 3.8 + UP * (0.75 - j * 0.5))
            vector.add(n)
        bracket_l = Line(chip.get_center() + RIGHT * 3.55 + UP * 1.0, chip.get_center() + RIGHT * 3.55 + DOWN * 1.0, color="#10B981", stroke_width=2)
        bracket_r = Line(chip.get_center() + RIGHT * 4.05 + UP * 1.0, chip.get_center() + RIGHT * 4.05 + DOWN * 1.0, color="#10B981", stroke_width=2)

        self.play(
            LaggedStart(*[Transform(grid[i], vector[i]) for i in range(4)], lag_ratio=0.1),
            FadeOut(grid[4:]),
            Create(bracket_l), Create(bracket_r),
            run_time=1.2
        )
        self.wait(0.3)
        self.play(FadeOut(chip_group), FadeOut(vector), FadeOut(bracket_l), FadeOut(bracket_r), run_time=0.6)

        # ============ SCENE 5: Arrow + cursor ============
        self.next_section("scene_4")
        # duration ~ 7.57
        axes = Axes(x_range=[-3, 3], y_range=[-3, 3], x_length=5, y_length=5, axis_config={"color": GRAY, "stroke_width": 1}).shift(LEFT * 3)
        self.play(Create(axes), run_time=0.6)

        arrow = Arrow(axes.c2p(0, 0), axes.c2p(2, 1), buff=0, color="#38BDF8", stroke_width=4)
        self.play(GrowArrow(arrow), run_time=0.6)

        screen = RoundedRectangle(width=3.5, height=3.5, corner_radius=0.15, color="#10B981", stroke_width=2).shift(RIGHT * 3.5)
        cursor = Dot(screen.get_center(), radius=0.12, color="#38BDF8")
        self.play(Create(screen), run_time=0.4)
        self.play(FadeIn(cursor), run_time=0.3)

        # rotate arrow and mirror cursor
        for angle_deg in [45, 90, 135, 60, 20]:
            a = angle_deg * DEGREES
            new_end = axes.c2p(2 * math.cos(a), 2 * math.sin(a))
            new_arrow = Arrow(axes.c2p(0, 0), new_end, buff=0, color="#38BDF8", stroke_width=4)
            new_cursor_pos = screen.get_center() + RIGHT * 1.2 * math.cos(a) + UP * 1.2 * math.sin(a)
            self.play(
                Transform(arrow, new_arrow),
                cursor.animate.move_to(new_cursor_pos),
                run_time=0.55
            )

        self.wait(0.3)

        # ============ FADE OUT ============
        self.play(
            FadeOut(axes), FadeOut(arrow), FadeOut(screen), FadeOut(cursor),
            run_time=0.8
        )
        final_text = Text("Neural Links", font_size=44, color="#38BDF8", weight=BOLD)
        self.play(FadeIn(final_text), run_time=0.5)
        self.wait(0.3)
        self.play(FadeOut(final_text), run_time=0.5)
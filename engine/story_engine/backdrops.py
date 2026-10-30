"""Drawn thumbnail backdrops: deep, lit scenes that stop the scroll.

A video frame is flat; a thumbnail needs depth, a bright focal point and
contrast. Each backdrop draws a scene in perspective towards a vanishing point
`vp`, which thumbnail.py uses as the target of the beam from the subject.
"""

from __future__ import annotations

import math
import random

from PIL import Image, ImageChops, ImageDraw, ImageFilter

LED_COLORS = ["#22E3FF"] * 6 + ["#3DFF8B"] * 3 + ["#FF3B5C", "#FFB020"]


def bloom(light: Image.Image) -> Image.Image:
    """Light layer plus a tight and a wide blur of itself, like a camera's glare."""
    return ImageChops.screen(light, ImageChops.add(light.filter(ImageFilter.GaussianBlur(4)),
                                                   light.filter(ImageFilter.GaussianBlur(18))))


def radial(w: int, h: int, center: tuple[int, int], radius: int, color: str, strength: float) -> Image.Image:
    mask = Image.new("L", (w, h), 0)
    cx, cy = center
    ImageDraw.Draw(mask).ellipse((cx - radius, cy - radius, cx + radius, cy + radius), fill=round(255 * strength))
    mask = mask.filter(ImageFilter.GaussianBlur(radius * 0.6))
    return Image.composite(Image.new("RGB", (w, h), color), Image.new("RGB", (w, h), "black"), mask)


def datacenter(w: int, h: int, vp: tuple[int, int], seed: int = 7) -> Image.Image:
    """An endless corridor of server racks, LEDs blinking, light at the far end."""
    rng = random.Random(seed)
    vx, vy = vp
    f = 0.47 * w

    def p(x: float, y: float, z: float) -> tuple[float, float]:
        return vx + f * x / z, vy - f * y / z

    base = Image.new("RGB", (w, h), "#03060F")
    d = ImageDraw.Draw(base)
    light = Image.new("RGB", (w, h), "black")
    ld = ImageDraw.Draw(light)

    # Floor grid, fading into the distance
    for x in (-1, -0.5, 0, 0.5, 1):
        d.line((p(x, -1, 0.5), p(x, -1, 80)), fill="#0E2A44", width=2)
    z = 0.6
    while z < 60:
        d.line((p(-1, -1, z), p(1, -1, z)), fill="#0C2238", width=max(1, round(3 / z)))
        z *= 1.18

    # Ceiling light strips
    z = 1.5
    while z < 50:
        ld.polygon([p(-0.22, 1.3, z), p(0.22, 1.3, z), p(0.22, 1.3, z + 1.1), p(-0.22, 1.3, z + 1.1)], fill="#9FE8FF")
        z += 3.2

    # Racks on both walls, each a grid of LEDs
    for side in (-1, 1):
        z = 0.9
        while z < 45:
            z1 = z + 0.85
            d.polygon([p(side, -1, z), p(side, 1.0, z), p(side, 1.0, z1), p(side, -1, z1)],
                      fill="#0A1426", outline="#1B3A5C")
            for r in range(14):
                yy = -0.9 + 1.8 * r / 14
                for c in range(6):
                    if rng.random() < 0.55:
                        continue
                    sx, sy = p(side, yy, z + 0.08 + (z1 - z - 0.16) * c / 5)
                    s = max(1.0, 0.012 * f / z)
                    ld.ellipse((sx - s, sy - s, sx + s, sy + s), fill=rng.choice(LED_COLORS))
            z = z1 + 0.15

    out = ImageChops.screen(base, bloom(light))
    # Light at the end of the corridor
    out = ImageChops.screen(out, radial(w, h, vp, int(h * 0.22), "#BFF4FF", 0.9))
    return ImageChops.screen(out, radial(w, h, vp, int(h * 0.55), "#0B6E99", 0.55))


def ocean(w: int, h: int, vp: tuple[int, int], seed: int = 11) -> Image.Image:
    """Deep water: light shafts from the surface, drifting particles, a glow where the beam lands."""
    rng = random.Random(seed)
    vx, vy = vp
    base = Image.new("RGB", (w, h), "#010B16")
    # Darker towards the seabed
    depth = Image.new("L", (w, h))
    for y in range(h):
        depth.paste(round(150 * y / h), (0, y, w, y + 1))
    base = Image.composite(Image.new("RGB", (w, h), "#000306"), base, depth)
    base = ImageChops.screen(base, radial(w, h, (w // 2, -h // 3), int(h * 0.9), "#0A4F7A", 0.8))

    light = Image.new("RGB", (w, h), "black")
    ld = ImageDraw.Draw(light)
    for _ in range(9):  # shafts fanning down from the surface
        x0 = rng.uniform(-0.1, 1.1) * w
        spread = rng.uniform(0.04, 0.09) * w
        x1 = x0 + (vx - x0) * 0.35
        ld.polygon([(x0 - spread * 0.3, 0), (x0 + spread * 0.3, 0), (x1 + spread, h), (x1 - spread, h)],
                   fill=rng.choice(["#0E3A55", "#12486A", "#0B3048"]))
    light = light.filter(ImageFilter.GaussianBlur(28))
    for _ in range(260):  # marine snow, bigger and brighter closer to the camera
        x, y, z = rng.uniform(0, w), rng.uniform(0, h), rng.uniform(0.2, 1.0)
        r = 1 + 3.5 * z
        ImageDraw.Draw(light).ellipse((x - r, y - r, x + r, y + r), fill="#9EE6FF" if z > 0.7 else "#4FA3C7")
    out = ImageChops.screen(base, bloom(light))
    out = ImageChops.screen(out, radial(w, h, vp, int(h * 0.2), "#BFF4FF", 0.85))
    return ImageChops.screen(out, radial(w, h, vp, int(h * 0.5), "#0B6E99", 0.5))


def rays(w: int, h: int, vp: tuple[int, int], seed: int = 3) -> Image.Image:
    """A burst of light from the vanishing point: depth and energy for any topic."""
    rng = random.Random(seed)
    vx, vy = vp
    base = Image.new("RGB", (w, h), "#05030F")
    light = Image.new("RGB", (w, h), "black")
    ld = ImageDraw.Draw(light)
    reach = 2 * max(w, h)
    k, angle = 0, 0.0
    while angle < 360:
        width = rng.uniform(2.5, 7.0)
        a0, a1 = math.radians(angle), math.radians(angle + width)
        ld.polygon([(vx, vy), (vx + reach * math.cos(a0), vy + reach * math.sin(a0)),
                    (vx + reach * math.cos(a1), vy + reach * math.sin(a1))],
                   fill=["#1B2A6B", "#2B1458", "#0E4D7A"][k % 3])
        angle += width + rng.uniform(3.0, 9.0)
        k += 1
    light = light.filter(ImageFilter.GaussianBlur(10))
    for _ in range(180):  # sparks streaming out of the light
        a = rng.uniform(0, 2 * math.pi)
        dist = rng.uniform(0.1, 1.0) ** 0.7 * max(w, h) * 0.75
        x, y = vx + dist * math.cos(a), vy + dist * math.sin(a)
        r = 0.8 + 3.0 * dist / max(w, h)
        ImageDraw.Draw(light).ellipse((x - r, y - r, x + r, y + r), fill=rng.choice(["#7FDBFF", "#C9A7FF", "#FFD08A"]))
    out = ImageChops.screen(base, bloom(light))
    out = ImageChops.screen(out, radial(w, h, vp, int(h * 0.2), "#E6F7FF", 0.9))
    return ImageChops.screen(out, radial(w, h, vp, int(h * 0.55), "#3B2A8F", 0.55))


BACKDROPS = {"datacenter": datacenter, "ocean": ocean, "rays": rays}

#!/usr/bin/env python3
"""Render MaxoGranular's photographic textures: the rack frame with its worn hammertone module
plates, knobs, reels, CRT bezels and glass, lamps, buttons, jacks, keys, wheels and the badge.

Run from the repo root:  python3 tools/render_textures.py
Writes PNG / JPEG files into plugins/maxogranular/assets/ui/. Everything is procedural
(numpy + Pillow + scipy), so the look can be tuned here and re-rendered at any resolution.
The rectangles below must match plugins/maxogranular/src/editor/layout.rs.
"""
import math
import os

import numpy as np
from PIL import Image, ImageDraw, ImageFont
from scipy.ndimage import gaussian_filter

OUT = os.path.join(os.path.dirname(__file__), "..", "plugins", "maxogranular", "assets", "ui")
os.makedirs(OUT, exist_ok=True)
RNG = np.random.default_rng(20261122)
S = 2  # texture pixels per GUI pixel

W, H = 1240, 820
CHEEK = 16
# module plates (x, y, w, h) in GUI pixels - keep in sync with layout.rs
PLATES = {
    "header": (24, 6, 1192, 54),
    "a": (24, 64, 590, 332),
    "b": (626, 64, 590, 332),
    "mod": (24, 400, 626, 292),
    "filter": (654, 400, 252, 292),
    "fx": (910, 400, 306, 292),
    "perf": (24, 696, 398, 118),
}
KEYBED = (426, 696, 790, 118)
# CRT screens (x, y, w, h): the glass area, the bezel goes around it
CRTS = {"layer": (352, 150), "mod": (318, 176), "filter": (224, 98)}

FONT_DIR = "/usr/share/fonts/truetype"


def font(name, size):
    for p in [f"{FONT_DIR}/google-fonts/ofl/{name}", f"{FONT_DIR}/dejavu/{name}"]:
        if os.path.exists(p):
            return ImageFont.truetype(p, size)
    return ImageFont.load_default()


def blur(a, r, mode="wrap"):
    if r <= 0:
        return a
    return gaussian_filter(a.astype(np.float32), r, mode=mode)


def noise(h, w, r):
    return blur(RNG.standard_normal((h, w)).astype(np.float32), r)


def norm(a):
    a = a - a.min()
    return a / max(a.max(), 1e-9)


def save_rgb(a, name, q=90):
    im = Image.fromarray(np.clip(a * 255.0 + 0.5, 0, 255).astype(np.uint8), "RGB")
    path = os.path.join(OUT, name)
    if name.endswith(".jpg"):
        im.save(path, quality=q, optimize=True)
    else:
        im.save(path, optimize=True)


def save_rgba(rgb, alpha, name, down=1):
    a = np.dstack([np.clip(rgb, 0, 1), np.clip(alpha, 0, 1)[..., None]])
    im = Image.fromarray((a * 255.0 + 0.5).astype(np.uint8), "RGBA")
    if down > 1:
        im = im.resize((im.width // down, im.height // down), Image.LANCZOS)
    im.save(os.path.join(OUT, name), optimize=True)


def rrect_sd(xx, yy, x0, y0, x1, y1, rad):
    """Signed distance to a rounded rectangle, positive inside."""
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    hx, hy = (x1 - x0) / 2 - rad, (y1 - y0) / 2 - rad
    qx = np.abs(xx - cx) - hx
    qy = np.abs(yy - cy) - hy
    out = np.sqrt(np.maximum(qx, 0) ** 2 + np.maximum(qy, 0) ** 2) + np.minimum(np.maximum(qx, qy), 0) - rad
    return -out


def scratches(h, w, n, mean_len, rng=RNG, horiz=0.5):
    simg = Image.new("F", (w, h), 0.0)
    sd = ImageDraw.Draw(simg)
    for _ in range(n):
        x0, y0 = rng.uniform(0, w), rng.uniform(0, h)
        ln = rng.exponential(mean_len) + 3
        ang = rng.normal(0, 0.15) if rng.random() < horiz else rng.uniform(0, math.pi)
        curv = rng.normal(0, 0.002)
        steps = int(ln / 5) + 2
        pts = []
        for i in range(steps):
            t = i / (steps - 1)
            a = ang + curv * t * ln
            pts.append((x0 + math.cos(a) * t * ln, y0 + math.sin(a) * t * ln))
        sd.line(pts, fill=float(rng.uniform(0.15, 0.6)), width=1)
    return np.asarray(simg, dtype=np.float32)


# --------------------------------------------------------------------------------------------- rack
def wood_strip(h, w):
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    warp = noise(h, w, 40) * 60 + noise(h, w, 8) * 5
    rings = np.sin((xx * 0.7 + warp + yy * 0.015) * 0.45)
    fine = noise(h, w, 0.8) * 0.5
    streak = np.repeat(blur(RNG.standard_normal((1, w)).astype(np.float32), 1.5), h, axis=0)
    v = 0.55 + 0.16 * rings + 0.08 * fine + 0.10 * streak
    v *= 1.0 - 0.45 * (np.abs(xx / w - 0.5) * 2) ** 3
    base = np.array([0.27, 0.14, 0.07], np.float32)
    rgb = base * v[..., None]
    rgb *= (1.08 - 0.25 * (yy / h))[..., None]
    rgb += (np.clip(0.10 - np.abs(xx / w - 0.32), 0, 1) * 0.7)[..., None] * np.array([0.22, 0.15, 0.10])
    # dents and a worn varnish band where hands rest
    wear = np.clip(norm(noise(h, w, 25)) * 1.6 - 0.9, 0, 1)
    rgb = rgb * (1 - 0.25 * wear[..., None]) + np.array([0.36, 0.24, 0.15]) * 0.25 * wear[..., None]
    return rgb


def faceplate():
    h, w = H * S, W * S
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)

    # the rack behind the plates: black crinkle paint
    crink = noise(h, w, 1.2)
    crink = blur(np.abs(crink), 0.6)
    rgb = (0.045 + 0.035 * norm(crink))[..., None] * np.array([1.0, 0.97, 0.95], np.float32)
    rgb *= (1.0 - 0.25 * (yy / h))[..., None]

    # wooden cheeks
    cw = CHEEK * S
    rgb[:, :cw] = wood_strip(h, cw)
    rgb[:, w - cw:] = wood_strip(h, cw)[:, ::-1]

    # plate paint: a deep blue-grey hammertone enamel
    hn = blur(RNG.standard_normal((h, w)).astype(np.float32), 3.2 * S / 2)
    hn += 0.5 * blur(RNG.standard_normal((h, w)).astype(np.float32), 1.2)
    gy, gx = np.gradient(hn)
    hammer = (gx * -0.6 + gy * -0.8)
    hammer = hammer / (np.std(hammer) + 1e-6)
    paint = np.array([0.20, 0.235, 0.245], np.float32)
    bare = np.array([0.36, 0.36, 0.355], np.float32)
    big = noise(h, w, 80)
    grime = np.clip(norm(noise(h, w, 50) + 0.7 * noise(h, w, 14)) * 1.5 - 0.55, 0, 1)
    scr = scratches(h, w, 900, 30 * S)
    scr_d = np.roll(np.roll(scr, 1, 0), 1, 1)

    shadow = np.zeros((h, w), np.float32)
    plate_mask = np.zeros((h, w), np.float32)
    plate_rgb = np.zeros((h, w, 3), np.float32)
    for name, (px, py, pw, ph) in list(PLATES.items()) + [("keys", KEYBED)]:
        x0, y0, x1, y1 = px * S, py * S, (px + pw) * S, (py + ph) * S
        sd = rrect_sd(xx, yy, x0, y0, x1, y1, 4 * S)
        m = np.clip(sd + 0.5, 0, 1)
        sh = rrect_sd(xx - 3 * S, yy - 5 * S, x0, y0, x1, y1, 4 * S)
        shadow = np.maximum(shadow, np.clip((sh + 9 * S) / (9 * S), 0, 1) ** 2 * (1 - m))
        if name == "keys":
            # the key bed: dark felt / wood
            felt = (0.06 + 0.02 * norm(noise(h, w, 1.0)))[..., None] * np.array([0.75, 0.18, 0.16])
            col = felt * (1.0 - 0.3 * np.clip((yy - y0) / (y1 - y0), 0, 1))[..., None]
            plate_rgb = plate_rgb * (1 - m[..., None]) + col * m[..., None]
            plate_mask = np.maximum(plate_mask, m)
            continue
        inside = np.clip(sd, 0, None)
        lum = 1.0 + 0.10 * hammer + 0.05 * big
        lum *= 1.10 - 0.22 * np.clip((yy - y0) / (y1 - y0), 0, 1)
        col = paint * lum[..., None]
        # grime in the paint, darker at the bottom of the plate
        g = grime * (0.4 + 0.8 * np.clip((yy - y0) / (y1 - y0), 0, 1))
        col *= (1.0 - 0.30 * g)[..., None]
        # worn edges and chips down to the steel
        d = inside / S
        edge = np.clip((5 - d) / 5, 0, 1) ** 1.6
        z = lambda q: (q - q.mean()) / (q.std() + 1e-9)
        score = z(noise(h, w, 5 * S)) + 0.45 * z(noise(h, w, 1.2 * S)) + 2.3 * np.exp(-d / 3.5)
        worn = np.clip((score - 2.35) * 3.0, 0, 1)
        chips = np.clip((z(noise(h, w, 3 * S)) + 0.35 * z(noise(h, w, 0.8 * S)) - 3.15) * 3.0, 0, 1)
        # rubbed areas: paint polished thin (lighter, shows the primer)
        rub = np.clip(z(noise(h, w, 30 * S)) - 1.2, 0, 1) * 0.30
        col = col * (1 - rub[..., None]) + np.array([0.36, 0.37, 0.35]) * rub[..., None]
        worn = np.clip(worn + chips, 0, 1)
        steel = bare * (0.85 + 0.25 * norm(noise(h, w, 0.8)))[..., None]
        rim = np.clip(blur(worn, 1.6, "nearest") - worn, 0, 1) * 2.2
        # the chip is a little dip: shade its upper-left inside edge
        dip = np.clip(worn - np.roll(np.roll(blur(worn, 1.5, 'nearest'), 2, 0), 2, 1), 0, 1)  # dark lip of the paint around a chip
        col = col * (1 - 0.5 * rim[..., None])
        col = col * (1 - worn[..., None]) + steel * worn[..., None]
        col = col * (1 - 0.35 * dip[..., None])
        # scratches: light in the paint
        col = col + scr[..., None] * 0.16 - scr_d[..., None] * 0.05
        # bevel
        bev = np.clip(1 - d / 2.2, 0, 1)
        top = np.clip(1 - (yy - y0) / (6 * S), 0, 1) + np.clip(1 - (xx - x0) / (6 * S), 0, 1)
        bot = np.clip(1 - (y1 - yy) / (6 * S), 0, 1) + np.clip(1 - (x1 - xx) / (6 * S), 0, 1)
        col = col + (bev * (0.20 * np.clip(top, 0, 1) - 0.18 * np.clip(bot, 0, 1)))[..., None]
        plate_rgb = plate_rgb * (1 - m[..., None]) + col * m[..., None]
        plate_mask = np.maximum(plate_mask, m)

    rgb = rgb * (1 - 0.75 * shadow[..., None])
    rgb = rgb * (1 - plate_mask[..., None]) + plate_rgb * plate_mask[..., None]

    # screws in the plate corners (and some rust around them)
    sc_rgb, sc_a = screw_img(13 * S)
    sc_n = sc_rgb.shape[0]
    for name, (px, py, pw, ph) in PLATES.items():
        inset = 9 if name != "header" else 10
        pts = [(px + inset, py + inset), (px + pw - inset, py + inset), (px + inset, py + ph - inset), (px + pw - inset, py + ph - inset)]
        if name == "header":
            pts = [(px + inset, py + ph / 2), (px + pw - inset, py + ph / 2)]
        for (sx, sy) in pts:
            cx, cy = int(sx * S), int(sy * S)
            x0, y0 = cx - sc_n // 2, cy - sc_n // 2
            # rust halo
            rr = np.sqrt((xx[y0 - 20:y0 + sc_n + 20, x0 - 20:x0 + sc_n + 20] - cx) ** 2 + (yy[y0 - 20:y0 + sc_n + 20, x0 - 20:x0 + sc_n + 20] - cy) ** 2)
            halo = np.clip(1 - rr / (sc_n * 0.85), 0, 1) ** 1.5 * np.clip(norm(noise(rr.shape[0], rr.shape[1], 3)) * 1.6 - 0.4, 0, 1) * RNG.uniform(0.2, 0.6)
            sub = rgb[y0 - 20:y0 + sc_n + 20, x0 - 20:x0 + sc_n + 20]
            sub[:] = sub * (1 - halo[..., None]) + np.array([0.28, 0.14, 0.06]) * halo[..., None]
            # rotate each screw randomly
            im = Image.fromarray((np.dstack([sc_rgb, sc_a[..., None]]) * 255).astype(np.uint8), "RGBA").rotate(RNG.uniform(0, 90), resample=Image.BICUBIC)
            a = np.asarray(im, dtype=np.float32) / 255.0
            sub = rgb[y0:y0 + sc_n, x0:x0 + sc_n]
            sub[:] = sub * (1 - a[..., 3:4]) + a[..., :3] * a[..., 3:4]

    rgb += RNG.standard_normal((h, w, 1)).astype(np.float32) * 0.007
    save_rgb(np.clip(rgb, 0, 1), "faceplate.jpg", q=88)


def screw_img(n):
    c = n / 2
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    dx, dy = xx - c + 0.5, yy - c + 0.5
    rr = np.sqrt(dx * dx + dy * dy)
    r = n * 0.36
    sh = np.clip(1 - (np.sqrt((dx - 1.5) ** 2 + (dy - 2.5) ** 2) - r) / (0.12 * n), 0, 1) ** 2 * 0.7
    head = np.clip(r - rr + 0.5, 0, 1)
    nd = (dx * -0.55 + dy * -0.7) / r
    hv = 0.40 + 0.28 * np.clip(nd, -1, 1) * (rr / r) + 0.05 * noise(n, n, 0.7) - 0.6 * np.clip(rr / r - 0.85, 0, 1)
    slot = np.clip(0.07 * n - np.abs(dx), 0, 1) * np.clip(r * 0.8 - rr, 0, 1)
    hv = hv * (1 - 0.8 * slot)
    rgb = hv[..., None] * np.array([0.97, 0.95, 0.92])
    rgb = rgb * head[..., None]
    return np.clip(rgb, 0, 1), np.maximum(sh, head)


# --------------------------------------------------------------------------------------------- knobs
def knob(n=256):
    """Base (shadow + black bakelite skirt), cap (spun aluminium, tinted at run time), rotating
    layer (knurl + pointer) and a fixed highlight drawn over everything."""
    c = n / 2
    r = n * 0.40
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    dx, dy = xx - c + 0.5, yy - c + 0.5
    rr = np.sqrt(dx * dx + dy * dy)
    th = np.arctan2(dy, dx)
    aa = lambda d: np.clip(d, 0, 1)
    lx, ly = -0.55, -0.75
    nd = (dx * lx + dy * ly) / (r + 1e-6)

    # base: soft drop shadow + skirt
    sx, sy = dx - 0.04 * n, dy - 0.07 * n
    sh = np.clip(1 - (np.sqrt(sx * sx + sy * sy) - r * 0.95) / (0.11 * n), 0, 1) ** 2 * 0.8
    rgb = np.zeros((n, n, 3), np.float32)
    al = sh.copy()
    skirt = aa(r - rr + 0.5)
    sk = 0.050 + 0.06 * np.clip(nd, 0, 1) ** 2 + 0.012 * noise(n, n, 1.0) + 0.02 * (1 - rr / r)
    rim = np.clip(1 - np.abs(rr - (r - 2.0)) / 2.0, 0, 1) * np.clip(nd + 0.25, 0, 1) * 0.30
    skc = (sk + rim)[..., None] * np.array([1.0, 0.97, 0.95])
    rgb = rgb * (1 - skirt[..., None]) + skc * skirt[..., None]
    al = np.maximum(al, skirt)
    # step between skirt and cap
    rc = r * 0.62
    step = np.clip(1 - np.abs(rr - rc - 2) / 3, 0, 1) * (rr > rc)
    rgb *= (1 - 0.6 * step)[..., None]
    save_rgba(rgb, al, "knob_base.png")

    # cap: spun aluminium (grey, tinted at run time)
    cap = aa(rc - rr + 0.5)
    rings = np.sin(rr * 1.7 + noise(n, n, 1) * 2) * 0.025 + noise(n, n, 0.5) * 0.02
    capv = 0.62 + rings - 0.10 * (rr / rc)
    bevel = np.clip(1 - (rc - rr) / (0.10 * rc), 0, 1) * cap
    capv = capv * (1 - 0.30 * bevel) + 0.25 * bevel * np.clip(nd, 0, 1)
    save_rgba(np.repeat(capv[..., None], 3, axis=2), cap, "knob_cap.png")

    # fixed light on the cap: the anisotropic bow tie of spun metal + specular dot (additive-ish)
    bow = (0.5 + 0.5 * np.cos(2 * (th - (-2.25)))) ** 8
    spec = np.exp(-(((dx + rc * 0.35) ** 2 + (dy + rc * 0.45) ** 2) / (0.03 * n * n))) * 0.5
    hl = (0.55 * bow + spec) * cap * (1 - 0.6 * bevel)
    hl += np.clip(1 - np.abs(rr - (r - 1.5)) / 1.5, 0, 1) * np.clip(nd, 0, 1) * 0.4 * (rr > rc + 3)
    save_rgba(np.ones((n, n, 3), np.float32), np.clip(hl, 0, 1) * 0.85, "knob_light.png")

    # rotating layer: knurl ribs on the skirt + cream pointer (pointing up)
    rgbr = np.zeros((n, n, 3), np.float32)
    alr = np.zeros((n, n), np.float32)
    zone = aa((rr - rc - 5) / 2) * aa((r - 2.5 - rr) / 1.5)
    ribs = np.clip((np.cos(th * 36) - 0.35) * 2.0, 0, 1)
    shade_r = 0.15 + 0.25 * np.clip(-np.sin(th * 36 + 0.8), 0, 1)
    rgbr = rgbr + (shade_r[..., None] * np.array([1.0, 0.97, 0.95])) * (ribs * zone)[..., None]
    alr = np.maximum(alr, ribs * zone * 0.85)
    px = np.abs(dx)
    up = -dy
    line_sk = aa(0.022 * n - px) * aa(up - rc - 3) * aa(r - 3 - up)
    line_cap = aa(0.018 * n - px) * aa(up - rc * 0.25) * aa(rc * 0.92 - up)
    rgbr = rgbr * (1 - line_sk[..., None]) + np.array([0.95, 0.91, 0.80]) * line_sk[..., None]
    alr = np.maximum(alr, line_sk)
    rgbr = rgbr * (1 - line_cap[..., None]) + np.array([0.08, 0.08, 0.09]) * line_cap[..., None]
    alr = np.maximum(alr, line_cap * 0.85)
    save_rgba(rgbr, alr, "knob_rot.png")


# --------------------------------------------------------------------------------------------- reels and tape
def reel(n=320):
    c = n / 2
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    dx, dy = xx - c + 0.5, yy - c + 0.5
    rr = np.sqrt(dx * dx + dy * dy)
    th = np.arctan2(dy, dx)
    aa = lambda d: np.clip(d, 0, 1)
    R = n * 0.48
    nd = (dx * -0.55 + dy * -0.75) / R
    disc = aa(R - rr + 0.5)
    # three big cut-outs and three small ones (classic aluminium NAB reel)
    holes = np.zeros((n, n), np.float32)
    for k in range(3):
        a = k * 2 * math.pi / 3 - math.pi / 2
        # kidney window between radius 0.30R and 0.88R, +-0.62 rad wide
        da = np.angle(np.exp(1j * (th - a)))
        win = aa((0.88 * R - rr) / 2) * aa((rr - 0.30 * R) / 2) * aa((0.62 - np.abs(da)) * rr / 2)
        holes = np.maximum(holes, win)
        a2 = a + math.pi / 3
        hx, hy = c + math.cos(a2) * 0.20 * R, c + math.sin(a2) * 0.20 * R
        small = aa(0.055 * R - np.sqrt((xx - hx) ** 2 + (yy - hy) ** 2))
        holes = np.maximum(holes, small)
    hub = aa(0.14 * R - rr)
    body = disc * (1 - holes)
    # brushed / spun aluminium with a raised rim
    spun = 0.50 + 0.02 * np.sin(rr * 2.3) + 0.03 * noise(n, n, 0.6)
    rim = np.clip(1 - np.abs(rr - 0.95 * R) / (0.03 * R), 0, 1)
    v = spun + 0.10 * rim * np.sign(nd) + 0.06 * nd
    # wear: scuffs
    sc = scratches(n, n, 40, n * 0.08, horiz=0.0)
    v = v + 0.12 * sc
    # hub with the three-prong spindle hole (dark)
    spindle = aa(0.075 * R - rr) * (1 - aa((np.cos(th * 3) - 0.6) * 4) * aa(rr - 0.04 * R))
    v = np.where(hub > 0, 0.62 + 0.15 * nd, v)
    v = v * (1 - 0.85 * spindle)
    rgb = v[..., None] * np.array([0.92, 0.93, 0.95])
    # an edge of the cut-outs (machined, a bit darker)
    edge = np.clip(blur(holes, 1.5) - holes, 0, 1) * disc
    rgb *= (1 - 0.6 * edge)[..., None]
    # little red label sticker on the hub
    lab = aa(0.13 * R - np.sqrt((dx - 0.0) ** 2 + (dy + 0.0) ** 2)) * aa(np.sqrt(dx ** 2 + dy ** 2) - 0.09 * R)
    rgb = rgb * (1 - lab[..., None] * 0.8) + np.array([0.55, 0.12, 0.08]) * lab[..., None] * 0.8
    save_rgba(rgb, body, "reel.png", down=1)

    # tape pack (scaled at run time): dark brown with fine winding rings and a shiny edge
    t = n
    tc = t / 2
    yy, xx = np.mgrid[0:t, 0:t].astype(np.float32)
    rr = np.sqrt((xx - tc + 0.5) ** 2 + (yy - tc + 0.5) ** 2)
    pack = aa(t * 0.49 - rr)
    windings = 0.5 + 0.5 * np.sin(rr * 1.9 + noise(t, t, 4) * 3)
    v = 0.10 + 0.035 * windings + 0.03 * noise(t, t, 0.6)
    nd = ((xx - tc) * -0.55 + (yy - tc) * -0.75) / (t * 0.5)
    v = v + 0.10 * (0.5 + 0.5 * np.cos(2 * (np.arctan2(yy - tc, xx - tc) + 2.25))) ** 6 * aa(rr - t * 0.12)
    rgb = v[..., None] * np.array([0.95, 0.70, 0.48])
    save_rgba(rgb, pack, "tape.png")


# --------------------------------------------------------------------------------------------- CRT
def crt(name, gw, gh):
    """Bezel (with the opening for the screen) and a glass overlay drawn on top of the trace."""
    b = 9  # bezel width, GUI px
    pad = 8
    w, h = (gw + 2 * (b + pad)) * S, (gh + 2 * (b + pad)) * S
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    ox0, oy0, ox1, oy1 = pad * S, pad * S, w - pad * S, h - pad * S
    ix0, iy0, ix1, iy1 = (pad + b) * S, (pad + b) * S, w - (pad + b) * S, h - (pad + b) * S
    outer = rrect_sd(xx, yy, ox0, oy0, ox1, oy1, 6 * S)
    inner = rrect_sd(xx, yy, ix0, iy0, ix1, iy1, 10 * S)
    om = np.clip(outer + 0.5, 0, 1)
    im = np.clip(inner + 0.5, 0, 1)
    sh = rrect_sd(xx - 2 * S, yy - 4 * S, ox0, oy0, ox1, oy1, 6 * S)
    shadow = np.clip((sh + 8 * S) / (8 * S), 0, 1) ** 2 * 0.7
    # black wrinkle bezel, bevelled towards the glass
    v = 0.07 + 0.03 * norm(blur(np.abs(noise(h, w, 1.0)), 0.7))
    d_in = np.clip(-inner, 0, None) / S  # distance from the glass edge, outwards
    slope = np.clip(1 - d_in / (b * 0.65), 0, 1)
    nd = np.where(yy < (iy0 + iy1) / 2, 1.0, -1.0) * 0.6 + np.where(xx < (ix0 + ix1) / 2, 0.4, -0.4)
    v = v - 0.05 * slope + 0.05 * slope * np.clip(-nd, 0, 1)
    lip = np.clip(1 - np.abs(outer) / (1.4 * S), 0, 1) * 0.12
    v = v + lip
    rgb = v[..., None] * np.array([1.0, 1.0, 1.0])
    ring = om * (1 - im)
    al = np.maximum(shadow * (1 - om), ring)
    # dark gap right at the glass
    gap = np.clip(1 - np.abs(inner) / (1.5 * S), 0, 1)
    rgb *= (1 - 0.8 * gap)[..., None]
    save_rgba(rgb, al, f"crt_{name}.png")

    # glass: vignette, a curved reflection band, dust, scanlines (very faint)
    gw2, gh2 = gw * S, gh * S
    yy, xx = np.mgrid[0:gh2, 0:gw2].astype(np.float32)
    u, vv = xx / gw2 * 2 - 1, yy / gh2 * 2 - 1
    vig = np.clip((u ** 2) ** 2 * 0.6 + (vv ** 2) ** 2 * 0.7, 0, 1)
    dark = np.clip(vig, 0, 1) * 0.85
    band = np.exp(-((vv + 0.55 - 0.25 * u * u) ** 2) / 0.03) * np.clip(1 - np.abs(u) * 0.9, 0, 1) * 0.10
    spot = np.exp(-(((u + 0.62) ** 2) / 0.02 + ((vv + 0.62) ** 2) / 0.015)) * 0.22
    dust = np.clip(norm(noise(gh2, gw2, 0.7)) * 6 - 5.3, 0, 1) * 0.15
    smudge = np.clip(norm(noise(gh2, gw2, 18)) * 2 - 1.3, 0, 1) * 0.025
    light = band + spot + dust + smudge
    scan = (np.sin(yy * math.pi / 1.5) * 0.5 + 0.5) * 0.10
    # premultiply into one RGBA: dark where vignetted, white where reflecting
    alpha = np.clip(dark + light + scan * 0.6, 0, 1)
    col = np.where(alpha > 1e-4, light / np.maximum(alpha, 1e-4), 0)
    rgb = np.repeat(np.clip(col, 0, 1)[..., None], 3, axis=2) * np.array([0.92, 1.0, 0.95])
    save_rgba(rgb, alpha, f"glass_{name}.png")


# --------------------------------------------------------------------------------------------- toggles, lamps, buttons, jacks
def toggle(state_up):
    w, h = 96, 132
    c = (w / 2, h / 2)
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    rgb = np.zeros((h, w, 3), np.float32)
    al = np.zeros((h, w), np.float32)
    dx, dy = xx - c[0], yy - c[1]
    rr = np.sqrt(dx * dx + dy * dy)
    th = np.arctan2(dy, dx)
    aa = lambda d: np.clip(d, 0, 1)
    sh = np.clip(1 - (np.sqrt((dx - 4) ** 2 + (dy - 6) ** 2) - 18) / 10, 0, 1) ** 2 * 0.6
    al = np.maximum(al, sh)
    hexr = 19 / np.cos(((th + math.pi / 6) % (math.pi / 3)) - math.pi / 6)
    nut = aa(hexr - rr)
    nd = (dx * -0.55 + dy * -0.7) / 19
    nv = 0.45 + 0.25 * np.clip(nd, -1, 1) + 0.04 * noise(h, w, 0.8)
    face_edge = np.clip(1 - (hexr - rr) / 3, 0, 1) * nut
    nv = nv + 0.15 * face_edge * np.sign(nd)
    rgb = rgb * (1 - nut[..., None]) + nv[..., None] * nut[..., None]
    al = np.maximum(al, nut)
    col = aa(11 - rr)
    cv = 0.35 + 0.35 * np.clip(nd, 0, 1) + 0.06 * np.sin(rr * 3)
    rgb = rgb * (1 - col[..., None]) + cv[..., None] * col[..., None]
    s = -1 if state_up else 1
    L = 46
    t = np.clip((dy * s) / L, 0, 1)
    half = 4.0 + 3.5 * t
    inside = aa(half - np.abs(dx)) * aa(dy * s + 2) * aa(L - dy * s)
    tipc = (c[0], c[1] + s * L)
    tip = aa(9.5 - np.sqrt((xx - tipc[0]) ** 2 + (yy - tipc[1]) ** 2))
    lever = np.maximum(inside, tip)
    u = np.clip(dx / (half + 1e-3), -1, 1)
    chrome = 0.35 + 0.55 * np.exp(-((u + 0.35) ** 2) / 0.05) + 0.15 * (1 - np.abs(u)) - 0.2 * (1 - t)
    tu = (xx - tipc[0]) / 9.5
    tv = (yy - tipc[1]) / 9.5
    tipv = 0.45 + 0.5 * np.exp(-(((tu + 0.35) ** 2 + (tv + 0.4) ** 2) / 0.12)) - 0.15 * np.clip(tu + tv, 0, 2)
    lv = np.where(tip > inside, tipv, chrome)
    lsh = np.roll(np.roll(lever, 4, axis=1), 6, axis=0) * 0.5
    al = np.maximum(al, lsh)
    rgb = rgb * (1 - lsh[..., None] * (1 - lever[..., None]))
    rgb = rgb * (1 - lever[..., None]) + (lv[..., None] * np.array([0.98, 0.98, 1.0])) * lever[..., None]
    al = np.maximum(al, lever)
    save_rgba(rgb, al, "toggle_up.png" if state_up else "toggle_down.png", down=2)


def jewel(color, on, name):
    n = 96
    c = n / 2
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    dx, dy = xx - c, yy - c
    rr = np.sqrt(dx * dx + dy * dy)
    aa = lambda d: np.clip(d, 0, 1)
    col = np.array(color, np.float32)
    rgb = np.zeros((n, n, 3), np.float32)
    al = np.zeros((n, n), np.float32)
    sh = np.clip(1 - (np.sqrt((dx - 2) ** 2 + (dy - 4) ** 2) - 24) / 8, 0, 1) ** 2 * 0.6
    al = np.maximum(al, sh)
    bez = aa(26 - rr) * aa(rr - 18)
    nd = (dx * -0.55 + dy * -0.7) / 26
    bv = 0.38 + 0.4 * np.clip(nd, -1, 1) * np.sign(rr - 22)
    rgb = rgb * (1 - bez[..., None]) + bv[..., None] * bez[..., None]
    al = np.maximum(al, bez)
    dome = aa(19 - rr)
    th = np.arctan2(dy, dx)
    facet = 0.85 + 0.15 * np.cos(th * 8)
    inner = (0.55 + 0.45 * np.clip(1 - rr / 19, 0, 1)) * facet
    lvl = 1.0 if on else 0.20
    dc = col * inner[..., None] * lvl
    if on:
        dc = dc + np.clip(1 - rr / 8, 0, 1)[..., None] * 0.6
    spec = np.exp(-(((dx + 6) ** 2 + (dy + 7) ** 2) / 18)) * 0.9
    dc = dc + spec[..., None]
    rgb = rgb * (1 - dome[..., None]) + dc * dome[..., None]
    al = np.maximum(al, dome)
    save_rgba(rgb, al, name, down=2)


def button(on):
    """Rectangular illuminated push button (3-slice: the ends are 12 GUI px)."""
    w, h = 64 * S, 22 * S
    pad = 3 * S
    W2, H2 = w + 2 * pad, h + 2 * pad
    yy, xx = np.mgrid[0:H2, 0:W2].astype(np.float32)
    x0, y0, x1, y1 = pad, pad, pad + w, pad + h
    sd = rrect_sd(xx, yy, x0, y0, x1, y1, 3 * S)
    m = np.clip(sd + 0.5, 0, 1)
    sh = rrect_sd(xx - 1 * S, yy - 2 * S, x0, y0, x1, y1, 3 * S)
    shadow = np.clip((sh + 3 * S) / (3 * S), 0, 1) ** 2 * 0.7
    # black bezel 2px, then the translucent cap
    cap_sd = rrect_sd(xx, yy, x0 + 2 * S, y0 + 2 * S, x1 - 2 * S, y1 - 2 * S, 2 * S)
    cm = np.clip(cap_sd + 0.5, 0, 1)
    v = (yy - y0) / h
    bez = (0.10 + 0.08 * (1 - v))[..., None] * np.ones(3)
    if on:
        capc = np.array([1.0, 0.93, 0.80]) * (0.85 + 0.15 * (1 - v))[..., None]
    else:
        capc = np.array([0.36, 0.34, 0.31]) * (0.70 + 0.35 * (1 - v))[..., None]
    capc = capc + (np.clip(1 - np.abs(v - 0.18) / 0.10, 0, 1) * 0.10)[..., None]
    capc = capc * (1 - 0.15 * np.clip(norm(noise(H2, W2, 2)) * 1.5 - 0.5, 0, 1))[..., None]
    rgb = bez * (1 - cm[..., None]) + capc * cm[..., None]
    rgb = rgb * m[..., None]
    al = np.maximum(shadow * (1 - m), m)
    save_rgba(rgb, al, "btn_on.png" if on else "btn_off.png")


def jack():
    n = 72
    c = n / 2
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    dx, dy = xx - c + 0.5, yy - c + 0.5
    rr = np.sqrt(dx * dx + dy * dy)
    th = np.arctan2(dy, dx)
    aa = lambda d: np.clip(d, 0, 1)
    sh = np.clip(1 - (np.sqrt((dx - 2) ** 2 + (dy - 3) ** 2) - 24) / 7, 0, 1) ** 2 * 0.6
    hexr = 25 / np.cos(((th + math.pi / 6) % (math.pi / 3)) - math.pi / 6)
    nut = aa(hexr - rr)
    nd = (dx * -0.55 + dy * -0.7) / 25
    nv = 0.50 + 0.25 * np.clip(nd, -1, 1) + 0.05 * noise(n, n, 0.6)
    fe = np.clip(1 - (hexr - rr) / 3, 0, 1) * nut
    nv = nv + 0.15 * fe * np.sign(nd)
    hole = aa(10 - rr)
    ring = aa(15 - rr) * (1 - hole)
    rv = 0.30 + 0.35 * np.clip(-nd, 0, 1)
    v = np.where(ring > 0, rv, nv) * (1 - hole) + hole * (0.02 + 0.05 * np.clip(-dy / 10, 0, 1))
    rgb = v[..., None] * np.array([0.98, 0.97, 0.95])
    al = np.maximum(sh, nut)
    save_rgba(rgb, al, "jack.png", down=2)


def plug():
    """A cable plug seen from above (drawn rotated along the cable)."""
    w, h = 40, 96
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    aa = lambda d: np.clip(d, 0, 1)
    dx = xx - w / 2 + 0.5
    body = aa(12 - np.abs(dx)) * aa(yy - 10) * aa(h - 6 - yy)
    sleeve = aa(9 - np.abs(dx)) * aa(10 - yy)
    u = np.clip(dx / 12, -1, 1)
    v = 0.12 + 0.30 * np.exp(-((u + 0.35) ** 2) / 0.05) + 0.05 * np.cos(yy * 0.9) * aa(yy - 40)
    rgb = v[..., None] * np.ones(3)
    sv = 0.45 + 0.45 * np.exp(-((u + 0.3) ** 2) / 0.04)
    rgb = np.where(sleeve[..., None] > 0, sv[..., None] * np.ones(3), rgb)
    al = np.maximum(body, sleeve)
    save_rgba(rgb, al, "plug.png", down=2)


# --------------------------------------------------------------------------------------------- keys and wheels
def keys():
    # white key 30 x 110 GUI px, black 18 x 68
    for down in (False, True):
        w, h = 30 * S, 112 * S
        yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
        sd = rrect_sd(xx, yy, 1 * S, -6 * S, w - 1 * S, h - 1 * S, 3 * S)
        m = np.clip(sd + 0.5, 0, 1)
        ivory = np.array([0.93, 0.89, 0.79])
        v = 0.92 + 0.05 * noise(h, w, 6) + 0.02 * noise(h, w, 0.7)
        v *= 1.0 - 0.10 * (yy / h) if not down else 0.92 - 0.12 * (yy / h)
        # front lip
        lip = np.clip((yy - (h - 9 * S)) / (2 * S), 0, 1)
        v = v * (1 - 0.18 * lip)
        side = np.clip(1 - np.minimum(xx - 1 * S, w - 1 * S - xx) / (2.5 * S), 0, 1)
        v = v * (1 - 0.20 * side)
        # yellowing and finger wear at the playing end
        age = np.clip(norm(noise(h, w, 14)) * 1.4 - 0.5, 0, 1) * np.clip(yy / h * 1.4, 0, 1)
        rgb = ivory * v[..., None]
        rgb = rgb * (1 - 0.10 * age[..., None]) + np.array([0.80, 0.66, 0.40]) * 0.10 * age[..., None]
        rgb = rgb + scratches(h, w, 6, 10 * S, horiz=0.0)[..., None] * 0.05
        if down:
            rgb *= np.clip(0.75 + 0.25 * (yy / h * 3), 0, 1)[..., None] * 0.0 + 1.0
            top_shadow = np.clip(1 - yy / (14 * S), 0, 1) * 0.35
            rgb *= (1 - top_shadow)[..., None]
        save_rgba(rgb, m, "key_white_down.png" if down else "key_white.png")

        w, h = 18 * S, 70 * S
        yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
        sd = rrect_sd(xx, yy, 0, -6 * S, w, h, 2.5 * S)
        m = np.clip(sd + 0.5, 0, 1)
        sh = rrect_sd(xx, yy, 0, -6 * S, w, h, 2.5 * S)
        top_w = 2.5 * S
        inner = rrect_sd(xx, yy, top_w, -6 * S, w - top_w, h - (5 if not down else 3) * S, 2 * S)
        im = np.clip(inner + 0.5, 0, 1)
        v_top = 0.07 + 0.05 * (1 - yy / h) + 0.02 * noise(h, w, 0.8)
        v_side = 0.03 + 0.06 * np.clip((xx - w / 2) / w * -2, 0, 1)
        v = np.where(im > 0.5, v_top, v_side)
        gloss = np.exp(-((xx - w * 0.33) ** 2) / (0.006 * w * w * 8)) * 0.10 * im * (1 - yy / h)
        v = v + gloss
        if down:
            v *= 0.8
        rgb = v[..., None] * np.array([1.0, 0.97, 0.96])
        save_rgba(rgb, m, "key_black_down.png" if down else "key_black.png")


def wheel():
    # the slot (static) and the rubber wheel surface (scrolled by uv)
    w, h = 26 * S, 92 * S
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    sd = rrect_sd(xx, yy, 0, 0, w, h, 4 * S)
    m = np.clip(sd + 0.5, 0, 1)
    inner = np.clip(-sd + 4 * S, 0, 4 * S) / (4 * S)
    v = 0.02 + 0.02 * (1 - inner)
    save_rgba(v[..., None] * np.ones(3), m, "wheel_slot.png")
    # rubber: ribs across, a curved cylinder shading (applied at run time); the texture tiles in v
    w2, h2 = 20 * S, 128 * S
    yy, xx = np.mgrid[0:h2, 0:w2].astype(np.float32)
    ribs = 0.5 + 0.5 * np.cos(yy / h2 * 2 * math.pi * 24)
    v = 0.10 + 0.08 * ribs + 0.02 * noise(h2, w2, 0.6)
    u = xx / w2 * 2 - 1
    v = v * (1 - 0.35 * u * u)
    rgb = v[..., None] * np.array([1.0, 0.98, 0.97])
    save_rgb(np.clip(rgb, 0, 1), "wheel_rubber.png")
    # cylinder light (static overlay)
    w3, h3 = 20 * S, 84 * S
    yy, xx = np.mgrid[0:h3, 0:w3].astype(np.float32)
    vv = yy / h3 * 2 - 1
    dark = np.clip(np.abs(vv) ** 2.2, 0, 1) * 0.9
    light = np.exp(-((vv + 0.35) ** 2) / 0.03) * 0.25
    a = np.clip(dark + light, 0, 1)
    col = np.where(a > 1e-4, light / np.maximum(a, 1e-4), 0)
    save_rgba(np.repeat(col[..., None], 3, axis=2), a, "wheel_light.png")


def glow():
    n = 128
    c = n / 2
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    rr = np.sqrt((xx - c + 0.5) ** 2 + (yy - c + 0.5) ** 2) / c
    a = np.clip(1 - rr, 0, 1) ** 2.4
    save_rgba(np.ones((n, n, 3), np.float32), a, "glow.png")


def vu_face():
    w, h = 104 * S, 52 * S
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    paper = np.array([0.95, 0.87, 0.66], np.float32)
    v = 1.0 + 0.025 * noise(h, w, 0.6) + 0.04 * noise(h, w, 12)
    cx, cy = w / 2, h * 1.05
    d = np.sqrt(((xx - cx) / w) ** 2 + ((yy - cy) / h) ** 2)
    v *= 1.10 - 0.55 * d ** 1.6
    age = np.clip(norm(noise(h, w, 15)) * 1.3 - 0.4, 0, 1)
    rgb = paper * v[..., None]
    rgb = rgb * (1 - 0.14 * age[..., None]) + np.array([0.72, 0.52, 0.24]) * 0.14 * age[..., None]
    save_rgb(np.clip(rgb, 0, 1), "vu_face.jpg", q=90)


def badge():
    """Engraved brushed-aluminium name badge with black enamel letters, two rivets."""
    W2, H2 = 232 * S, 40 * S
    pad = 5 * S
    w, h = W2 + 2 * pad, H2 + 2 * pad
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    x0, y0, x1, y1 = pad, pad, pad + W2, pad + H2
    sd = rrect_sd(xx, yy, x0, y0, x1, y1, 3 * S)
    body = np.clip(sd + 0.5, 0, 1)
    shd = rrect_sd(xx - 2 * S, yy - 3 * S, x0, y0, x1, y1, 3 * S)
    shadow = np.clip((shd + 5 * S) / (5 * S), 0, 1) ** 2 * 0.6
    g = RNG.standard_normal((h, w)).astype(np.float32)
    k = 50
    cs = np.cumsum(np.pad(g, ((0, 0), (k, k)), mode="wrap"), axis=1)
    g = (cs[:, 2 * k:] - cs[:, :-2 * k])[:, :w] / (2 * k)
    brushed = g * 3 + RNG.standard_normal((h, 1)).astype(np.float32) * 0.4
    v = 0.64 + 0.035 * brushed + 0.10 * (1 - (yy - y0) / H2)
    v += 0.08 * np.exp(-((xx - x0 - W2 * 0.3) ** 2) / (0.02 * W2 * W2))
    bev = np.clip(1 - sd / (1.6 * S), 0, 1) * body
    v = v + bev * np.where(yy < (y0 + y1) / 2, 0.2, -0.25)
    # letters (engraved: dark fill, light lower edge)
    txt = Image.new("L", (w, h), 0)
    dr = ImageDraw.Draw(txt)
    f1 = font("anton/Anton-Regular.ttf", 27 * S)
    dr.text((x0 + 14 * S, y0 + 2 * S), "MAXO", font=f1, fill=255)
    mw = dr.textlength("MAXO", font=f1)
    f2 = font("anton/Anton-Regular.ttf", 27 * S)
    dr.text((x0 + 14 * S + mw + 6 * S, y0 + 2 * S), "GRANULAR", font=f2, fill=150)
    gw_ = dr.textlength("GRANULAR", font=f2)
    f3 = font("DejaVuSansCondensed-Bold.ttf", 7 * S)
    t = np.asarray(txt, dtype=np.float32) / 255.0
    full = (t > 0.0).astype(np.float32) * np.clip(t * 4, 0, 1)
    second = np.clip((0.75 - np.abs(t - 150 / 255) * 8), 0, 1)
    fill_dark = full
    hi = np.clip(np.roll(full, -S, 0) - full, 0, 1)
    lo = np.clip(np.roll(full, S, 0) - full, 0, 1)
    rgb = v[..., None] * np.array([0.95, 0.96, 0.98])
    enamel = np.where(second[..., None] > 0.3, np.array([0.70, 0.12, 0.08]), np.array([0.05, 0.05, 0.06]))
    rgb = rgb * (1 - fill_dark[..., None]) + enamel * fill_dark[..., None]
    rgb = rgb + hi[..., None] * 0.25 - lo[..., None] * 0.2
    # wear on the enamel
    wear = np.clip(norm(noise(h, w, 2)) * 3 - 2.2, 0, 1) * fill_dark
    rgb = rgb * (1 - wear[..., None]) + (v[..., None] * 0.9) * wear[..., None]
    # rivets
    for sx in (x0 + 6 * S, x1 - 6 * S):
        sy = (y0 + y1) / 2
        dd = np.sqrt((xx - sx) ** 2 + (yy - sy) ** 2)
        head = np.clip(2.6 * S - dd + 0.5, 0, 1)
        nd = ((xx - sx) * -0.55 + (yy - sy) * -0.7) / (2.6 * S)
        hv = 0.55 + 0.35 * np.clip(nd, -1, 1)
        rgb = rgb * (1 - head[..., None]) + hv[..., None] * head[..., None]
    rgb += RNG.standard_normal((h, w, 1)).astype(np.float32) * 0.01
    sc = scratches(h, w, 25, 20 * S)
    rgb += sc[..., None] * 0.12
    rgb = rgb * body[..., None]
    save_rgba(rgb, np.maximum(body, shadow), "badge.png")


if __name__ == "__main__":
    import sys
    if len(sys.argv) > 1:
        for f in sys.argv[1:]:
            globals()[f]()
        sys.exit(0)
    faceplate()
    knob()
    reel()
    for k, (gw, gh) in CRTS.items():
        crt(k, gw, gh)
    toggle(True)
    toggle(False)
    for nm, col in [("amber", (1.0, 0.62, 0.15)), ("green", (0.35, 1.0, 0.45)), ("red", (1.0, 0.18, 0.12))]:
        jewel(col, True, f"lamp_{nm}_on.png")
        jewel(col, False, f"lamp_{nm}_off.png")
    button(True)
    button(False)
    jack()
    plug()
    keys()
    wheel()
    glow()
    vu_face()
    badge()
    for f in sorted(os.listdir(OUT)):
        print(f, os.path.getsize(os.path.join(OUT, f)))

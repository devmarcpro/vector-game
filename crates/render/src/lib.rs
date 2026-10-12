//! CPU line rasteriser, projection, clipping and bitmap text.
pub mod font;
pub mod png;

use vector_core::{Mat3, Vec3};

/// A colour (0x00RRGGBB) with every channel scaled by `k` (clamped): shading and dimming.
pub fn scale_color(c: u32, k: f32) -> u32 {
    let ch = |s: u32| (((c >> s) & 0xFF) as f32 * k).clamp(0.0, 255.0) as u32;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// Linear blend of two colours: `a` at t = 0, `b` at t = 1.
pub fn mix_color(a: u32, b: u32, t: f32) -> u32 {
    let ch = |c: u32, s: u32| ((c >> s) & 0xFF) as f32;
    let m = |s: u32| (ch(a, s) * (1.0 - t) + ch(b, s) * t).clamp(0.0, 255.0) as u32;
    (m(16) << 16) | (m(8) << 8) | m(0)
}

pub struct Framebuffer {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u32>,
    /// Texts that ran off the right edge of the screen (a layout test reads it).
    pub clipped: usize,
    /// The first of them, for the report.
    pub clipped_first: Option<String>,
}

impl Framebuffer {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![0; w * h],
            clipped: 0,
            clipped_first: None,
        }
    }
    pub fn clear(&mut self) {
        self.px.fill(0);
    }
    /// Additive plot (phosphor look), saturating per channel.
    #[inline]
    /// Overwrite one pixel (no blending); out-of-range coordinates are ignored.
    pub fn px_set(&mut self, x: i32, y: i32, c: u32) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = c;
        }
    }

    pub fn plot(&mut self, x: i32, y: i32, c: u32) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        let p = &mut self.px[y as usize * self.w + x as usize];
        let add = |a: u32, b: u32| (a + b).min(255);
        let r = add((*p >> 16) & 255, (c >> 16) & 255);
        let g = add((*p >> 8) & 255, (c >> 8) & 255);
        let b = add(*p & 255, c & 255);
        *p = (r << 16) | (g << 8) | b;
    }
    /// Bresenham line, clipped to the framebuffer.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: u32) {
        let Some((x0, y0, x1, y1)) =
            clip_screen(x0, y0, x1, y1, self.w as f32 - 1.0, self.h as f32 - 1.0)
        else {
            return;
        };
        let (mut x, mut y) = (x0.round() as i32, y0.round() as i32);
        let (xe, ye) = (x1.round() as i32, y1.round() as i32);
        let dx = (xe - x).abs();
        let dy = -(ye - y).abs();
        let (sx, sy) = (if x < xe { 1 } else { -1 }, if y < ye { 1 } else { -1 });
        let mut err = dx + dy;
        loop {
            self.plot(x, y, c);
            if x == xe && y == ye {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }
    pub fn text(&mut self, col: usize, row: usize, s: &str, c: u32) {
        let cols = self.cols();
        if s.chars().skip(cols.saturating_sub(col)).any(|ch| ch != ' ') {
            self.clipped += 1;
            if self.clipped_first.is_none() {
                self.clipped_first = Some(format!("{col},{row}: {s}"));
            }
        }
        for (i, ch) in s.chars().enumerate() {
            let g = font::glyph(ch);
            let (ox, oy) = ((col + i) * font::CELL_W, row * font::CELL_H);
            for (cx, bits) in g.iter().enumerate() {
                for cy in 0..7 {
                    if bits >> cy & 1 == 1 {
                        self.plot((ox + cx) as i32, (oy + cy) as i32, c);
                    }
                }
            }
            if let Some((_, mark)) = font::decompose(ch) {
                for &(mx, my) in mark.pixels() {
                    self.plot(ox as i32 + mx, oy as i32 + my, c);
                }
            }
        }
    }
    /// Scaled text (each font pixel becomes a `scale` x `scale` block), for titles.
    pub fn text_big(&mut self, x: i32, y: i32, s: &str, scale: i32, c: u32) {
        for (i, ch) in s.chars().enumerate() {
            let g = font::glyph(ch);
            let ox = x + i as i32 * font::CELL_W as i32 * scale;
            for (cx, bits) in g.iter().enumerate() {
                for cy in 0..7 {
                    if bits >> cy & 1 == 1 {
                        for dx in 0..scale {
                            for dy in 0..scale {
                                self.plot(ox + cx as i32 * scale + dx, y + cy * scale + dy, c);
                            }
                        }
                    }
                }
            }
        }
    }
    /// Like `text_big` but opaque: the pixels of the glyph replace what is under them instead of
    /// adding to it (billboards drawn over a bright ground).
    pub fn text_big_over(&mut self, x: i32, y: i32, s: &str, scale: i32, c: u32) {
        for (i, ch) in s.chars().enumerate() {
            let g = font::glyph(ch);
            let ox = x + i as i32 * font::CELL_W as i32 * scale;
            for (cx, bits) in g.iter().enumerate() {
                for cy in 0..7 {
                    if bits >> cy & 1 == 1 {
                        for dx in 0..scale {
                            for dy in 0..scale {
                                self.px_set(ox + cx as i32 * scale + dx, y + cy * scale + dy, c);
                            }
                        }
                    }
                }
            }
        }
    }
    /// Fill a rectangle, opaque.
    pub fn rect_over(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.px_set(xx, yy, c);
            }
        }
    }
    /// Outline of the rectangle from (l, t) to (r, b) in pixels (lines, additive like `line`).
    pub fn outline<T: Into<f64>>(&mut self, l: T, t: T, r: T, b: T, c: u32) {
        let (l, t, r, b) = (
            l.into() as f32,
            t.into() as f32,
            r.into() as f32,
            b.into() as f32,
        );
        self.line(l, t, r, t, c);
        self.line(r, t, r, b, c);
        self.line(r, b, l, b, c);
        self.line(l, b, l, t, c);
    }
    /// Ellipse of radii (rx, ry) around (cx, cy), 48 segments.
    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, c: u32) {
        let mut prev = (cx + rx, cy);
        for i in 1..=48 {
            let a = i as f32 / 48.0 * std::f32::consts::TAU;
            let p = (cx + rx * a.cos(), cy + ry * a.sin());
            self.line(prev.0, prev.1, p.0, p.1, c);
            prev = p;
        }
    }
    /// Text that is cut with "..." where it would run off the right edge of the screen.
    pub fn text_fit(&mut self, col: usize, row: usize, s: &str, c: u32) {
        let room = self.cols().saturating_sub(col + 1);
        if s.chars().count() <= room {
            self.text(col, row, s, c);
        } else {
            let t: String = s.chars().take(room.saturating_sub(3)).collect();
            self.text(col, row, &format!("{t}..."), c);
        }
    }

    /// Text at an arbitrary pixel position.
    pub fn text_at(&mut self, x: i32, y: i32, s: &str, c: u32) {
        for (i, ch) in s.chars().enumerate() {
            let g = font::glyph(ch);
            let ox = x + (i * font::CELL_W) as i32;
            for (cx, bits) in g.iter().enumerate() {
                for cy in 0..7 {
                    if bits >> cy & 1 == 1 {
                        self.plot(ox + cx as i32, y + cy, c);
                    }
                }
            }
            if let Some((_, mark)) = font::decompose(ch) {
                for &(mx, my) in mark.pixels() {
                    self.plot(ox + mx, y + my, c);
                }
            }
        }
    }
    /// Screen post-processing: palette tint, glow bleed and CRT scanlines.
    /// `tint` is an optional (r,g,b) multiplier in 0..=255.
    pub fn postfx(&mut self, scanlines: bool, glow: bool, tint: Option<(u32, u32, u32)>) {
        let w = self.w;
        if glow {
            let src = self.px.clone();
            for y in 1..self.h.saturating_sub(1) {
                for x in 1..w.saturating_sub(1) {
                    let c = src[y * w + x];
                    if c & 0xFF_FFFF == 0 {
                        continue;
                    }
                    for (dx, dy) in [(1usize, 0usize), (0, 1)] {
                        for (nx, ny) in [(x + dx, y + dy), (x - dx, y - dy)] {
                            let i = ny * w + nx;
                            let d = self.px[i];
                            let add = |sh: u32| {
                                (((d >> sh) & 0xFF) + (((c >> sh) & 0xFF) >> 2)).min(255) << sh
                            };
                            self.px[i] = add(16) | add(8) | add(0);
                        }
                    }
                }
            }
        }
        for (i, p) in self.px.iter_mut().enumerate() {
            let mut c = *p;
            if let Some((r, g, b)) = tint {
                let lum =
                    (((c >> 16) & 0xFF) * 77 + ((c >> 8) & 0xFF) * 150 + (c & 0xFF) * 29) >> 8;
                c = ((lum * r / 255) << 16) | ((lum * g / 255) << 8) | (lum * b / 255);
            }
            if scanlines && (i / w) % 2 == 1 {
                c = (c >> 1) & 0x7F_7F7F;
            }
            *p = c;
        }
    }

    pub fn cols(&self) -> usize {
        self.w / font::CELL_W
    }
    pub fn rows(&self) -> usize {
        self.h / font::CELL_H
    }
}

/// Liang-Barsky clip of a 2D segment to [0,maxx]x[0,maxy].
pub fn clip_screen(
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    maxx: f32,
    maxy: f32,
) -> Option<(f32, f32, f32, f32)> {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [(-dx, x0), (dx, maxx - x0), (-dy, y0), (dy, maxy - y0)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return None;
                }
                t0 = t0.max(r);
            } else {
                if r < t0 {
                    return None;
                }
                t1 = t1.min(r);
            }
        }
    }
    Some((x0 + t0 * dx, y0 + t0 * dy, x0 + t1 * dx, y0 + t1 * dy))
}

/// Perspective camera looking down +Z in view space.
pub struct Camera {
    pub pos: Vec3,
    pub rot: Mat3, // world -> view
    pub fov_y: f64,
}

pub const NEAR: f64 = 0.1;

impl Camera {
    pub fn to_view(&self, p: Vec3) -> Vec3 {
        self.rot.mul_vec(p - self.pos)
    }
    fn project(&self, v: Vec3, fb: &Framebuffer) -> (f32, f32) {
        let f = (fb.h as f64 * 0.5) / (self.fov_y * 0.5).tan();
        (
            (fb.w as f64 * 0.5 + v.x / v.z * f) as f32,
            (fb.h as f64 * 0.5 - v.y / v.z * f) as f32,
        )
    }
    /// Draw a world-space segment with near-plane clipping.
    pub fn draw_line(&self, fb: &mut Framebuffer, a: Vec3, b: Vec3, c: u32) {
        let (mut va, mut vb) = (self.to_view(a), self.to_view(b));
        if va.z < NEAR && vb.z < NEAR {
            return;
        }
        if va.z < NEAR {
            let t = (NEAR - va.z) / (vb.z - va.z);
            va = va + (vb - va) * t;
        } else if vb.z < NEAR {
            let t = (NEAR - vb.z) / (va.z - vb.z);
            vb = vb + (va - vb) * t;
        }
        let (x0, y0) = self.project(va, fb);
        let (x1, y1) = self.project(vb, fb);
        fb.line(x0, y0, x1, y1, c);
    }
}

impl Camera {
    /// Project a view-space point; None if behind the near plane.
    pub fn project_view(&self, v: Vec3, fb: &Framebuffer) -> Option<(f32, f32)> {
        (v.z > NEAR).then(|| self.project(v, fb))
    }
    /// Project a world point to screen; None if behind the camera.
    pub fn project_world(&self, p: Vec3, fb: &Framebuffer) -> Option<(f32, f32)> {
        self.project_view(self.to_view(p), fb)
    }
    /// Infinitely distant point (stars): only the rotation matters.
    pub fn project_dir(&self, dir: Vec3, fb: &Framebuffer) -> Option<(f32, f32)> {
        self.project_view(self.rot.mul_vec(dir), fb)
    }

    /// Wireframe sphere: limb circle plus front-facing latitude/longitude lines.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_sphere(
        &self,
        fb: &mut Framebuffer,
        center: Vec3,
        radius: f64,
        spin: &Mat3,
        lat: usize,
        lon: usize,
        c: u32,
    ) {
        let to_cam = self.pos - center;
        let d = to_cam.length();
        if d <= radius {
            return;
        }
        let u = to_cam * (1.0 / d);
        // Limb: circle of tangent points.
        let helper = if u.x.abs() < 0.9 {
            Vec3::new(1.0, 0.0, 0.0)
        } else {
            Vec3::new(0.0, 1.0, 0.0)
        };
        let e1 = u.cross(helper).normalized();
        let e2 = u.cross(e1);
        let lc = center + u * (radius * radius / d);
        let lr = radius * (1.0 - (radius / d).powi(2)).sqrt();
        const SEG: usize = 64;
        let mut prev = lc + e1 * lr;
        for i in 1..=SEG {
            let a = i as f64 / SEG as f64 * std::f64::consts::TAU;
            let p = lc + e1 * (lr * a.cos()) + e2 * (lr * a.sin());
            self.draw_line(fb, prev, p, c);
            prev = p;
        }
        let thresh = radius / d;
        let visible = |dir: Vec3| dir.dot(u) > thresh;
        let pt = |dir: Vec3| center + dir * radius;
        const STEPS: usize = 32;
        // Latitudes.
        for i in 1..lat {
            let phi = std::f64::consts::PI * i as f64 / lat as f64 - std::f64::consts::FRAC_PI_2;
            let mut prev: Option<Vec3> = None;
            for j in 0..=STEPS {
                let th = std::f64::consts::TAU * j as f64 / STEPS as f64;
                let dir = spin.mul_vec(Vec3::new(
                    phi.cos() * th.cos(),
                    phi.sin(),
                    phi.cos() * th.sin(),
                ));
                if let Some(p) = prev {
                    if visible(dir) && visible(p) {
                        self.draw_line(fb, pt(p), pt(dir), c);
                    }
                }
                prev = Some(dir);
            }
        }
        // Longitudes.
        for i in 0..lon {
            let th = std::f64::consts::TAU * i as f64 / lon as f64;
            let mut prev: Option<Vec3> = None;
            for j in 0..=STEPS {
                let phi =
                    std::f64::consts::PI * j as f64 / STEPS as f64 - std::f64::consts::FRAC_PI_2;
                let dir = spin.mul_vec(Vec3::new(
                    phi.cos() * th.cos(),
                    phi.sin(),
                    phi.cos() * th.sin(),
                ));
                if let Some(p) = prev {
                    if visible(dir) && visible(p) {
                        self.draw_line(fb, pt(p), pt(dir), c);
                    }
                }
                prev = Some(dir);
            }
        }
    }
}

impl Framebuffer {
    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, c: u32) {
        let n = ((r * 2.0) as usize).clamp(12, 64);
        let mut prev = (cx + r, cy);
        for i in 1..=n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            let p = (cx + r * a.cos(), cy + r * a.sin());
            self.line(prev.0, prev.1, p.0, p.1, c);
            prev = p;
        }
    }
    /// Horizontal gauge in text cells: `[#####-----]`.
    pub fn gauge(&mut self, col: usize, row: usize, label: &str, frac: f64, width: usize, c: u32) {
        let filled = (frac.clamp(0.0, 1.0) * width as f64).round() as usize;
        let s = format!(
            "{label} [{}{}]",
            "#".repeat(filled),
            "-".repeat(width - filled)
        );
        self.text(col, row, &s, c);
    }
}

pub struct Wireframe {
    pub verts: Vec<Vec3>,
    pub edges: Vec<(usize, usize)>,
}

impl Wireframe {
    pub fn cube(s: f64) -> Self {
        let mut verts = Vec::new();
        for i in 0..8 {
            verts.push(Vec3::new(
                if i & 1 == 0 { -s } else { s },
                if i & 2 == 0 { -s } else { s },
                if i & 4 == 0 { -s } else { s },
            ));
        }
        let mut edges = Vec::new();
        for i in 0..8usize {
            for b in [1, 2, 4] {
                if i & b == 0 {
                    edges.push((i, i | b));
                }
            }
        }
        Self { verts, edges }
    }
    /// A small angular spacecraft pointing along +Z, 1 unit long.
    pub fn ship() -> Self {
        let v = |x: f64, y: f64, z: f64| Vec3::new(x, y, z);
        Self {
            verts: vec![
                v(0.0, 0.0, 1.0),
                v(-0.5, 0.0, -0.6),
                v(0.5, 0.0, -0.6),
                v(0.0, 0.25, -0.5),
                v(0.0, -0.25, -0.5),
            ],
            edges: vec![
                (0, 1),
                (0, 2),
                (0, 3),
                (0, 4),
                (1, 3),
                (2, 3),
                (1, 4),
                (2, 4),
                (1, 2),
            ],
        }
    }

    pub fn draw(&self, cam: &Camera, fb: &mut Framebuffer, rot: &Mat3, at: Vec3, c: u32) {
        for &(a, b) in &self.edges {
            let pa = rot.mul_vec(self.verts[a]) + at;
            let pb = rot.mul_vec(self.verts[b]) + at;
            cam.draw_line(fb, pa, pb, c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clip_rejects_outside() {
        assert!(clip_screen(-10.0, -10.0, -5.0, -1.0, 100.0, 100.0).is_none());
        assert!(clip_screen(-10.0, 50.0, 200.0, 50.0, 100.0, 100.0).is_some());
    }
    #[test]
    fn png_has_signature() {
        let p = png::encode(2, 2, &[0, 0xFF0000, 0x00FF00, 0xFFFFFF]);
        assert_eq!(&p[1..4], b"PNG");
    }
}

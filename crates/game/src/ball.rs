//! The lit ball of a world, fast. The ground of a world (colour of its cells warped by noise,
//! detailed relief, slope against the light) does not change from one image to the next, so it is
//! baked once into an equirectangular texture; the clouds are another texture, rebuilt only when
//! they have drifted. Each image then only casts a ray per pixel, reads the textures (bilinear),
//! and lights the point: the Lambert term, the terminator and the rim of air. The pixels are
//! painted in bands of rows on all the cores (std::thread::scope); every pixel depends on nothing
//! but itself, so the picture is the same for any number of threads.
use crate::globe::{noise2, Paint};
use crate::terrain::Terrain;
use std::cell::RefCell;
use std::f32::consts::{FRAC_PI_2, PI};
use vector_core::{Mat3, Vec3};
use vector_render::{Camera, Framebuffer};
use vector_world::surface::{H, W};

const TAU: f64 = std::f64::consts::TAU;

/// One texel of the baked ground: the cell of the map it shows (and a water override), the slope
/// of the ground east and north (already scaled for the light), how much of the texel is land and
/// the shade of its height, and the variation from place to place.
#[derive(Clone, Copy, Default)]
struct Texel {
    se: f32,
    sn: f32,
    land: f32,
    ht: f32,
    var: f32,
    /// Cell index (13 bits) | override (2 bits: 1 sea, 2 sand) << 13.
    cell: u16,
}

/// How many threads to paint with (0: all the cores). Tests set 1 and N to compare.
static THREADS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

thread_local! {
    /// Override for the calling thread only (tests run side by side and must not change each
    /// other's count).
    static LOCAL_THREADS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Set the count for the whole program (the `--threads` flag of the benchmarks).
pub fn set_threads(n: usize) {
    THREADS.store(n, std::sync::atomic::Ordering::Relaxed);
}

/// Set the count for the calling thread only; 0: back to the program's.
#[cfg(test)]
pub fn set_local_threads(n: usize) {
    LOCAL_THREADS.with(|t| t.set(n));
}

pub fn threads() -> usize {
    let local = LOCAL_THREADS.with(std::cell::Cell::get);
    match if local > 0 {
        local
    } else {
        THREADS.load(std::sync::atomic::Ordering::Relaxed)
    } {
        0 => std::thread::available_parallelism().map_or(1, |n| n.get().min(16)),
        n => n,
    }
}

/// Run `f(first_row, rows)` over the pieces of `data` (cut in pieces of a few rows of `row_len`
/// each), on all the threads: a thread that is done takes the next piece, so the cost that is not
/// the same everywhere is shared out. Each piece is written by one call only and depends on
/// nothing but itself, so the result is the same for any number of threads.
pub fn bands<T: Send>(
    data: &mut [T],
    row_len: usize,
    rows: usize,
    f: impl Fn(usize, &mut [T]) + Sync,
) {
    let n = threads().min(rows).max(1);
    if n == 1 || row_len == 0 {
        f(0, data);
        return;
    }
    let grain = (rows / (n * 6)).max(1);
    let queue = std::sync::Mutex::new(
        data.chunks_mut(grain * row_len)
            .enumerate()
            .map(|(i, c)| (i * grain, c)),
    );
    let work = || loop {
        let next = queue.lock().map(|mut q| q.next()).unwrap_or(None);
        match next {
            Some((y0, piece)) => f(y0, piece),
            None => break,
        }
    };
    std::thread::scope(|s| {
        for _ in 1..n {
            s.spawn(work);
        }
        work();
    });
}

fn fast_atan2(y: f32, x: f32) -> f32 {
    let (ax, ay) = (x.abs(), y.abs());
    let (mn, mx) = if ax < ay { (ax, ay) } else { (ay, ax) };
    let a = if mx == 0.0 { 0.0 } else { mn / mx };
    let s = a * a;
    let mut r = ((-0.046_496_47 * s + 0.159_314_22) * s - 0.327_622_77) * s * a + a;
    if ay > ax {
        r = FRAC_PI_2 - r;
    }
    if x < 0.0 {
        r = PI - r;
    }
    if y < 0.0 {
        -r
    } else {
        r
    }
}

fn unpack(c: u32) -> [f32; 3] {
    [
        ((c >> 16) & 255) as f32,
        ((c >> 8) & 255) as f32,
        (c & 255) as f32,
    ]
}

fn pack(c: [f32; 3]) -> u32 {
    let q = |v: f32| (v.clamp(0.0, 255.0)) as u32;
    (q(c[0]) << 16) | (q(c[1]) << 8) | q(c[2])
}

/// Sizes of the baked textures, from the coarsest: a ball is given the one that matches how many
/// pixels it covers (a texel about one to two pixels across at the middle of the ball).
const SIZES: [(usize, usize); 5] = [(64, 32), (128, 64), (256, 128), (512, 256), (1024, 512)];

fn level_for(r_px: f64) -> usize {
    match r_px {
        r if r < 14.0 => 0,
        r if r < 45.0 => 1,
        r if r < 100.0 => 2,
        r if r < 260.0 => 3,
        _ => 4,
    }
}

/// Bake the geometry of the ground (everything that does not depend on the colours of the day)
/// for the rows `r0..` of a texture `w` by `h`, into `rows` (a whole number of rows).
fn bake_rows(
    terrain: Option<&Terrain>,
    radius: f64,
    w: usize,
    h: usize,
    r0: usize,
    rows: &mut [Texel],
) {
    let px_m = radius * std::f64::consts::PI / h as f64;
    bands(rows, w, rows.len() / w, |y0, part| {
        for (j, row) in part.chunks_mut(w).enumerate() {
            let ty = r0 + y0 + j;
            let lat = (0.5 - (ty as f64 + 0.5) / h as f64) * std::f64::consts::PI;
            for (tx, t) in row.iter_mut().enumerate() {
                let lon = (tx as f64 + 0.5) / w as f64 * TAU;
                let fx = lon / TAU * W as f64 - 0.5;
                let fy = (0.5 - lat / std::f64::consts::PI) * H as f64 - 0.5;
                // Borders between biomes are not squares: the lookup is warped by noise.
                let warp = (
                    noise2(fx * 2.3, fy * 2.3) * 0.9,
                    noise2(fx * 2.3 + 40.0, fy * 2.3) * 0.9,
                );
                let (wxi, wyi) = (
                    ((fx + f64::from(warp.0)).round() as i64).rem_euclid(W as i64) as usize,
                    ((fy + f64::from(warp.1)).round() as i64).clamp(0, H as i64 - 1) as usize,
                );
                let mut cell = (wyi * W + wxi) as u16;
                *t = Texel {
                    var: 1.0,
                    ..Texel::default()
                };
                if let Some(terr) = terrain {
                    let (hc, e) = terr.sample(lat, lon, px_m);
                    let delta = px_m * 1.2 / radius;
                    let (he, _) = terr.sample(lat, lon + delta / lat.cos().abs().max(0.05), px_m);
                    let (hn, _) = terr.sample(lat + delta, lon, px_m);
                    if hc <= 0.0 && e > 0.0 {
                        cell |= 1 << 13;
                    } else if hc > 0.0 && e <= 0.0 {
                        cell |= 2 << 13;
                    }
                    if hc > 0.0 {
                        t.land = 1.0;
                        t.se = (f64::from(he - hc) / (px_m * 1.2) * 9.0) as f32;
                        t.sn = (f64::from(hn - hc) / (px_m * 1.2) * 9.0) as f32;
                        t.ht = (hc / terr.relief as f32).clamp(0.0, 1.0) * 0.3;
                        t.var =
                            1.0 + 0.12 * noise2(lon * radius / 30_000.0, lat * radius / 30_000.0);
                    } else {
                        t.var = 1.0 + 0.06 * e.clamp(-1.0, 0.0);
                    }
                }
                t.cell = cell;
            }
        }
    });
}

/// Colours of the texels from the colours of the cells (cheap: no terrain).
fn recolor(table: &[u32], wet: bool, texels: &[Texel], albedo: &mut Vec<u32>) {
    let sea = 0x0020_60A0;
    let sand = vector_render::scale_color(0x00C8_B880, 0.9);
    albedo.clear();
    albedo.extend(texels.iter().map(|t| match (t.cell >> 13, wet) {
        (1, true) => sea,
        (2, true) => sand,
        _ => table[usize::from(t.cell & 0x1FFF)],
    }));
}

fn build_clouds(f: &(dyn Fn(f64, f64) -> f32 + Sync), w: usize, h: usize) -> Vec<u8> {
    let mut cover = vec![0u8; w * h];
    bands(&mut cover, w, h, |y0, rows| {
        for (j, row) in rows.chunks_mut(w).enumerate() {
            let lat = (0.5 - ((y0 + j) as f64 + 0.5) / h as f64) * std::f64::consts::PI;
            for (x, c) in row.iter_mut().enumerate() {
                let lon = (x as f64 + 0.5) / w as f64 * TAU;
                *c = (f(lat, lon).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        }
    });
    cover
}

/// Lookup of `x^0.8` on [0, 1].
fn pow08(x: f32) -> f32 {
    static T: std::sync::OnceLock<[f32; 1025]> = std::sync::OnceLock::new();
    let t = T.get_or_init(|| {
        let mut t = [0.0; 1025];
        for (i, v) in t.iter_mut().enumerate() {
            *v = (i as f32 / 1024.0).powf(0.8);
        }
        t
    });
    let f = x.clamp(0.0, 1.0) * 1024.0;
    let i = (f as usize).min(1023);
    t[i] + (t[i + 1] - t[i]) * (f - i as f32)
}

/// What the pixel pass reads.
struct Shared<'a> {
    tex: &'a Level,
    cloud: Option<&'a CloudTex>,
    /// Columns of the matrix from the screen (right, up, forward) to the frame of the world.
    a0: Vec3,
    a1: Vec3,
    a2: Vec3,
    /// The sun in the frame of the world (the world turns: all the work is done in it).
    sun_d: Vec3,
    sun_b: [f32; 3],
    /// The eye relative to the centre, in the frame of the world.
    oc: Vec3,
    radius: f64,
    focal: f64,
    c: (f64, f64),
    sc: (f64, f64),
    reach: f64,
    step: i32,
    x_lo: i32,
    x_hi: i32,
    atmosphere: bool,
    air: f64,
}

/// Corners and fractions of a bilinear read at (u, v) in [0, 1) of a texture whose width is a
/// power of two (it wraps in longitude, it stops at the poles): no division, no floor.
#[inline]
fn bilerp(w: usize, h: usize, u: f32, v: f32) -> (usize, usize, usize, usize, f32, f32) {
    let fx = u * w as f32 - 0.5;
    let fy = (v * h as f32 - 0.5).clamp(0.0, h as f32 - 1.0);
    // fx is in [-0.5, w - 0.5): shift by one so that truncation is the floor.
    let xi = (fx + 1.0) as usize;
    let tx = fx - (xi as f32 - 1.0);
    let x0 = (xi + w - 1) & (w - 1);
    let x1 = xi & (w - 1);
    let y0 = fy as usize;
    let ty = fy - y0 as f32;
    let y1 = (y0 + 1).min(h - 1);
    (x0, x1, y0, y1, tx, ty)
}

impl Shared<'_> {
    /// Colour of the ball at the block whose corner is (x, y), None outside the ball and its air;
    /// `under` is what is on the screen there (for the rim of air).
    fn pixel(&self, x: i32, row: Vec3, vy: f64, under: u32) -> Option<u32> {
        let px = f64::from(x) + f64::from(self.step) / 2.0;
        let dx = px - self.c.0;
        let dy = vy;
        if (px - self.sc.0).powi(2) + (dy + self.c.1 - self.sc.1).powi(2) > self.reach {
            return None;
        }
        // The ray in the frame of the world (not normalised: the quadratic takes its length).
        let d = self.a0 * (dx / self.focal) + row;
        let oc = self.oc;
        let a = d.dot(d);
        let b = oc.dot(d);
        let disc = b * b - a * (oc.dot(oc) - self.radius * self.radius);
        if disc < 0.0 {
            if !self.atmosphere {
                return None;
            }
            // The air above the limb: a thin bright rim on the lit side.
            let d = d * (1.0 / a.sqrt());
            let b = oc.dot(d);
            let closest = (oc - d * b).length();
            let edge = (closest / self.radius - 1.0) / 0.045;
            if edge < 1.0 && b < 0.0 {
                let p = (oc + d * (-b)) * (1.0 / closest);
                let lit = (p.dot(self.sun_d) as f32 * 0.7 + 0.5).clamp(0.0, 1.0);
                let a = (1.0 - edge as f32).powi(2) * lit * (self.air as f32).min(1.0).sqrt();
                return Some(vector_render::mix_color(under, 0x0070_B0FF, a));
            }
            return None;
        }
        let t = (-b - disc.sqrt()) / a;
        let nb = (oc + d * t) * (1.0 / self.radius);
        let (nx, ny, nz) = (nb.x as f32, nb.y as f32, nb.z as f32);
        let lon = fast_atan2(nz, nx);
        let u = lon * (0.5 / PI) + if lon < 0.0 { 1.0 } else { 0.0 };
        let c = (nx * nx + nz * nz).sqrt();
        let v = 0.5 - fast_atan2(ny, c) / PI;
        // The ground.
        let tex = self.tex;
        let (x0, x1, y0, y1, tx, ty) = bilerp(tex.w, tex.h, u, v);
        let idx = [
            y0 * tex.w + x0,
            y0 * tex.w + x1,
            y1 * tex.w + x0,
            y1 * tex.w + x1,
        ];
        let wt = [
            (1.0 - tx) * (1.0 - ty),
            tx * (1.0 - ty),
            (1.0 - tx) * ty,
            tx * ty,
        ];
        let (mut se, mut sn, mut land, mut ht, mut var) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let mut base = [0.0f32; 3];
        for k in 0..4 {
            let t = &tex.texels[idx[k]];
            se += t.se * wt[k];
            sn += t.sn * wt[k];
            land += t.land * wt[k];
            ht += t.ht * wt[k];
            var += t.var * wt[k];
            let a = unpack(tex.albedo[idx[k]]);
            for ch in 0..3 {
                base[ch] += a[ch] * wt[k];
            }
        }
        let cinv = 1.0 / c.max(0.05);
        // East and north of the point, in the frame of the world, against the light.
        let (ex, ez) = (-nz * cinv, nx * cinv);
        let (nox, noy, noz) = (-ny * nx * cinv, c, -ny * nz * cinv);
        let s = self.sun_b;
        let toward = (s[0] * ex + s[2] * ez) * se + (s[0] * nox + s[1] * noy + s[2] * noz) * sn;
        let mut shade = (1.0 + land * (toward.clamp(-0.6, 0.6) + ht)) * var;
        if let Some(cl) = self.cloud {
            let (x0, x1, y0, y1, tx, ty) = bilerp(cl.w, cl.h, u, v);
            let g = |x: usize, y: usize| f32::from(cl.cover[y * cl.w + x]);
            let a = g(x0, y0) + (g(x1, y0) - g(x0, y0)) * tx;
            let bb = g(x0, y1) + (g(x1, y1) - g(x0, y1)) * tx;
            let cv = (a + (bb - a) * ty) * (1.0 / 255.0);
            if cv > 0.02 {
                let m = (cv * 0.92).min(0.92);
                let white = unpack(0x00F4_F6FA);
                for ch in 0..3 {
                    base[ch] += (white[ch] - base[ch]) * m;
                }
                shade = shade * (1.0 - cv) + cv * 0.95;
            }
        }
        let light = (nb.dot(self.sun_d)).max(-0.2) as f32;
        let lit = ((light + 0.08) / 0.25).clamp(0.0, 1.0);
        let day = 0.10 + 0.90 * pow08(light);
        let k = if lit >= 1.0 {
            day
        } else {
            0.10 + (day - 0.10) * lit
        };
        let kk = (k * shade).max(0.05);
        Some(pack([base[0] * kk, base[1] * kk, base[2] * kk]))
    }
}

#[inline]
fn add_px(p: &mut u32, c: u32) {
    let add = |a: u32, b: u32| (a + b).min(255);
    let r = add((*p >> 16) & 255, (c >> 16) & 255);
    let g = add((*p >> 8) & 255, (c >> 8) & 255);
    let b = add(*p & 255, c & 255);
    *p = (r << 16) | (g << 8) | b;
}

/// One size of the baked ground of a world: filled row by row over several images when it is big.
struct Level {
    w: usize,
    h: usize,
    texels: Vec<Texel>,
    rows_done: usize,
    albedo: Vec<u32>,
    /// Colour stamp the albedo was made for (u64::MAX: not made).
    colors_key: u64,
}

struct Ball {
    world: u64,
    radius_bits: u64,
    levels: [Option<Level>; 5],
    last_used: u64,
}

struct CloudTex {
    world: u64,
    stamp: u64,
    w: usize,
    h: usize,
    cover: Vec<u8>,
}

thread_local! {
    static BALLS: RefCell<Vec<Ball>> = const { RefCell::new(Vec::new()) };
    static CLOUDS: RefCell<Vec<CloudTex>> = const { RefCell::new(Vec::new()) };
    static TICK: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Worlds kept (the least recently drawn goes first).
const KEEP: usize = 40;

/// Texels baked per image per thread while a finer level is being made (the ball is drawn with
/// the finest finished level meanwhile).
const BAKE_BUDGET: usize = 2048;

/// Paint a solid world as a lit ball (see the module). Same contract as ever: the colour of each
/// cell, sunlight and a terminator, relief shaded from the slope of the ground, a rim of air.
pub fn fill(
    fb: &mut Framebuffer,
    cam: &Camera,
    center: Vec3,
    radius: f64,
    orient: &Mat3,
    sun: Vec3,
    paint: &Paint,
) {
    let _p = crate::perf::span("ball_total");
    let oc = cam.pos - center;
    let dist = oc.length();
    if dist <= radius * 1.001 {
        return;
    }
    let focal = fb.h as f64 * 0.5 / (cam.fov_y * 0.5).tan();
    let (cx, cy) = (fb.w as f64 * 0.5, fb.h as f64 * 0.5);
    let view_c = cam.to_view(center);
    if view_c.z <= 0.0 {
        return;
    }
    let (sx, sy) = (
        cx + view_c.x / view_c.z * focal,
        cy - view_c.y / view_c.z * focal,
    );
    let r_px = radius / (dist * dist - radius * radius).sqrt() * focal;
    let half = (r_px * 1.08) as i32 + 2;
    let step = ((r_px / 150.0).ceil() as i32).max(1);
    let (x_lo, x_hi) = (
        (sx as i32 - half).max(0),
        (sx as i32 + half).min(fb.w as i32 - 1),
    );
    let (y_lo, y_hi) = (
        (sy as i32 - half).max(0),
        (sy as i32 + half).min(fb.h as i32 - 1),
    );
    if x_lo > x_hi || y_lo > y_hi {
        return;
    }
    let want = level_for(r_px);
    let atmosphere = paint.air > 0.02;
    let wet = paint.air > 0.05;
    let colors_key = paint.colors ^ u64::from(wet);
    let tick = TICK.with(|t| {
        t.set(t.get() + 1);
        t.get()
    });
    BALLS.with(|balls| {
        CLOUDS.with(|clouds| {
            let mut balls = balls.borrow_mut();
            let mut clouds = clouds.borrow_mut();
            let radius_bits = radius.to_bits();
            let at = match balls
                .iter()
                .position(|b| b.world == paint.world && b.radius_bits == radius_bits)
            {
                Some(i) => i,
                None => {
                    if balls.len() >= KEEP {
                        let old = (0..balls.len())
                            .min_by_key(|&i| balls[i].last_used)
                            .unwrap_or(0);
                        balls.remove(old);
                    }
                    balls.push(Ball {
                        world: paint.world,
                        radius_bits,
                        levels: [None, None, None, None, None],
                        last_used: tick,
                    });
                    balls.len() - 1
                }
            };
            balls[at].last_used = tick;
            let terrain = paint.map.map(|m| Terrain::new(m, radius));
            // The finest finished level, or a coarse one made now (never a long wait: 2k to 8k
            // texels).
            let best_done = (0..=want).rev().find(|&l| {
                balls[at].levels[l]
                    .as_ref()
                    .is_some_and(|v| v.rows_done == v.h)
            });
            let used = match best_done {
                Some(l) => l,
                None => {
                    let l = 0;
                    let _p = crate::perf::span("ball_bake");
                    bake_level(&mut balls[at], l, terrain.as_ref(), radius, usize::MAX);
                    l
                }
            };
            if used < want {
                // Bake the wanted level a little each image; the big ones leave the other worlds.
                if want >= 3 && balls[at].levels[want].is_none() {
                    for (i, b) in balls.iter_mut().enumerate() {
                        if i != at {
                            b.levels[3] = None;
                            b.levels[4] = None;
                        }
                    }
                }
                let _p = crate::perf::span("ball_bake");
                bake_level(
                    &mut balls[at],
                    want,
                    terrain.as_ref(),
                    radius,
                    BAKE_BUDGET * threads(),
                );
            }
            let level = balls[at].levels[used].as_mut().expect("baked level");
            if level.colors_key != colors_key {
                let _p = crate::perf::span("ball_recolor");
                let table: Vec<u32> = (0..W * H).map(|i| (paint.color)(i % W, i / W)).collect();
                let mut albedo = std::mem::take(&mut level.albedo);
                recolor(&table, wet, &level.texels, &mut albedo);
                level.albedo = albedo;
                level.colors_key = colors_key;
            }
            let level = &*level;
            // The clouds: a texture rebuilt when they have drifted.
            let mut cloud_ref = None;
            if let (Some(f), true) = (paint.clouds, r_px >= 10.0) {
                let (cw, ch) = if r_px < 45.0 { (128, 64) } else { (256, 128) };
                let i = match clouds.iter().position(|c| c.world == paint.world) {
                    Some(i) => i,
                    None => {
                        if clouds.len() >= KEEP {
                            clouds.remove(0);
                        }
                        clouds.push(CloudTex {
                            world: paint.world,
                            stamp: u64::MAX,
                            w: 0,
                            h: 0,
                            cover: Vec::new(),
                        });
                        clouds.len() - 1
                    }
                };
                let c = &mut clouds[i];
                if c.stamp != paint.cloud_stamp || c.w != cw {
                    let _p = crate::perf::span("cloud_build");
                    c.cover = build_clouds(f, cw, ch);
                    (c.stamp, c.w, c.h) = (paint.cloud_stamp, cw, ch);
                }
                cloud_ref = Some(&clouds[i]);
            }
            let inv_orient = orient.transpose();
            let to_body = inv_orient.mul(&cam.rot.transpose());
            let sb = inv_orient.mul_vec(sun);
            let reach = (r_px * 1.12 + 3.0).powi(2);
            let sh = Shared {
                tex: level,
                cloud: cloud_ref,
                a0: to_body.mul_vec(Vec3::new(1.0, 0.0, 0.0)),
                a1: to_body.mul_vec(Vec3::new(0.0, 1.0, 0.0)),
                a2: to_body.mul_vec(Vec3::new(0.0, 0.0, 1.0)),
                sun_d: sb,
                sun_b: [sb.x as f32, sb.y as f32, sb.z as f32],
                oc: inv_orient.mul_vec(oc),
                radius,
                focal,
                c: (cx, cy),
                sc: (sx, sy),
                reach,
                step,
                x_lo,
                x_hi,
                atmosphere,
                air: paint.air,
            };
            let _p = crate::perf::span("ball_pixels");
            paint_pixels(fb, &sh, y_lo, y_hi);
        });
    });
}

/// Bake rows of level `l` of a world, at most `budget` texels.
fn bake_level(ball: &mut Ball, l: usize, terrain: Option<&Terrain>, radius: f64, budget: usize) {
    let (w, h) = SIZES[l];
    let lv = ball.levels[l].get_or_insert_with(|| Level {
        w,
        h,
        texels: vec![Texel::default(); w * h],
        rows_done: 0,
        albedo: Vec::new(),
        colors_key: u64::MAX,
    });
    let rows = (budget / w).max(1).min(h - lv.rows_done);
    if rows == 0 {
        return;
    }
    let r0 = lv.rows_done;
    bake_rows(
        terrain,
        radius,
        w,
        h,
        r0,
        &mut lv.texels[r0 * w..(r0 + rows) * w],
    );
    lv.rows_done += rows;
    if lv.rows_done == h {
        lv.colors_key = u64::MAX;
    }
}

/// Paint the blocks of the ball in bands of rows, one thread per band.
fn paint_pixels(fb: &mut Framebuffer, sh: &Shared, y_lo: i32, y_hi: i32) {
    let step = sh.step;
    let blocks = ((y_hi - y_lo) / step + 1) as usize;
    let w = fb.w;
    let fb_h = fb.h;
    // A block row owns `step` rows of the screen.
    let region_end = (y_lo as usize + blocks * step as usize).min(fb_h);
    let region = &mut fb.px[y_lo as usize * w..region_end * w];
    let x_lo = sh.x_lo;
    let paint_band = |b0: usize, b1: usize, slice: &mut [u32]| {
        for bj in b0..b1 {
            let y = y_lo + bj as i32 * step;
            // Chord of the disc (and its air) on this row, to skip the empty sides.
            let dy = f64::from(y) + f64::from(step) / 2.0 - sh.sc.1;
            let rem = sh.reach - dy * dy;
            if rem < 0.0 {
                continue;
            }
            let chord = rem.sqrt() as i32 + step + 1;
            let xa = (sh.sc.0 as i32 - chord).max(x_lo);
            let xb = (sh.sc.0 as i32 + chord).min(sh.x_hi);
            let mut x = x_lo + ((xa - x_lo) / step) * step;
            let row0 = (y as usize - y_lo as usize - b0 * step as usize) * w;
            let vy = f64::from(y) + f64::from(step) / 2.0 - sh.c.1;
            let row = sh.a1 * (-vy / sh.focal) + sh.a2;
            while x <= xb {
                let under = slice.get(row0 + x as usize).copied().unwrap_or(0);
                if let Some(c) = sh.pixel(x, row, vy, under) {
                    for ddy in 0..step as usize {
                        if y as usize + ddy >= region_end {
                            break;
                        }
                        let o = row0 + ddy * w + x as usize;
                        for ddx in 0..(step as usize).min(w - x as usize) {
                            if let Some(p) = slice.get_mut(o + ddx) {
                                add_px(p, c);
                            }
                        }
                    }
                }
                x += step;
            }
        }
    };
    let block_len = w * step as usize;
    bands(region, block_len, blocks, |b0, piece| {
        paint_band(b0, b0 + piece.len().div_ceil(block_len), piece);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vector_data::GameData;
    use vector_render::{mix_color, scale_color};
    use vector_sim::planet::PlanetMap;
    use vector_sim::scene::new_sim;

    fn solid_world() -> (vector_sim::Sim, usize) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let s = new_sim(5, Arc::new(GameData::load(&dir).expect("data")));
        let i = (0..s.bodies.len())
            .find(|&i| s.landable(i))
            .expect("a solid world");
        (s, i)
    }

    /// A camera `dist` radii from the centre of a unit-radius world at the origin, looking at it
    /// along -z, `fov` wide.
    fn cam_at(dist: f64, fov: f64) -> Camera {
        let pos = Vec3::new(0.0, 0.0, dist);
        Camera {
            pos,
            rot: Mat3::look_at(Vec3::new(0.0, 0.0, -1.0), Vec3::new(0.0, 1.0, 0.0)).transpose(),
            fov_y: fov,
        }
    }

    fn flat<'a>(color: &'a dyn Fn(usize, usize) -> u32, world: u64) -> Paint<'a> {
        Paint {
            map: None,
            color,
            air: 0.0,
            clouds: None,
            world,
            colors: 0,
            cloud_stamp: 0,
        }
    }

    fn render_flat(w: usize, h: usize, sun: Vec3, world: u64) -> Framebuffer {
        let mut fb = Framebuffer::new(w, h);
        let cam = cam_at(3.0, 1.2);
        let color = |_: usize, _: usize| 0x00C0_C0C0;
        fill(
            &mut fb,
            &cam,
            Vec3::default(),
            1.0,
            &Mat3::rot_y(0.0),
            sun.normalized(),
            &flat(&color, world),
        );
        fb
    }

    fn lum(c: u32) -> f64 {
        f64::from((c >> 16) & 255) + f64::from((c >> 8) & 255) + f64::from(c & 255)
    }

    #[test]
    fn a_ball_lit_from_the_front_is_symmetric_and_round() {
        let fb = render_flat(320, 200, Vec3::new(0.0, 0.0, 1.0), 1);
        let (cx, cy) = (160usize, 100usize);
        for k in 1..60 {
            let (l, r) = (fb.px[cy * 320 + cx - k - 1], fb.px[cy * 320 + cx + k]);
            assert!(
                (lum(l) - lum(r)).abs() <= 6.0,
                "left {l:x} right {r:x} at {k}"
            );
            let (u, d) = (fb.px[(cy - k - 1) * 320 + cx], fb.px[(cy + k) * 320 + cx]);
            assert!((lum(u) - lum(d)).abs() <= 6.0, "up {u:x} down {d:x} at {k}");
        }
        // The middle is the brightest, the rim of the disc the dimmest, outside is the void.
        assert!(lum(fb.px[cy * 320 + cx]) > lum(fb.px[cy * 320 + cx + 55]));
        assert_eq!(fb.px[2 * 320 + 2], 0);
        // About the area of a disc: radius in pixels from the focal length.
        let lit = fb.px.iter().filter(|&&p| p != 0).count() as f64;
        let focal = 100.0 / (0.6f64).tan();
        let r_px = 1.0 / (9.0f64 - 1.0).sqrt() * focal;
        let want = std::f64::consts::PI * r_px * r_px;
        assert!((lit / want - 1.0).abs() < 0.06, "{lit} vs {want}");
    }

    #[test]
    fn the_terminator_is_sharp_and_on_the_right_side() {
        // The sun to the right of the viewer (the view looks down -z, so right is -x): the right half is lit, the left half is night.
        let fb = render_flat(320, 200, Vec3::new(-1.0, 0.0, 0.0), 2);
        let (cx, cy) = (160usize, 100usize);
        let left = lum(fb.px[cy * 320 + cx - 40]);
        let right = lum(fb.px[cy * 320 + cx + 40]);
        assert!(right > left * 3.0, "night {left} day {right}");
        // From dark to lit within a few pixels around the middle (the terminator).
        let a = lum(fb.px[cy * 320 + cx - 12]);
        let b = lum(fb.px[cy * 320 + cx + 12]);
        assert!(b > a * 1.5, "{a} -> {b}");
        // And the night side is dim but not black (a ball is still seen against the sky).
        assert!(left > 0.0);
    }

    #[test]
    fn the_same_picture_on_one_thread_and_on_many() {
        let sun = Vec3::new(0.6, 0.3, 0.7);
        set_local_threads(1);
        let one = render_flat(300, 180, sun, 3);
        set_local_threads(5);
        let many = render_flat(300, 180, sun, 3);
        set_local_threads(0);
        assert!(one.px == many.px, "threads change the picture");
    }

    #[test]
    fn no_panic_whatever_the_size_or_the_place() {
        for (w, h) in [(1, 1), (2, 3), (17, 5), (64, 64), (333, 77)] {
            for dist in [1.0005, 1.01, 1.5, 3.0, 40.0, 1.0e5] {
                for off in [0.0, 0.5, 5.0, -3.0] {
                    let mut fb = Framebuffer::new(w, h);
                    let mut cam = cam_at(dist, 1.2);
                    cam.pos.x = off;
                    let color = |x: usize, y: usize| (x * 7 + y) as u32 & 0xFFFFFF;
                    let paint = Paint {
                        air: 1.0,
                        ..flat(&color, 9)
                    };
                    fill(
                        &mut fb,
                        &cam,
                        Vec3::default(),
                        1.0,
                        &Mat3::rot_y(0.3),
                        Vec3::new(0.0, 0.0, 1.0),
                        &paint,
                    );
                }
            }
        }
    }

    fn terran_paint<'a>(
        map: &'a PlanetMap,
        color: &'a dyn Fn(usize, usize) -> u32,
        clouds: Option<&'a (dyn Fn(f64, f64) -> f32 + Sync)>,
    ) -> Paint<'a> {
        Paint {
            map: Some(map),
            color,
            air: map.pressure.max(0.5),
            clouds,
            world: 77,
            colors: 1,
            cloud_stamp: 1,
        }
    }

    /// The ball as it used to be drawn, pixel by pixel from the map and the noise (kept to check
    /// that the baked textures look the same).
    #[allow(clippy::too_many_arguments)]
    fn reference_fill(
        fb: &mut Framebuffer,
        cam: &Camera,
        center: Vec3,
        radius: f64,
        orient: &Mat3,
        sun: Vec3,
        paint: &Paint,
    ) {
        use crate::globe::noise2;
        let oc = cam.pos - center;
        let dist = oc.length();
        if dist <= radius * 1.001 {
            return;
        }
        let focal = fb.h as f64 * 0.5 / (cam.fov_y * 0.5).tan();
        let (cx, cy) = (fb.w as f64 * 0.5, fb.h as f64 * 0.5);
        let view_c = cam.to_view(center);
        if view_c.z <= 0.0 {
            return;
        }
        let (sx, sy) = (
            cx + view_c.x / view_c.z * focal,
            cy - view_c.y / view_c.z * focal,
        );
        let r_px = radius / (dist * dist - radius * radius).sqrt() * focal;
        let half = (r_px * 1.08) as i32 + 2;
        let step = ((r_px / 170.0).ceil() as i32).max(1);
        let (x_lo, x_hi) = (
            (sx as i32 - half).max(0),
            (sx as i32 + half).min(fb.w as i32 - 1),
        );
        let (y_lo, y_hi) = (
            (sy as i32 - half).max(0),
            (sy as i32 + half).min(fb.h as i32 - 1),
        );
        let to_world = cam.rot.transpose();
        let inv_orient = orient.transpose();
        let sun_b = inv_orient.mul_vec(sun);
        let atmosphere = paint.air > 0.02;
        let terrain = paint.map.map(|m| Terrain::new(m, radius));
        let mut y = y_lo;
        while y <= y_hi {
            let mut x = x_lo;
            while x <= x_hi {
                let px = f64::from(x) + f64::from(step) / 2.0;
                let py = f64::from(y) + f64::from(step) / 2.0;
                let d = to_world
                    .mul_vec(Vec3::new((px - cx) / focal, (cy - py) / focal, 1.0))
                    .normalized();
                let b = oc.dot(d);
                let disc = b * b - (oc.dot(oc) - radius * radius);
                let color = if disc >= 0.0 {
                    let t = -b - disc.sqrt();
                    let n = (oc + d * t) * (1.0 / radius);
                    let nb = inv_orient.mul_vec(n);
                    let lat = nb.y.clamp(-1.0, 1.0).asin();
                    let lon = nb.z.atan2(nb.x).rem_euclid(std::f64::consts::TAU);
                    let fx = lon / std::f64::consts::TAU * W as f64 - 0.5;
                    let fy = (0.5 - lat / std::f64::consts::PI) * H as f64 - 0.5;
                    // Borders between biomes are not squares: the lookup is warped by noise.
                    let warp = (
                        noise2(fx * 2.3, fy * 2.3) * 0.9,
                        noise2(fx * 2.3 + 40.0, fy * 2.3) * 0.9,
                    );
                    let (wxi, wyi) = (
                        ((fx + f64::from(warp.0)).round() as i64).rem_euclid(W as i64) as usize,
                        ((fy + f64::from(warp.1)).round() as i64).clamp(0, H as i64 - 1) as usize,
                    );
                    let mut base = (paint.color)(wxi, wyi);
                    let mut shade = 1.0f32;
                    if let Some(terr) = &terrain {
                        // The same ground as seen from low: the height at this point, to the
                        // resolution of a pixel, and its slope against the light.
                        let px_m = radius / r_px.max(1.0) * f64::from(step);
                        let (hc, e) = terr.sample(lat, lon, px_m);
                        let delta = px_m * 1.2 / radius;
                        let (he, _) =
                            terr.sample(lat, lon + delta / lat.cos().abs().max(0.05), px_m);
                        let (hn, _) = terr.sample(lat + delta, lon, px_m);
                        let slope = (
                            f64::from(he - hc) / (px_m * 1.2),
                            f64::from(hn - hc) / (px_m * 1.2),
                        );
                        let east = Vec3::new(-lon.sin(), 0.0, lon.cos());
                        let north =
                            Vec3::new(-lat.sin() * lon.cos(), lat.cos(), -lat.sin() * lon.sin());
                        let toward =
                            (sun_b.dot(east) * slope.0 + sun_b.dot(north) * slope.1) as f32;
                        // Seas and lakes follow the detailed height, not the cell.
                        if hc <= 0.0 && e > 0.0 && paint.air > 0.05 {
                            base = 0x0020_60A0;
                        } else if hc > 0.0 && e <= 0.0 && paint.air > 0.05 {
                            base = scale_color(0x00C8_B880, 0.9);
                        }
                        if hc > 0.0 {
                            shade += (toward * 9.0).clamp(-0.6, 0.6);
                            shade += (hc / terr.relief as f32).clamp(0.0, 1.0) * 0.3;
                            // Variation of the ground from place to place.
                            shade *= 1.0
                                + 0.12 * noise2(lon * radius / 30_000.0, lat * radius / 30_000.0);
                        } else {
                            shade *= 1.0 + 0.06 * e.clamp(-1.0, 0.0);
                        }
                    }
                    // Clouds hide the ground and shade the part of the sky they stand in.
                    if let Some(cloud) = paint.clouds {
                        let c = cloud(lat, lon);
                        if c > 0.02 {
                            base = mix_color(base, 0x00F4_F6FA, (c * 0.92).min(0.92));
                            shade = shade * (1.0 - c) + c * 0.95;
                        }
                    }
                    let light = n.dot(sun).max(-0.2);
                    let lit = ((light + 0.08) / 0.25).clamp(0.0, 1.0) as f32;
                    let day = 0.10 + 0.90 * (light.max(0.0) as f32).powf(0.8);
                    let k = if lit >= 1.0 {
                        day
                    } else {
                        0.10 + (day - 0.10) * lit
                    };
                    Some(scale_color(base, (k * shade).max(0.05)))
                } else if atmosphere {
                    // The air above the limb: a thin bright rim on the lit side.
                    let closest = (oc - d * b).length();
                    let edge = (closest / radius - 1.0) / 0.045;
                    if edge < 1.0 && b < 0.0 {
                        let p = (oc + d * (-b)) * (1.0 / closest);
                        let lit = (p.dot(sun) as f32 * 0.7 + 0.5).clamp(0.0, 1.0);
                        let a =
                            (1.0 - edge as f32).powi(2) * lit * (paint.air as f32).min(1.0).sqrt();
                        let at = (fb.w as i32 * y.min(fb.h as i32 - 1) + x) as usize;
                        let under = fb.px.get(at).copied().unwrap_or(0);
                        Some(mix_color(under, 0x0070_B0FF, a))
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(c) = color {
                    for dy in 0..step {
                        for dx in 0..step {
                            fb.plot(x + dx, y + dy, c);
                        }
                    }
                }
                x += step;
            }
            y += step;
        }
    }

    /// Draw until the finest level of the texture is baked.
    fn settle(f: &mut dyn FnMut()) {
        for _ in 0..80 {
            f();
        }
    }

    #[test]
    fn the_baked_ball_looks_like_the_per_pixel_one_and_is_faster() {
        let (s, i) = solid_world();
        let map = s.planet_map(i).expect("map");
        let radius = s.bodies[i].radius;
        let color = |x: usize, y: usize| s.data.biomes[map.biome_at(x, y)].color;
        let atmo = s.atmo(&map);
        let surf = &map.surface;
        let clouds =
            move |lat: f64, lon: f64| vector_world::climate::cloud_cover(surf, &atmo, lat, lon);
        let paint = terran_paint(&map, &color, Some(&clouds));
        let (w, h) = (480usize, 300usize);
        // The world fills the screen: the radius on screen is about 0.4 of the height.
        let focal = h as f64 * 0.5 / (0.6f64).tan();
        let r_px = 0.4 * h as f64;
        let dist = radius * (1.0 + (focal / r_px).powi(2)).sqrt();
        let cam = Camera {
            pos: Vec3::new(0.0, 0.0, dist),
            rot: Mat3::look_at(Vec3::new(0.0, 0.0, -1.0), Vec3::new(0.0, 1.0, 0.0)).transpose(),
            fov_y: 1.2,
        };
        let orient = Mat3::rot_z(0.2).mul(&Mat3::rot_y(1.0));
        let sun = Vec3::new(0.5, 0.2, 0.8).normalized();
        set_local_threads(1);
        let mut fast = Framebuffer::new(w, h);
        settle(&mut || {
            fast.clear();
            fill(
                &mut fast,
                &cam,
                Vec3::default(),
                radius,
                &orient,
                sun,
                &paint,
            );
        });
        let mut fast_ms = f64::MAX;
        for _ in 0..5 {
            let t = std::time::Instant::now();
            fast.clear();
            fill(
                &mut fast,
                &cam,
                Vec3::default(),
                radius,
                &orient,
                sun,
                &paint,
            );
            fast_ms = fast_ms.min(t.elapsed().as_secs_f64() * 1000.0);
        }
        let mut slow = Framebuffer::new(w, h);
        let t = std::time::Instant::now();
        reference_fill(
            &mut slow,
            &cam,
            Vec3::default(),
            radius,
            &orient,
            sun,
            &paint,
        );
        let slow_ms = t.elapsed().as_secs_f64() * 1000.0;
        set_local_threads(0);
        // Mean absolute difference per channel over the pixels either of them painted.
        let (mut sum, mut n) = (0.0f64, 0usize);
        for (a, b) in fast.px.iter().zip(&slow.px) {
            if *a != 0 || *b != 0 {
                for sh in [16, 8, 0] {
                    sum += f64::from(((*a >> sh) & 255).abs_diff((*b >> sh) & 255));
                }
                n += 3;
            }
        }
        let mean = sum / n.max(1) as f64;
        eprintln!(
            "baked {fast_ms:.1} ms, per pixel {slow_ms:.1} ms, mean difference {mean:.2}/255"
        );
        assert!(n > 1000, "something is drawn");
        assert!(mean < 12.0, "mean difference {mean}/255");
        // The budget: a world over the whole screen in well under a tenth of a second on one
        // thread, even on a machine that is busy (it takes some ms on a quiet one).
        assert!(fast_ms < 250.0, "{fast_ms} ms");
        assert!(fast_ms < slow_ms, "{fast_ms} ms vs {slow_ms} ms");
    }
}

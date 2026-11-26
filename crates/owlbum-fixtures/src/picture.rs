//! Painting the pictures. Integer arithmetic only, so every system paints
//! the same pixels (no `sin`, `powf` or other platform maths).

use image::{Rgb, RgbImage, RgbaImage, imageops};

use crate::rng::Rng;

fn mix(a: [u8; 3], b: [u8; 3], num: u32, den: u32) -> [u8; 3] {
    let den = den.max(1);
    let num = num.min(den);
    let f = |x: u8, y: u8| ((u32::from(x) * (den - num) + u32::from(y) * num) / den) as u8;
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2])]
}

fn fill_rect(img: &mut RgbImage, x0: i64, y0: i64, x1: i64, y1: i64, c: [u8; 3]) {
    let (w, h) = (i64::from(img.width()), i64::from(img.height()));
    for y in y0.max(0)..y1.min(h) {
        for x in x0.max(0)..x1.min(w) {
            img.put_pixel(x as u32, y as u32, Rgb(c));
        }
    }
}

fn fill_ellipse(img: &mut RgbImage, cx: i64, cy: i64, rx: i64, ry: i64, c: [u8; 3]) {
    let (w, h) = (i64::from(img.width()), i64::from(img.height()));
    let (rx, ry) = (rx.max(1), ry.max(1));
    for y in (cy - ry).max(0)..(cy + ry).min(h) {
        for x in (cx - rx).max(0)..(cx + rx).min(w) {
            let (dx, dy) = (x - cx, y - cy);
            if dx * dx * ry * ry + dy * dy * rx * rx <= rx * rx * ry * ry {
                img.put_pixel(x as u32, y as u32, Rgb(c));
            }
        }
    }
}

/// A triangle with a flat base from `(x0, base)` to `(x1, base)` and its
/// tip at `(tip_x, tip_y)` above the base.
fn fill_peak(img: &mut RgbImage, x0: i64, x1: i64, base: i64, tip_x: i64, tip_y: i64, c: [u8; 3]) {
    let h = i64::from(img.height());
    for y in tip_y.max(0)..base.min(h) {
        let t = y - tip_y;
        let span = (base - tip_y).max(1);
        let left = tip_x - (tip_x - x0) * t / span;
        let right = tip_x + (x1 - tip_x) * t / span;
        fill_rect(img, left, y, right + 1, y + 1, c);
    }
}

fn gradient(w: u32, h: u32, top: [u8; 3], bottom: [u8; 3], direction: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let (num, den) = match direction % 3 {
            0 => (y, h),
            1 => (x, w),
            _ => (x + y, w + h),
        };
        Rgb(mix(top, bottom, num, den))
    })
}

/// A random picture of a few shapes over a gradient. Different seeds give
/// pictures that differ in structure, not just in colour, so near-duplicate
/// detection does not group them.
pub fn scene(seed: u64, w: u32, h: u32) -> RgbImage {
    let mut r = Rng::new(seed);
    let (a, b) = (r.colour(), r.colour());
    let mut img = gradient(w, h, a, b, r.range(0, 3));
    let (wi, hi) = (i64::from(w), i64::from(h));
    for _ in 0..r.range(4, 8) {
        let c = r.colour();
        let cx = i64::from(r.range(0, w));
        let cy = i64::from(r.range(0, h));
        let rx = i64::from(r.range(w / 12 + 1, w / 4 + 2));
        let ry = i64::from(r.range(h / 12 + 1, h / 3 + 2));
        if r.chance(50) {
            fill_rect(&mut img, cx - rx, cy - ry, cx + rx, cy + ry, c);
        } else {
            fill_ellipse(&mut img, cx, cy, rx, ry, c);
        }
    }
    // A thin frame line so even a flat scene has edges.
    let c = r.colour();
    let t = i64::from(r.range(2, 6));
    fill_rect(&mut img, 0, hi / 2 - t, wi, hi / 2 + t, c);
    img
}

/// Sky, hills and a lake with a sun reflected in it.
pub fn lake(w: u32, h: u32) -> RgbImage {
    let (wi, hi) = (i64::from(w), i64::from(h));
    let shore = hi * 11 / 20;
    let mut img = RgbImage::from_fn(w, h, |_, y| {
        let y = i64::from(y);
        if y < shore {
            Rgb(mix([96, 150, 220], [214, 232, 246], y as u32, shore as u32))
        } else {
            Rgb(mix(
                [70, 120, 170],
                [20, 50, 95],
                (y - shore) as u32,
                (hi - shore) as u32,
            ))
        }
    });
    fill_ellipse(
        &mut img,
        wi * 3 / 4,
        hi / 5,
        wi / 16,
        wi / 16,
        [255, 236, 170],
    );
    fill_peak(
        &mut img,
        -wi / 8,
        wi / 2,
        shore,
        wi / 6,
        hi / 4,
        [70, 100, 80],
    );
    fill_peak(
        &mut img,
        wi / 4,
        wi + wi / 8,
        shore,
        wi * 2 / 3,
        hi / 3,
        [52, 88, 62],
    );
    fill_rect(&mut img, 0, shore - hi / 40, wi, shore, [40, 92, 46]);
    // The sun's reflection: light streaks on the water.
    for i in 0..6 {
        let y = shore + hi / 20 + i * hi / 30;
        let half = wi / 40 + i * wi / 120;
        fill_rect(
            &mut img,
            wi * 3 / 4 - half,
            y,
            wi * 3 / 4 + half,
            y + 2,
            [230, 220, 170],
        );
    }
    img
}

/// Sky, sea, a quay and boats.
pub fn harbour(w: u32, h: u32) -> RgbImage {
    let (wi, hi) = (i64::from(w), i64::from(h));
    let sea = hi / 2;
    let mut img = RgbImage::from_fn(w, h, |_, y| {
        let y = i64::from(y);
        if y < sea {
            Rgb(mix([150, 180, 215], [235, 225, 210], y as u32, sea as u32))
        } else {
            Rgb(mix(
                [40, 90, 120],
                [20, 55, 80],
                (y - sea) as u32,
                (hi - sea) as u32,
            ))
        }
    });
    // Buildings on the far side.
    let mut r = Rng::new(0x4a8b);
    let mut x = 0;
    while x < wi {
        let bw = i64::from(r.range(w / 20 + 1, w / 8 + 2));
        let bh = i64::from(r.range(h / 12 + 1, h / 4 + 2));
        fill_rect(&mut img, x, sea - bh, x + bw, sea, r.colour());
        x += bw;
    }
    // The quay.
    fill_rect(&mut img, 0, hi * 4 / 5, wi * 2 / 5, hi, [120, 100, 80]);
    // Boats: a hull, a cabin and a mast.
    let boats = [
        (wi / 2, hi * 2 / 3, [235, 235, 235]),
        (wi * 3 / 4, hi * 3 / 5, [200, 40, 40]),
        (wi * 9 / 10, hi * 3 / 4, [240, 200, 40]),
    ];
    for (bx, by, c) in boats {
        let half = wi / 14;
        fill_rect(
            &mut img,
            bx - half,
            by - hi / 30,
            bx + half,
            by + hi / 30,
            c,
        );
        fill_rect(
            &mut img,
            bx - half / 3,
            by - hi / 12,
            bx + half / 3,
            by - hi / 30,
            [250, 250, 250],
        );
        fill_rect(
            &mut img,
            bx - 1,
            by - hi / 4,
            bx + 2,
            by - hi / 12,
            [60, 50, 40],
        );
    }
    img
}

/// Four quadrants: red top-left, green top-right, blue bottom-left, white
/// bottom-right. Any wrong turn or flip moves a colour to another corner.
pub fn quadrants(w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        Rgb(match (x < w / 2, y < h / 2) {
            (true, true) => [255, 0, 0],
            (false, true) => [0, 255, 0],
            (true, false) => [0, 0, 255],
            (false, false) => [255, 255, 255],
        })
    })
}

/// Box blur of the given radius, three passes (close to a Gaussian).
pub fn blur(img: &RgbImage, radius: u32) -> RgbImage {
    let mut cur = img.clone();
    for _ in 0..3 {
        cur = box_pass(&cur, radius, true);
        cur = box_pass(&cur, radius, false);
    }
    cur
}

fn box_pass(img: &RgbImage, radius: u32, horizontal: bool) -> RgbImage {
    let (w, h) = img.dimensions();
    let r = i64::from(radius);
    RgbImage::from_fn(w, h, |x, y| {
        let mut sum = [0u32; 3];
        let mut n = 0u32;
        for d in -r..=r {
            let (sx, sy) = if horizontal {
                (i64::from(x) + d, i64::from(y))
            } else {
                (i64::from(x), i64::from(y) + d)
            };
            if sx >= 0 && sy >= 0 && sx < i64::from(w) && sy < i64::from(h) {
                let p = img.get_pixel(sx as u32, sy as u32).0;
                for c in 0..3 {
                    sum[c] += u32::from(p[c]);
                }
                n += 1;
            }
        }
        Rgb([(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8])
    })
}

/// Average each `factor` × `factor` block into one pixel.
pub fn shrink(img: &RgbImage, factor: u32) -> RgbImage {
    let (w, h) = (img.width() / factor, img.height() / factor);
    RgbImage::from_fn(w, h, |x, y| {
        let mut sum = [0u32; 3];
        for dy in 0..factor {
            for dx in 0..factor {
                let p = img.get_pixel(x * factor + dx, y * factor + dy).0;
                for c in 0..3 {
                    sum[c] += u32::from(p[c]);
                }
            }
        }
        let n = factor * factor;
        Rgb([(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8])
    })
}

/// Shift a picture by `(dx, dy)` pixels (edges repeat) and lighten it by
/// `lift` levels: one shot of a burst.
pub fn nudge(img: &RgbImage, dx: i64, dy: i64, lift: u8) -> RgbImage {
    let (w, h) = img.dimensions();
    RgbImage::from_fn(w, h, |x, y| {
        let sx = (i64::from(x) - dx).clamp(0, i64::from(w) - 1) as u32;
        let sy = (i64::from(y) - dy).clamp(0, i64::from(h) - 1) as u32;
        let p = img.get_pixel(sx, sy).0;
        Rgb(p.map(|v| v.saturating_add(lift)))
    })
}

/// The stored pixels for an upright picture under EXIF orientation `n`:
/// the inverse of [`apply_orientation`].
pub fn store_for_orientation(upright: &RgbImage, n: u8) -> RgbImage {
    match n {
        2 => imageops::flip_horizontal(upright),
        3 => imageops::rotate180(upright),
        4 => imageops::flip_vertical(upright),
        5 => imageops::flip_horizontal(&imageops::rotate90(upright)),
        6 => imageops::rotate270(upright),
        7 => imageops::flip_vertical(&imageops::rotate90(upright)),
        8 => imageops::rotate90(upright),
        _ => upright.clone(),
    }
}

/// Turn stored pixels upright for EXIF orientation `n`, as a viewer does.
pub fn apply_orientation(stored: RgbImage, n: u8) -> RgbImage {
    match n {
        2 => imageops::flip_horizontal(&stored),
        3 => imageops::rotate180(&stored),
        4 => imageops::flip_vertical(&stored),
        5 => imageops::flip_horizontal(&imageops::rotate90(&stored)),
        6 => imageops::rotate90(&stored),
        7 => imageops::flip_vertical(&imageops::rotate90(&stored)),
        8 => imageops::rotate270(&stored),
        _ => stored,
    }
}

/// A picture with a transparent background and an opaque disc, with a
/// soft edge of partly transparent pixels.
pub fn transparent_disc(w: u32, h: u32) -> RgbaImage {
    let (cx, cy) = (i64::from(w) / 2, i64::from(h) / 2);
    let r = i64::from(w.min(h)) / 3;
    RgbaImage::from_fn(w, h, |x, y| {
        let (dx, dy) = (i64::from(x) - cx, i64::from(y) - cy);
        let d2 = dx * dx + dy * dy;
        let alpha = if d2 <= r * r {
            255
        } else if d2 <= (r + 8) * (r + 8) {
            128
        } else {
            0
        };
        image::Rgba([30, 140, 90, alpha])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_round_trips() {
        let up = quadrants(6, 4);
        for n in 1..=8 {
            let stored = store_for_orientation(&up, n);
            assert_eq!(apply_orientation(stored, n), up, "orientation {n}");
        }
    }

    #[test]
    fn sideways_orientations_swap_the_size() {
        let up = quadrants(6, 4);
        for n in 5..=8 {
            assert_eq!(store_for_orientation(&up, n).dimensions(), (4, 6));
        }
    }

    #[test]
    fn blur_flattens_edges() {
        let q = quadrants(64, 64);
        let b = blur(&q, 6);
        let p = b.get_pixel(32, 10).0;
        assert!(p[0] > 60 && p[1] > 60, "{p:?}");
    }

    #[test]
    fn scenes_differ_by_seed() {
        assert_ne!(scene(1, 32, 24), scene(2, 32, 24));
        assert_eq!(scene(1, 32, 24), scene(1, 32, 24));
    }
}

//! HSV math and the hue/saturation wheel image for the dock color picker.

/// `0xRRGGBB` from hue in degrees and saturation and value from 0 to 1.
pub fn from_hsv(h: f32, s: f32, v: f32) -> u32 {
    let h = h.rem_euclid(360.0);
    let (s, v) = (s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let byte = |f: f32| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u32;
    byte(r) << 16 | byte(g) << 8 | byte(b)
}

/// Hue in degrees, saturation and value of `0xRRGGBB`.
pub fn to_hsv(rgb: u32) -> (f32, f32, f32) {
    let channel = |shift: u32| ((rgb >> shift) & 0xFF) as f32 / 255.0;
    let (r, g, b) = (channel(16), channel(8), channel(0));
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}

/// Hue and saturation at an offset from the wheel's centre, measured in
/// radii with y pointing up. Points past the rim keep full saturation.
pub fn wheel_pick(dx: f32, dy: f32) -> (f32, f32) {
    let h = dy.atan2(dx).to_degrees().rem_euclid(360.0);
    (h, (dx * dx + dy * dy).sqrt().min(1.0))
}

/// Where hue `h` and saturation `s` sit, as an offset from the centre in radii, y down.
pub fn wheel_point(h: f32, s: f32) -> (f32, f32) {
    let a = h.to_radians();
    (s * a.cos(), -s * a.sin())
}

/// A `size`-pixel square BMP of the wheel at full value, with a soft
/// transparent rim. A 32-bit BITMAPV4 keeps the alpha channel.
pub fn wheel_bmp(size: u32) -> Vec<u8> {
    const HEADERS: u32 = 14 + 108;
    let pixels = size * size * 4;
    let mut out = Vec::with_capacity((HEADERS + pixels) as usize);
    let u16le = |out: &mut Vec<u8>, v: u16| out.extend_from_slice(&v.to_le_bytes());
    let u32le = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
    // BITMAPFILEHEADER
    out.extend_from_slice(b"BM");
    u32le(&mut out, HEADERS + pixels);
    u32le(&mut out, 0);
    u32le(&mut out, HEADERS);
    // BITMAPV4HEADER, top-down (negative height), BI_BITFIELDS with an alpha mask
    u32le(&mut out, 108);
    u32le(&mut out, size);
    u32le(&mut out, (size as i32).wrapping_neg() as u32);
    u16le(&mut out, 1);
    u16le(&mut out, 32);
    u32le(&mut out, 3);
    u32le(&mut out, pixels);
    u32le(&mut out, 2835);
    u32le(&mut out, 2835);
    u32le(&mut out, 0);
    u32le(&mut out, 0);
    for mask in [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000] {
        u32le(&mut out, mask);
    }
    u32le(&mut out, 0x7352_4742); // LCS_sRGB
    out.extend_from_slice(&[0; 36 + 12]); // endpoints and gamma, unused for sRGB
    let c = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let (dx, dy) = ((x as f32 + 0.5 - c) / c, (c - (y as f32 + 0.5)) / c);
            let d = (dx * dx + dy * dy).sqrt();
            // One pixel of anti-aliasing at the rim.
            let alpha = ((1.0 - d) * c + 0.5).clamp(0.0, 1.0);
            let (h, s) = wheel_pick(dx, dy);
            let rgb = if alpha > 0.0 { from_hsv(h, s, 1.0) } else { 0 };
            out.extend_from_slice(&[
                rgb as u8,
                (rgb >> 8) as u8,
                (rgb >> 16) as u8,
                (alpha * 255.0).round() as u8,
            ]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trips_and_hits_primaries() {
        assert_eq!(from_hsv(0.0, 1.0, 1.0), 0xFF0000);
        assert_eq!(from_hsv(120.0, 1.0, 1.0), 0x00FF00);
        assert_eq!(from_hsv(240.0, 1.0, 1.0), 0x0000FF);
        assert_eq!(from_hsv(360.0, 1.0, 1.0), 0xFF0000);
        assert_eq!(from_hsv(77.0, 0.0, 0.0), 0x000000);
        for rgb in [0x1E3A8A, 0x0F766E, 0xF1F5F9, 0x9F1239, 0x808080, 0x000000, 0xFFFFFF] {
            let (h, s, v) = to_hsv(rgb);
            assert_eq!(from_hsv(h, s, v), rgb, "{rgb:06X}");
        }
    }

    #[test]
    fn wheel_picks_and_points_agree() {
        let (h, s) = wheel_pick(0.0, 1.0);
        assert!((h - 90.0).abs() < 1e-3 && (s - 1.0).abs() < 1e-6);
        assert_eq!(wheel_pick(3.0, 0.0), (0.0, 1.0));
        let (x, y) = wheel_point(90.0, 0.5);
        assert!(x.abs() < 1e-6 && (y + 0.5).abs() < 1e-6);
    }

    #[test]
    fn wheel_bmp_has_a_transparent_corner_and_a_white_centre() {
        let size = 64;
        let bmp = wheel_bmp(size);
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(bmp.len(), 122 + (size * size * 4) as usize);
        let pixel = |x: u32, y: u32| {
            let i = 122 + ((y * size + x) * 4) as usize;
            [bmp[i], bmp[i + 1], bmp[i + 2], bmp[i + 3]]
        };
        assert_eq!(pixel(0, 0)[3], 0);
        let centre = pixel(size / 2, size / 2);
        assert!(centre.iter().all(|&c| c > 240), "{centre:?}");
    }
}

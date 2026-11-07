//! The Margin icon: a touchpad outline with two highlighted edges, drawn by a tiny software rasterizer.
//! Used at runtime for the tray and window icons, and by `build.rs` to make the exe's `.ico`.

/// Signed distance from `(px, py)` to a rounded rectangle centred at `(cx, cy)`; negative inside.
fn rounded_box(px: f32, py: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = (px - cx).abs() - (hw - r);
    let qy = (py - cy).abs() - (hh - r);
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}

/// A touchpad outline with two highlighted edges, as straight (non-premultiplied) RGBA pixels.
pub fn icon_pixels(size: i32) -> Vec<u8> {
    let s = size as f32;
    let u = s / 16.0;
    // body: x 1.5..14.5, y 3..13 (in 16-unit space), corner radius 1.5, pen 1.2
    let (bx, by, bw, bh) = (1.5 * u, 3.0 * u, 13.0 * u, 10.0 * u);
    let (cx, cy) = (bx + bw / 2.0, by + bh / 2.0);
    let radius = 1.5 * u;
    let pen = 1.2 * u;
    let bars = [
        (bx + 1.6 * u, by + 2.2 * u, 1.5 * u, 5.6 * u),
        (bx + bw - 3.1 * u, by + 2.2 * u, 1.5 * u, 5.6 * u),
    ];

    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let outline = (pen / 2.0 + 0.5 - rounded_box(px, py, cx, cy, bw / 2.0, bh / 2.0, radius).abs()).clamp(0.0, 1.0);
            let mut bar = 0.0f32;
            for &(rx, ry, rw, rh) in &bars {
                let d = rounded_box(px, py, rx + rw / 2.0, ry + rh / 2.0, rw / 2.0, rh / 2.0, 0.0);
                bar = bar.max((0.5 - d).clamp(0.0, 1.0));
            }
            // composite the orange bars over the white outline
            let a = bar + outline * (1.0 - bar);
            if a <= 0.0 {
                continue;
            }
            let mix = |orange: f32, white: f32| (orange * bar + white * outline * (1.0 - bar)) / a;
            let i = ((y * size + x) * 4) as usize;
            out[i] = mix(255.0, 255.0).round() as u8;
            out[i + 1] = mix(160.0, 255.0).round() as u8;
            out[i + 2] = mix(40.0, 255.0).round() as u8;
            out[i + 3] = (a * 255.0).round() as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artwork_has_an_outline_and_orange_bars() {
        let size = 32;
        let px = icon_pixels(size);
        let at = |x: i32, y: i32| {
            let i = ((y * size + x) * 4) as usize;
            (px[i], px[i + 1], px[i + 2], px[i + 3])
        };
        assert_eq!(at(0, 0).3, 0, "corner is transparent");
        // left edge of the outline (x = 1.5u = 3, mid height) is opaque white
        let o = at(3, 16);
        assert!(o.3 > 200 && o.0 > 240 && o.2 > 240, "{o:?}");
        // left orange bar centre: x = (1.5 + 1.6 + 0.75)u = 7.7, y = (3 + 2.2 + 2.8)u = 16
        let bar = at(7, 16);
        assert!(bar.3 > 200 && bar.0 > 240 && bar.2 < 80, "{bar:?}");
        // the middle of the pad is empty
        assert_eq!(at(16, 16).3, 0);
    }
}

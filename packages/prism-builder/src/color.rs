//! Wave 2.4 (HSL slider follow-up) of `docs/dev/composable-builder-plan.md`
//! — RGB ↔ HSL math + hex parse/format helpers for the
//! `shell.color-picker` overlay's H/S/L sliders.
//!
//! All operations work on `u8` RGB tuples and `(f32, f32, f32)` HSL
//! triples where `h ∈ [0, 360)`, `s ∈ [0, 100]`, `l ∈ [0, 100]`.
//! Alpha is preserved through `parse_hex` / `format_hex` so the
//! picker can round-trip `#rrggbbaa` colours without dropping the
//! alpha channel. The picker writes the colour back through
//! `set_color_picker_value` as a `#rrggbb` (or `#rrggbbaa` when the
//! original carried alpha) string — the shape `parse_color` already
//! consumes through `ui_lower::parse_color`.

/// Parsed hex color. `a` defaults to `0xff` (fully opaque) when the
/// input was a 6-digit `#rrggbb`. Hex inputs without the leading
/// `#` are accepted to ease testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const BLACK: Self = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 0xff,
    };
}

/// Parse a `#rrggbb` / `#rrggbbaa` / `#rgb` / `#rgba` hex string. The
/// leading `#` is optional. Returns `None` for malformed input so
/// callers can fall back without panicking (matches `parse_color`'s
/// contract — `set_color_picker_value` simply skips the commit when
/// parsing fails).
pub fn parse_hex(s: &str) -> Option<Rgba> {
    let bytes = s.trim().trim_start_matches('#').as_bytes();
    match bytes.len() {
        3 => {
            let r = hex_digit(bytes[0])? * 17;
            let g = hex_digit(bytes[1])? * 17;
            let b = hex_digit(bytes[2])? * 17;
            Some(Rgba { r, g, b, a: 0xff })
        }
        4 => {
            let r = hex_digit(bytes[0])? * 17;
            let g = hex_digit(bytes[1])? * 17;
            let b = hex_digit(bytes[2])? * 17;
            let a = hex_digit(bytes[3])? * 17;
            Some(Rgba { r, g, b, a })
        }
        6 => Some(Rgba {
            r: hex_byte(bytes[0], bytes[1])?,
            g: hex_byte(bytes[2], bytes[3])?,
            b: hex_byte(bytes[4], bytes[5])?,
            a: 0xff,
        }),
        8 => Some(Rgba {
            r: hex_byte(bytes[0], bytes[1])?,
            g: hex_byte(bytes[2], bytes[3])?,
            b: hex_byte(bytes[4], bytes[5])?,
            a: hex_byte(bytes[6], bytes[7])?,
        }),
        _ => None,
    }
}

/// Format an `Rgba` as `#rrggbb` (or `#rrggbbaa` when the alpha
/// channel is less than fully opaque). Round-trips through
/// `parse_hex` exactly.
pub fn format_hex(rgba: Rgba) -> String {
    if rgba.a == 0xff {
        format!("#{:02x}{:02x}{:02x}", rgba.r, rgba.g, rgba.b)
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            rgba.r, rgba.g, rgba.b, rgba.a
        )
    }
}

/// Convert RGB (0..255) to HSL (`h ∈ [0, 360)`, `s ∈ [0, 100]`,
/// `l ∈ [0, 100]`). Achromatic colours (r == g == b) return
/// `h = 0, s = 0`. The output is float-precision so the slider
/// round-trips cleanly: `hsl_to_rgb(rgb_to_hsl(rgb))` reproduces
/// `rgb` to within ±1 of each channel.
pub fn rgb_to_hsl(rgba: Rgba) -> (f32, f32, f32) {
    let r = rgba.r as f32 / 255.0;
    let g = rgba.g as f32 / 255.0;
    let b = rgba.b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l * 100.0);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - r).abs() < f32::EPSILON {
        let h6 = (g - b) / d + if g < b { 6.0 } else { 0.0 };
        h6 / 6.0
    } else if (max - g).abs() < f32::EPSILON {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h * 360.0, s * 100.0, l * 100.0)
}

/// Convert HSL back to RGB. Alpha is supplied separately — HSL has
/// no alpha channel. Inputs are clamped to their natural ranges to
/// keep the picker robust against floating-point drift.
pub fn hsl_to_rgb(h: f32, s: f32, l: f32, alpha: u8) -> Rgba {
    let h = h.rem_euclid(360.0) / 360.0;
    let s = (s / 100.0).clamp(0.0, 1.0);
    let l = (l / 100.0).clamp(0.0, 1.0);
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return Rgba {
            r: v,
            g: v,
            b: v,
            a: alpha,
        };
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);
    Rgba {
        r: (r * 255.0).round() as u8,
        g: (g * 255.0).round() as u8,
        b: (b * 255.0).round() as u8,
        a: alpha,
    }
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 0.5 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn hex_byte(hi: u8, lo: u8) -> Option<u8> {
    Some(hex_digit(hi)? * 16 + hex_digit(lo)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_six_digit_hex() {
        assert_eq!(
            parse_hex("#ff0080"),
            Some(Rgba {
                r: 0xff,
                g: 0,
                b: 0x80,
                a: 0xff
            })
        );
    }

    #[test]
    fn parse_eight_digit_hex_preserves_alpha() {
        assert_eq!(
            parse_hex("#11223344"),
            Some(Rgba {
                r: 0x11,
                g: 0x22,
                b: 0x33,
                a: 0x44
            })
        );
    }

    #[test]
    fn parse_short_hex_expands_nibbles() {
        assert_eq!(
            parse_hex("#abc"),
            Some(Rgba {
                r: 0xaa,
                g: 0xbb,
                b: 0xcc,
                a: 0xff
            })
        );
        assert_eq!(
            parse_hex("#abcd"),
            Some(Rgba {
                r: 0xaa,
                g: 0xbb,
                b: 0xcc,
                a: 0xdd
            })
        );
    }

    #[test]
    fn parse_hex_rejects_malformed_input() {
        assert!(parse_hex("not-a-color").is_none());
        assert!(parse_hex("#xy0080").is_none());
        assert!(parse_hex("#ff").is_none());
    }

    #[test]
    fn format_hex_drops_alpha_when_fully_opaque() {
        assert_eq!(
            format_hex(Rgba {
                r: 0xff,
                g: 0,
                b: 0x80,
                a: 0xff
            }),
            "#ff0080"
        );
    }

    #[test]
    fn format_hex_keeps_alpha_when_translucent() {
        assert_eq!(
            format_hex(Rgba {
                r: 0x11,
                g: 0x22,
                b: 0x33,
                a: 0x44
            }),
            "#11223344"
        );
    }

    #[test]
    fn rgb_hsl_round_trips_pure_red() {
        let red = Rgba {
            r: 0xff,
            g: 0,
            b: 0,
            a: 0xff,
        };
        let (h, s, l) = rgb_to_hsl(red);
        assert!((h - 0.0).abs() < 0.5);
        assert!((s - 100.0).abs() < 0.5);
        assert!((l - 50.0).abs() < 0.5);
        let back = hsl_to_rgb(h, s, l, 0xff);
        assert_eq!(back, red);
    }

    #[test]
    fn rgb_hsl_round_trips_grey() {
        let grey = Rgba {
            r: 0x80,
            g: 0x80,
            b: 0x80,
            a: 0xff,
        };
        let (h, s, l) = rgb_to_hsl(grey);
        assert!((h - 0.0).abs() < 0.5);
        assert!((s - 0.0).abs() < 0.5);
        let back = hsl_to_rgb(h, s, l, 0xff);
        // Allow ±1 in channels for the float round-trip.
        assert!((back.r as i32 - 0x80).abs() <= 1);
        assert!((back.g as i32 - 0x80).abs() <= 1);
        assert!((back.b as i32 - 0x80).abs() <= 1);
    }

    #[test]
    fn rgb_hsl_round_trips_arbitrary_color() {
        let c = Rgba {
            r: 0x33,
            g: 0xaa,
            b: 0x77,
            a: 0xff,
        };
        let (h, s, l) = rgb_to_hsl(c);
        let back = hsl_to_rgb(h, s, l, 0xff);
        assert!((back.r as i32 - c.r as i32).abs() <= 1);
        assert!((back.g as i32 - c.g as i32).abs() <= 1);
        assert!((back.b as i32 - c.b as i32).abs() <= 1);
    }
}

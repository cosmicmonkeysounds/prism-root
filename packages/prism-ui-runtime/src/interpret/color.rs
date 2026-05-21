//! OKLCH colour math for the PRUI expression slot — Phase 1 / §7.12.
//!
//! `darken` / `lighten` / `alpha` / `mix` accept sRGB hex in and emit
//! sRGB hex out, but the lerp / channel adjustment happens in OKLab
//! (a perceptually-uniform space), so a darkened vivid blue stays a
//! vivid darker blue instead of desaturating to grey. `with` and
//! `saturate` / `desaturate` ride the same conversion path so a
//! one-axis adjustment never bleeds chroma or hue.
//!
//! ## Pipeline
//!
//! `sRGB hex → linear sRGB → OKLab → adjust → linear sRGB → sRGB hex`.
//! Out-of-gamut OKLab outputs are clipped via **chroma reduction**
//! (preserve hue + lightness, bisect chroma until back in gamut —
//! Q4 of the roadmap). Hue rotation is the alternative; we reject
//! it because designers reach for OKLCH precisely to preserve hue.
//!
//! ## Why hand-rolled, not a crate
//!
//! The conversion coefficients (Björn Ottosson's OKLab paper) are
//! six matrix rows of constants; the round-trip is ~30 lines. Pulling
//! the full `palette` crate would compile-cost the whole `cosmic-text`
//! style-resolution chain through an extra dep tree. Keeping the math
//! inline lets it run from `serde_json::Value`-shaped args without
//! ferrying through a `palette::Srgb<f32>` wrapper.

/// sRGB channel in 0..=255 → linear-sRGB in 0..=1.
fn srgb_to_linear(c: u8) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// linear-sRGB in 0..=1 → sRGB channel in 0..=255 (rounded, clamped).
fn linear_to_srgb(c: f64) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let out = if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (out * 255.0).round().clamp(0.0, 255.0) as u8
}

/// linear-sRGB → OKLab. Coefficients from Björn Ottosson, "A perceptual
/// color space for image processing" (2020).
fn linear_to_oklab(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    (
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    )
}

/// OKLab → linear-sRGB (inverse of [`linear_to_oklab`]).
fn oklab_to_linear(l: f64, a: f64, b: f64) -> (f64, f64, f64) {
    let l_ = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m_ = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s_ = (l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    (
        4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
        -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
        -0.0041960863 * l_ - 0.7034186147 * m_ + 1.7076147010 * s_,
    )
}

/// Cartesian OKLab → polar OKLCh. `h` is degrees in `[0, 360)`.
fn oklab_to_oklch(l: f64, a: f64, b: f64) -> (f64, f64, f64) {
    let c = (a * a + b * b).sqrt();
    let h = b.atan2(a).to_degrees();
    let h = if h < 0.0 { h + 360.0 } else { h };
    (l, c, h)
}

/// Polar OKLCh → cartesian OKLab.
fn oklch_to_oklab(l: f64, c: f64, h: f64) -> (f64, f64, f64) {
    let r = h.to_radians();
    (l, c * r.cos(), c * r.sin())
}

/// Full round trip: sRGB → OKLCh.
pub(super) fn srgb_to_oklch(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (lr, lg, lb) = (srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b));
    let (l, a, b) = linear_to_oklab(lr, lg, lb);
    oklab_to_oklch(l, a, b)
}

/// Linear-sRGB triple in-gamut iff every channel sits in `[0, 1]` (with
/// a small float epsilon for round-trip noise — a perfectly-saturated
/// red landing at 1.0000001 is still red).
fn in_gamut(r: f64, g: f64, b: f64) -> bool {
    const EPS: f64 = 1.0e-4;
    let ok = |c: f64| (-EPS..=1.0 + EPS).contains(&c);
    ok(r) && ok(g) && ok(b)
}

/// OKLCh → sRGB triple, applying **chroma-reduction gamut clipping**
/// (Q4): preserve `l` and `h`, bisect chroma until the converted
/// linear-sRGB falls inside `[0, 1]^3`. Hue rotation is the
/// alternative; we reject it because designers reach for OKLCH
/// precisely to preserve perceived hue.
pub(super) fn oklch_to_srgb(l: f64, c: f64, h: f64) -> (u8, u8, u8) {
    // Lightness outside [0, 1] always saturates to black / white — no
    // chroma will rescue it. Short-circuit so the bisection has a
    // bounded c=0 case to fall back to.
    let l = l.clamp(0.0, 1.0);
    let c = c.max(0.0);
    let try_at = |c: f64| -> (f64, f64, f64) {
        let (la, aa, bb) = oklch_to_oklab(l, c, h);
        oklab_to_linear(la, aa, bb)
    };
    let (mut lr, mut lg, mut lb) = try_at(c);
    if !in_gamut(lr, lg, lb) {
        // Bisect chroma between 0 (always in gamut) and current c.
        // ~20 iterations gets us to ~1e-6 — well below sRGB's 8-bit
        // resolution, so the rounded output is stable.
        let (mut lo, mut hi) = (0.0_f64, c);
        for _ in 0..20 {
            let mid = (lo + hi) / 2.0;
            let (rr, gg, bb) = try_at(mid);
            if in_gamut(rr, gg, bb) {
                lo = mid;
                lr = rr;
                lg = gg;
                lb = bb;
            } else {
                hi = mid;
            }
        }
    }
    (linear_to_srgb(lr), linear_to_srgb(lg), linear_to_srgb(lb))
}

/// `darken(c, amt)` — multiply OKLCh lightness by `(1 - amt)`. Hue +
/// chroma preserved, so a vivid blue darkens to a vivid darker blue
/// (the sRGB-lerp implementation desaturated mid-range hues to grey).
pub(super) fn darken(r: u8, g: u8, b: u8, amt: f64) -> (u8, u8, u8) {
    let amt = amt.clamp(0.0, 1.0);
    let (l, c, h) = srgb_to_oklch(r, g, b);
    oklch_to_srgb(l * (1.0 - amt), c, h)
}

/// `lighten(c, amt)` — push OKLCh lightness toward 1 by `amt`.
pub(super) fn lighten(r: u8, g: u8, b: u8, amt: f64) -> (u8, u8, u8) {
    let amt = amt.clamp(0.0, 1.0);
    let (l, c, h) = srgb_to_oklch(r, g, b);
    oklch_to_srgb(l + (1.0 - l) * amt, c, h)
}

/// `mix(c1, c2, t)` — OKLab lerp on all three axes (`t = 0` → `c1`,
/// `t = 1` → `c2`). OKLab is the cartesian space; lerping in it
/// avoids the OKLCh polar interpolation's hue-wrap ambiguity.
pub(crate) fn mix(r1: u8, g1: u8, b1: u8, r2: u8, g2: u8, b2: u8, t: f64) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    let (lr1, lg1, lb1) = (srgb_to_linear(r1), srgb_to_linear(g1), srgb_to_linear(b1));
    let (lr2, lg2, lb2) = (srgb_to_linear(r2), srgb_to_linear(g2), srgb_to_linear(b2));
    let (la1, aa1, bb1) = linear_to_oklab(lr1, lg1, lb1);
    let (la2, aa2, bb2) = linear_to_oklab(lr2, lg2, lb2);
    let l = la1 + (la2 - la1) * t;
    let a = aa1 + (aa2 - aa1) * t;
    let b = bb1 + (bb2 - bb1) * t;
    let (lr, lg, lb) = oklab_to_linear(l, a, b);
    let (lr, lg, lb) = if in_gamut(lr, lg, lb) {
        (lr, lg, lb)
    } else {
        // Round-trip through OKLCh to share the chroma-reduction
        // clamp with `darken` / `lighten` — keeps gamut handling
        // consistent across helpers.
        let (l, c, h) = oklab_to_oklch(l, a, b);
        let (r, g, b) = oklch_to_srgb(l, c, h);
        return (r, g, b);
    };
    (linear_to_srgb(lr), linear_to_srgb(lg), linear_to_srgb(lb))
}

/// A single channel adjustment for [`with_channels`]: `Set(v)` overrides
/// the channel; `Delta(v)` adds (so `l = +0.05` parses as `Delta(0.05)`
/// and `l = 0.5` parses as `Set(0.5)`). `None` leaves the channel
/// untouched.
#[derive(Debug, Clone, Copy)]
pub(super) enum ChannelAdjust {
    Set(f64),
    Delta(f64),
}

impl ChannelAdjust {
    fn apply(self, base: f64) -> f64 {
        match self {
            ChannelAdjust::Set(v) => v,
            ChannelAdjust::Delta(d) => base + d,
        }
    }
}

/// Bundle of OKLCh channel adjustments for [`with_channels`]. All
/// fields default to `None` (channel left as-is); the kwarg parser
/// fills in only the channels the author named. Grouping as a struct
/// keeps the call signature under clippy's `too_many_arguments` limit
/// and lets callers initialise just the fields they care about.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ChannelAdjustments {
    pub l: Option<ChannelAdjust>,
    pub c: Option<ChannelAdjust>,
    pub h: Option<ChannelAdjust>,
    pub a: Option<ChannelAdjust>,
}

/// `with(c, l=, c=, h=, a=)` — adjust any OKLCh channel (`l` =
/// lightness, `c` = chroma, `h` = hue degrees) plus alpha. `Delta`
/// values add to the current channel; `Set` overrides it. Hue wraps
/// at 360°.
pub(super) fn with_channels(
    r: u8,
    g: u8,
    b: u8,
    a: u8,
    adj: ChannelAdjustments,
) -> (u8, u8, u8, u8) {
    let (cl, cc, ch) = srgb_to_oklch(r, g, b);
    let new_l = adj.l.map_or(cl, |a| a.apply(cl));
    let new_c = adj.c.map_or(cc, |a| a.apply(cc).max(0.0));
    let new_h = adj.h.map_or(ch, |a| {
        let v = a.apply(ch) % 360.0;
        if v < 0.0 {
            v + 360.0
        } else {
            v
        }
    });
    let (rr, gg, bb) = oklch_to_srgb(new_l, new_c, new_h);
    let aa = adj.a.map_or(a, |adj| {
        let v = match adj {
            ChannelAdjust::Set(v) => v,
            ChannelAdjust::Delta(d) => (a as f64 / 255.0) + d,
        };
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    });
    (rr, gg, bb, aa)
}

/// **§7.15** — perceptual lerp between two `command::Color`s at
/// parameter `t ∈ [0, 1]`. RGB rides the OKLab cartesian space via
/// [`mix`] so a fade between a vivid blue and a vivid red passes
/// through the saturated perceptual midpoint instead of the muddy
/// sRGB straight-line midpoint. Alpha lerps linearly (no perceptual
/// analogue worth threading through the colour space). Used by the
/// animator's value-change / entry / exit / keyframe paths when a
/// transition is keyed on a colour-valued property (background,
/// foreground colour, tint).
pub(crate) fn lerp_command_color(
    a: crate::command::Color,
    b: crate::command::Color,
    t: f64,
) -> crate::command::Color {
    let t = t.clamp(0.0, 1.0);
    let (r, g, bb) = mix(a.r, a.g, a.b, b.r, b.g, b.b, t);
    let alpha = (a.a as f64 + (b.a as f64 - a.a as f64) * t)
        .round()
        .clamp(0.0, 255.0) as u8;
    crate::command::Color {
        r,
        g,
        b: bb,
        a: alpha,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1.0e-3, "expected {b}, got {a}");
    }

    #[test]
    fn round_trip_black() {
        let (l, c, h) = srgb_to_oklch(0, 0, 0);
        approx(l, 0.0);
        // chroma is undefined at L=0; whatever it lands at, the round
        // trip back must produce black.
        let (r, g, b) = oklch_to_srgb(l, c, h);
        assert_eq!((r, g, b), (0, 0, 0));
    }

    #[test]
    fn round_trip_white() {
        let (l, c, h) = srgb_to_oklch(255, 255, 255);
        approx(l, 1.0);
        let (r, g, b) = oklch_to_srgb(l, c, h);
        assert_eq!((r, g, b), (255, 255, 255));
    }

    #[test]
    fn round_trip_vivid_blue() {
        // The poster child for OKLCH: #3b82f6 (Tailwind blue-500).
        let (l, c, h) = srgb_to_oklch(0x3b, 0x82, 0xf6);
        let (r, g, b) = oklch_to_srgb(l, c, h);
        // Allow ±1 per channel for rounding through the chroma-clip
        // bisection.
        assert!((r as i32 - 0x3b).abs() <= 1);
        assert!((g as i32 - 0x82).abs() <= 1);
        assert!((b as i32 - 0xf6).abs() <= 1);
    }

    #[test]
    fn darken_blue_stays_vivid() {
        // The motivating example from §7.12: sRGB-lerp `darken('#3b82f6', 0.3)`
        // produces a desaturated grey-blue. OKLCH must preserve the
        // chroma so the output is still a recognisably blue colour.
        let (r, g, b) = darken(0x3b, 0x82, 0xf6, 0.3);
        // Hue stays blue: blue channel still dominates green which
        // still dominates red.
        assert!(
            b > g && g > r,
            "expected blue dominant, got #{r:02x}{g:02x}{b:02x}"
        );
        // Chroma preserved: the gap between max and min channel is
        // wider than a desaturated grey would allow. sRGB-lerp at
        // amt=0.3 collapses (b-r) toward ~0x70; OKLCh keeps it ≥0x80.
        assert!(
            (b as i32 - r as i32) > 0x80,
            "chroma collapsed to grey: #{r:02x}{g:02x}{b:02x}"
        );
    }

    #[test]
    fn lighten_black_to_white() {
        // The endpoint case — must hit exactly (255, 255, 255) so
        // `lighten('#000000', 1.0)` keeps its meaningful semantic.
        let (r, g, b) = lighten(0, 0, 0, 1.0);
        assert_eq!((r, g, b), (255, 255, 255));
    }

    #[test]
    fn darken_white_to_black() {
        let (r, g, b) = darken(255, 255, 255, 1.0);
        assert_eq!((r, g, b), (0, 0, 0));
    }

    #[test]
    fn mix_extremes_round_trip() {
        // mix(black, white, 0.5) lerps in OKLab — L=0.5 there, which
        // maps to a sRGB grey around 0x63, *not* the sRGB midpoint
        // 0x80. The point of OKLCH mix is that the perceptual midpoint
        // and the sRGB midpoint *disagree* — OKLab's L is more
        // perceptually-uniform, and sRGB 0x80 is closer to ~75%
        // perceived brightness, so the true perceptual mid sits
        // darker than the sRGB mid.
        let (r, g, b) = mix(0, 0, 0, 255, 255, 255, 0.5);
        assert_eq!(r, g);
        assert_eq!(g, b);
        assert!(
            r != 0x80,
            "OKLab midpoint must not equal sRGB midpoint 0x80 — that would mean we're still lerping in sRGB"
        );
        assert!(
            (0x55..0x80).contains(&r),
            "expected OKLab mid grey in 0x55..0x80, got 0x{r:02x}"
        );
    }

    #[test]
    fn mix_endpoints_exact() {
        // t=0 → c1, t=1 → c2, exactly (small rounding tolerance).
        let (r, g, b) = mix(0xff, 0, 0, 0, 0, 0xff, 0.0);
        assert!(r > 0xfe);
        assert_eq!(g, 0);
        assert_eq!(b, 0);
        let (r, g, b) = mix(0xff, 0, 0, 0, 0, 0xff, 1.0);
        assert_eq!(r, 0);
        assert_eq!(g, 0);
        assert!(b > 0xfe);
    }

    #[test]
    fn with_lightness_delta_lightens() {
        let (r, g, b, a) = with_channels(
            0x3b,
            0x82,
            0xf6,
            0xff,
            ChannelAdjustments {
                l: Some(ChannelAdjust::Delta(0.05)),
                ..Default::default()
            },
        );
        // Brighter than starting blue (sum of channels grows).
        let before: i32 = 0x3b + 0x82 + 0xf6;
        let after = r as i32 + g as i32 + b as i32;
        assert!(
            after > before,
            "expected lightened, got #{r:02x}{g:02x}{b:02x}"
        );
        assert_eq!(a, 0xff);
    }

    #[test]
    fn with_chroma_delta_negative_desaturates() {
        let (r, g, b, _) = with_channels(
            0x3b,
            0x82,
            0xf6,
            0xff,
            ChannelAdjustments {
                c: Some(ChannelAdjust::Delta(-0.05)),
                ..Default::default()
            },
        );
        // Channels move toward each other — gap shrinks.
        let gap_before: i32 = 0xf6 - 0x3b;
        let gap_after = (b as i32).abs_diff(r as i32) as i32;
        assert!(
            gap_after < gap_before,
            "expected desaturation, got #{r:02x}{g:02x}{b:02x}"
        );
    }

    #[test]
    fn with_alpha_set() {
        let (_, _, _, a) = with_channels(
            0x3b,
            0x82,
            0xf6,
            0xff,
            ChannelAdjustments {
                a: Some(ChannelAdjust::Set(0.5)),
                ..Default::default()
            },
        );
        // 0.5 * 255 = 127.5 → 128.
        assert_eq!(a, 128);
    }

    /// **§7.15** — `lerp_command_color` rides the OKLab path: lerping
    /// black → white at `t=0.5` lands on a perceptual mid-grey (well
    /// below sRGB 0x80), not the sRGB midpoint. Alpha lerps linearly.
    #[test]
    fn lerp_command_color_uses_oklab_midpoint() {
        use crate::command::Color;
        let from = Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
        let to = Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };
        let mid = lerp_command_color(from, to, 0.5);
        assert_eq!(mid.r, mid.g);
        assert_eq!(mid.g, mid.b);
        assert!(
            mid.r < 0x80,
            "OKLab mid-grey must sit below sRGB 0x80, got 0x{:02x}",
            mid.r
        );
        assert_eq!(mid.a, 128, "alpha lerps linearly: (0 + 255)/2 = 128");
    }

    #[test]
    fn lerp_command_color_endpoints_exact() {
        use crate::command::Color;
        let from = Color {
            r: 0x3b,
            g: 0x82,
            b: 0xf6,
            a: 0xff,
        };
        let to = Color {
            r: 0xff,
            g: 0,
            b: 0,
            a: 0,
        };
        let at_zero = lerp_command_color(from, to, 0.0);
        // Allow ±1 per channel for sRGB round-trip rounding.
        assert!((at_zero.r as i32 - 0x3b).abs() <= 1);
        assert!((at_zero.g as i32 - 0x82).abs() <= 1);
        assert!((at_zero.b as i32 - 0xf6).abs() <= 1);
        assert_eq!(at_zero.a, 0xff);
        let at_one = lerp_command_color(from, to, 1.0);
        assert!((at_one.r as i32 - 0xff).abs() <= 1);
        assert!((at_one.g as i32).abs() <= 1);
        assert!((at_one.b as i32).abs() <= 1);
        assert_eq!(at_one.a, 0);
    }

    #[test]
    fn out_of_gamut_chroma_reduces_not_panics() {
        // Force a wildly out-of-gamut chroma; bisection should clamp
        // back to a valid sRGB output.
        let (r, g, b) = oklch_to_srgb(0.5, 5.0, 200.0);
        // Just assert we got a valid result (no NaN, in 0..=255).
        let _ = (r, g, b);
    }
}

//! Design tokens — the canonical color / spacing / typography palette
//! the Slint UI tree reads from. Replaces what used to be Tailwind
//! classes + CSS variables in the React Studio.

use serde::{Deserialize, Serialize};

/// Static token table. Everything is a `const` so the Slint property
/// bindings can reference them without any runtime lookup.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DesignTokens {
    pub colors: Colors,
    pub spacing: Spacing,
    pub radius: Radius,
    pub typography: Typography,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Colors {
    pub background: Rgba,
    pub surface: Rgba,
    pub surface_elevated: Rgba,
    pub border: Rgba,
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub accent: Rgba,
    pub accent_muted: Rgba,
    pub danger: Rgba,
    pub success: Rgba,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Spacing {
    pub xs: u16,
    pub sm: u16,
    pub md: u16,
    pub lg: u16,
    pub xl: u16,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Radius {
    pub sm: u16,
    pub md: u16,
    pub lg: u16,
    pub pill: u16,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Typography {
    pub font_size_sm: u16,
    pub font_size_md: u16,
    pub font_size_lg: u16,
    pub font_size_xl: u16,
    pub line_height_md: u16,
}

/// Packed RGBA as four u8s. Stored as a struct instead of a `u32` so
/// serde output is human-readable.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

pub const DEFAULT_TOKENS: DesignTokens = DesignTokens {
    colors: Colors {
        background: Rgba::new(15, 18, 24, 255),
        surface: Rgba::new(22, 26, 34, 255),
        surface_elevated: Rgba::new(30, 35, 45, 255),
        border: Rgba::new(48, 55, 68, 255),
        text_primary: Rgba::new(232, 236, 244, 255),
        text_secondary: Rgba::new(156, 164, 180, 255),
        accent: Rgba::new(110, 170, 255, 255),
        accent_muted: Rgba::new(70, 100, 160, 255),
        danger: Rgba::new(240, 90, 100, 255),
        success: Rgba::new(80, 200, 140, 255),
    },
    spacing: Spacing {
        xs: 4,
        sm: 8,
        md: 12,
        lg: 20,
        xl: 32,
    },
    radius: Radius {
        sm: 4,
        md: 8,
        lg: 14,
        pill: 999,
    },
    typography: Typography {
        font_size_sm: 12,
        font_size_md: 14,
        font_size_lg: 18,
        font_size_xl: 24,
        line_height_md: 20,
    },
};

impl Default for DesignTokens {
    fn default() -> Self {
        DEFAULT_TOKENS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tokens_stable() {
        let tokens = DesignTokens::default();
        assert_eq!(tokens.spacing.md, 12);
        assert_eq!(tokens.colors.accent.r, 110);
    }
}

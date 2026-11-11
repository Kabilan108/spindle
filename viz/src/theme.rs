//! Copied from `src/ui/theme.rs`: the lab draws in the app's own palette and type.

//! Baliset's palette and type. Neutral greys with a faint warmth, one brass accent; album art
//! carries the rest of the colour.

use rune::{Color, Spring, TextStyle, oklch, oklcha};

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color,
    pub sidebar: Color,
    pub bar: Color,
    pub raised: Color,
    pub field: Color,
    pub line: Color,
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub hover: Color,
    pub press: Color,
    pub selected: Color,
    pub art_empty: Color,
    pub danger: Color,
    pub warn_bg: Color,
    pub warn_text: Color,
    pub toast: Color,
    pub toast_text: Color,
    pub shadow: Color,
    pub focus: Color,
}

impl Palette {
    pub fn new(dark: bool) -> Self {
        if dark {
            Palette {
                dark,
                bg: oklch(0.185, 0.004, 70.0),
                sidebar: oklch(0.155, 0.004, 70.0),
                bar: oklch(0.165, 0.004, 70.0),
                raised: oklch(0.235, 0.005, 70.0),
                field: oklch(0.225, 0.005, 70.0),
                line: oklch(0.27, 0.005, 70.0),
                text: oklch(0.945, 0.004, 80.0),
                muted: oklch(0.72, 0.006, 80.0),
                faint: oklch(0.56, 0.006, 80.0),
                accent: oklch(0.80, 0.115, 75.0),
                on_accent: oklch(0.2, 0.02, 70.0),
                hover: oklcha(1.0, 0.0, 0.0, 0.055),
                press: oklcha(1.0, 0.0, 0.0, 0.09),
                selected: oklcha(1.0, 0.0, 0.0, 0.085),
                art_empty: oklch(0.27, 0.006, 70.0),
                danger: oklch(0.72, 0.15, 25.0),
                warn_bg: oklch(0.32, 0.06, 70.0),
                warn_text: oklch(0.92, 0.05, 80.0),
                toast: oklch(0.29, 0.006, 70.0),
                toast_text: oklch(0.95, 0.004, 80.0),
                shadow: oklcha(0.0, 0.0, 0.0, 0.45),
                focus: oklch(0.72, 0.12, 250.0),
            }
        } else {
            Palette {
                dark,
                bg: oklch(0.99, 0.002, 80.0),
                sidebar: oklch(0.965, 0.003, 80.0),
                bar: oklch(0.975, 0.003, 80.0),
                raised: oklch(0.935, 0.004, 80.0),
                field: oklch(0.945, 0.004, 80.0),
                line: oklch(0.90, 0.004, 80.0),
                text: oklch(0.22, 0.005, 70.0),
                muted: oklch(0.46, 0.006, 70.0),
                faint: oklch(0.60, 0.006, 70.0),
                accent: oklch(0.56, 0.12, 65.0),
                on_accent: oklch(0.99, 0.004, 80.0),
                hover: oklcha(0.0, 0.0, 0.0, 0.045),
                press: oklcha(0.0, 0.0, 0.0, 0.08),
                selected: oklcha(0.0, 0.0, 0.0, 0.07),
                art_empty: oklch(0.91, 0.005, 80.0),
                danger: oklch(0.55, 0.18, 25.0),
                warn_bg: oklch(0.93, 0.06, 85.0),
                warn_text: oklch(0.38, 0.07, 65.0),
                toast: oklch(0.26, 0.006, 70.0),
                toast_text: oklch(0.97, 0.004, 80.0),
                shadow: oklcha(0.0, 0.0, 0.0, 0.18),
                focus: oklch(0.58, 0.15, 250.0),
            }
        }
    }
}

pub mod font {
    use super::TextStyle;

    pub const PAGE_TITLE: TextStyle = TextStyle::new(30.0, 700.0).lh(36.0).tracking(-0.4);
    pub const SECTION: TextStyle = TextStyle::new(16.0, 650.0).lh(22.0);
    pub const BRAND: TextStyle = TextStyle::new(14.0, 700.0).tracking(0.2);
    pub const NAV: TextStyle = TextStyle::new(13.0, 500.0).lh(18.0);
    pub const BODY: TextStyle = TextStyle::new(13.0, 400.0).lh(18.0);
    pub const ROW_TITLE: TextStyle = TextStyle::new(13.0, 500.0).lh(17.0);
    pub const ROW_SUB: TextStyle = TextStyle::new(12.0, 400.0).lh(15.0);
    pub const CARD_TITLE: TextStyle = TextStyle::new(13.0, 600.0).lh(18.0);
    pub const SMALL: TextStyle = TextStyle::new(12.0, 400.0).lh(16.0);
    pub const CAPS: TextStyle = TextStyle::new(11.0, 600.0).lh(14.0).tracking(0.6);
    pub const BUTTON: TextStyle = TextStyle::new(13.0, 600.0).lh(18.0);
    pub const TIME: TextStyle = TextStyle::new(11.5, 500.0).lh(14.0).features("'tnum' 1");
    pub const NUMBER: TextStyle = TextStyle::new(12.5, 400.0).lh(16.0).features("'tnum' 1");
    pub const MONO: TextStyle = TextStyle::new(12.0, 400.0).lh(17.0).mono();
}

/// State changes land on this: quick, no bounce.
pub const SPRING: Spring = Spring::new(0.24, 0.0);

pub const SIDEBAR_W: f32 = 232.0;
pub const BAR_H: f32 = 76.0;
pub const ROW_H: f32 = 40.0;

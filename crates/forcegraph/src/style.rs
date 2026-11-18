//! Renderer knobs. Mirrors `accrete::ForceGraphStyle` (the DSL side owns the
//! wire shape; this crate stays free of the DSL), so the mapping between the
//! two happens once, in the host component.

use crate::force;

/// Which background the graph is drawn on.
///
/// One set of hues cannot be readable on both white and near-black: Morandi
/// colours are low-chroma by definition, so a set that has enough contrast on a
/// light page turns to mud on a dark one, and the lighter set disappears on
/// white. Hence two palettes of the same six muted families, one darker, one
/// lighter, and a switch that picks both the palette and the background it was
/// drawn against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    Light,
    /// The graph is drawn as light: orbs for nodes, additive strokes for edges,
    /// and light needs something to be added *to*. Dark is where it is meant to
    /// be read, so it is the default.
    #[default]
    Dark,
}

/// Morandi on a light page: greyed sage, clay, slate, camel, mauve, moss. Chroma
/// is kept low (the point of the palette) and lightness low enough to hold
/// contrast against white.
const PALETTE_LIGHT: [&str; 6] = [
    "#6f7f74", "#8a7a72", "#71818f", "#94795f", "#7f7389", "#6d7f74",
];

/// The same six families lifted for a dark page. Measured against the panel
/// (`#12161c`) they land at 5.9-6.7:1, which is enough to read without the glow
/// a brighter set would give — a Morandi palette that glares is not one.
const PALETTE_DARK: [&str; 6] = [
    "#93a094", "#9d9089", "#8395a8", "#ac907f", "#9d8fae", "#8aa094",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub node_size_px: f32,
    pub node_size_step_px: f32,
    pub node_tiers: u8,
    pub link_width_px: f32,
    /// How a link is drawn between two nodes. `Straight` runs centre to centre;
    /// `Axes` and `Tetra` route it out of a port on each node (see `LinkShape`).
    pub link_shape: crate::frame::LinkShape,
    /// Node ball diameter as a multiple of the link width. This is the knob that
    /// keeps dots and lines in proportion: at 2 the ball is twice as wide as the
    /// line it sits on, which is what the eye reads as "a dot on a thread".
    pub node_size_ratio: f32,
    pub link_opacity: f32,
    pub charge: f32,
    pub link_distance: f32,
    /// `None` keeps d3's `1 / min(degree)`.
    pub link_strength: Option<f32>,
    pub velocity_decay: f32,
    pub labels: bool,
    pub label_px: f32,
    pub label_max_nodes: u32,
    pub auto_fit: bool,
    pub fit_padding_px: f32,
    pub background: Option<String>,
    pub palette: Vec<String>,
    pub theme: Theme,
    /// How strongly a highlighted node marks itself: the core halo's opacity, and
    /// the peak opacity of each ripple ring.
    pub glow_alpha: f32,
    /// Light up the paths reachable from the highlighted node(s) instead of
    /// dimming everything else. Off by default: it is a reading aid, and a graph
    /// that is mostly one connected component would light up whole.
    pub highlight_reach: bool,
    /// How many edges out from the highlight count as "reachable".
    pub reach_hops: u32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            // 0 means "derive the radius from `node_size_ratio`": a radius set by
            // hand here would silently fight the ratio knob.
            node_size_px: 0.0,
            node_size_step_px: 0.6,
            node_tiers: 3,
            // 2.5 px / 0.6 alpha: at 1.5 px and 0.35 the edges were drawn but
            // invisible on a light page (measured: 540 → 2486 coloured pixels
            // when the width went to 16 px).
            link_width_px: 2.5,
            link_shape: crate::frame::LinkShape::Straight,
            node_size_ratio: 2.0,
            // 0.8 on a light page is 2.9-3.1:1 against white once the muted hues
            // are composited — below this the edges read as "not drawn at all",
            // which is the complaint that produced the 2.5px width.
            link_opacity: 0.8,
            // 3d-force-graph's defaults, i.e. d3-force-3d's: charge -30,
            // link distance 30, velocity decay 0.4. Inventing these numbers by
            // eye is what produced a layout that looked wrong at every zoom.
            charge: -30.0,
            link_distance: 30.0,
            link_strength: None,
            velocity_decay: 0.4,
            labels: true,
            label_px: 12.0,
            label_max_nodes: 150,
            auto_fit: true,
            fit_padding_px: 40.0,
            background: None,
            palette: Vec::new(),
            theme: Theme::default(),
            glow_alpha: 0.45,
            highlight_reach: true,
            reach_hops: 1,
        }
    }
}

impl Style {
    pub fn params(&self) -> force::Params {
        force::Params {
            charge: self.charge,
            link_distance: self.link_distance,
            link_strength: self.link_strength,
            velocity_decay: self.velocity_decay,
            ..force::Params::default()
        }
    }

    /// Node radius in CSS pixels: the weight is normalised against the
    /// heaviest node in the scene and quantised into `node_tiers` sizes.
    /// Without `max_weight` a lone weight would have to invent a scale; the
    /// heaviest node is the only reference the graph itself carries.
    pub fn radius_px(&self, weight: f32, max_weight: f32) -> f32 {
        let tiers = self.node_tiers.max(1) as f32;
        let spread = (max_weight - 1.0).max(f32::EPSILON);
        let t = ((weight.max(1.0) - 1.0) / spread).clamp(0.0, 1.0);
        let step = (t * (tiers - 1.0)).round();
        let base = if self.node_size_px > 0.0 {
            self.node_size_px
        } else {
            // Diameter = ratio × link width, so the radius is half of that.
            self.link_width_px * self.node_size_ratio.max(0.2) * 0.5
        };
        (base + self.node_size_step_px * step).max(1.0)
    }

    /// The palette this theme draws with, unless the caller supplied one.
    pub fn palette_for(&self) -> Vec<&str> {
        if !self.palette.is_empty() {
            return self.palette.iter().map(String::as_str).collect();
        }
        match self.theme {
            Theme::Light => PALETTE_LIGHT.to_vec(),
            Theme::Dark => PALETTE_DARK.to_vec(),
        }
    }

    /// Edge alpha. A muted hue at the light page's alpha is nearly invisible on
    /// white (2.1:1), while the same alpha on the dark panel is already 3.5:1, so
    /// the dark theme scales it down rather than needing a second knob.
    pub fn link_alpha(&self) -> f32 {
        let alpha = self.link_opacity.clamp(0.05, 1.0);
        match self.theme {
            Theme::Light => alpha,
            // On a dark panel the link *adds* light, so it can carry close to the
            // full opacity: nothing is being covered, the stroke is being lit.
            Theme::Dark => (alpha * 0.95).clamp(0.05, 1.0),
        }
    }

    /// Panel background this theme is drawn against. `None` means "let the page
    /// show through" — which is what a light theme wants on a light page.
    pub fn background(&self) -> Option<String> {
        self.background.clone().or_else(|| match self.theme {
            Theme::Light => None,
            Theme::Dark => Some("#12161c".to_string()),
        })
    }

    /// Label foreground and its scrim. Labels sit on the same surface as the
    /// graph, so both halves have to follow the theme: a white scrim over a dark
    /// panel is the thing that makes a dark mode look broken.
    pub fn label_colors(&self) -> (&'static str, &'static str) {
        match self.theme {
            Theme::Light => ("#3b4149", "rgba(255,255,255,0.72)"),
            Theme::Dark => ("#d6dbe2", "rgba(16,20,26,0.62)"),
        }
    }

    pub fn toggle_theme(&mut self) {
        self.theme = match self.theme {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        };
    }

    /// Colour for a node kind, cycling the palette in first-seen order.
    pub fn color_for(&self, kind: &str) -> [f32; 3] {
        let palette = self.palette_for();
        let slot = kind.bytes().fold(0_usize, |acc, b| acc + b as usize) % palette.len().max(1);
        palette
            .get(slot)
            .and_then(|hex| parse_hex(hex))
            .unwrap_or([0.25, 0.35, 0.55])
    }
}

/// `#rrggbb` → linear-ish floats in 0..1. Anything else returns `None` and the
/// caller falls back to the built-in colour instead of rendering black.
pub fn parse_hex(hex: &str) -> Option<[f32; 3]> {
    let body = hex.strip_prefix('#').unwrap_or(hex);
    if body.len() != 6 {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(body.get(i..i + 2)?, 16).ok();
    Some([
        channel(0)? as f32 / 255.0,
        channel(2)? as f32 / 255.0,
        channel(4)? as f32 / 255.0,
    ])
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn weights_quantise_into_tiers() {
        let style = Style::default();
        // 1 is the floor, 9 is the heaviest node of this scene, 5 lands midway.
        // Three tiers, so the diameters run 5.2 / 5.9 / 6.6 px — about twice the
        // 2.5 px link width, which is the ratio the eye reads as "a dot on a line".
        // Diameter is twice the 2.5 px link width, so the small end has radius 2.5;
        // three tiers add 0.6 each.
        assert!((style.radius_px(1.0, 9.0) - 2.5).abs() < 1e-6);
        assert!((style.radius_px(5.0, 9.0) - 3.1).abs() < 1e-6);
        assert!((style.radius_px(9.0, 9.0) - 3.7).abs() < 1e-6);
        // A weight beyond the reference still stops at the last tier.
        assert!((style.radius_px(99.0, 9.0) - 3.7).abs() < 1e-6);
    }

    /// The palette has to be muted on purpose and the dark set has to be the
    /// lighter one, or the switch does nothing but rename a colour.
    #[test]
    fn both_palettes_are_muted_and_the_dark_one_is_lighter() {
        let mut style = Style::default();
        style.theme = Theme::Light;
        let lum = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let chroma = |c: [f32; 3]| {
            let hi = c.iter().copied().fold(0.0_f32, f32::max);
            let lo = c.iter().copied().fold(1.0_f32, f32::min);
            hi - lo
        };
        let light: Vec<[f32; 3]> = style
            .palette_for()
            .iter()
            .map(|h| parse_hex(h).unwrap())
            .collect();
        style.theme = Theme::Dark;
        let dark: Vec<[f32; 3]> = style
            .palette_for()
            .iter()
            .map(|h| parse_hex(h).unwrap())
            .collect();

        for color in light.iter().chain(dark.iter()) {
            assert!(chroma(*color) < 0.36, "not muted: {color:?}");
        }
        let mean = |set: &[[f32; 3]]| set.iter().map(|c| lum(*c)).sum::<f32>() / set.len() as f32;
        assert!(
            mean(&dark) > mean(&light) + 0.08,
            "dark palette should be lighter: {} vs {}",
            mean(&dark),
            mean(&light)
        );
        assert!(style.background().is_some(), "a dark theme paints a panel");
        assert_eq!(light.len(), 6);

        // Both themes have to be legible against their own panel, or the switch
        // only trades one unreadable surface for another.
        let on_panel = (0x12, 0x16, 0x1c);
        let rel = |rgb: [f32; 3]| {
            let channel = |v: f32| {
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2])
        };
        let ratio = |a: f32, b: f32| (a.max(b) + 0.05) / (a.min(b) + 0.05);
        let white = rel([1.0, 1.0, 1.0]);
        let panel = rel([
            on_panel.0 as f32 / 255.0,
            on_panel.1 as f32 / 255.0,
            on_panel.2 as f32 / 255.0,
        ]);
        for color in &light {
            let r = ratio(rel(*color), white);
            assert!(r >= 3.0, "light theme node {color:?} is {r:.2}:1 on white");
        }
        for color in &dark {
            let r = ratio(rel(*color), panel);
            assert!(r >= 4.0, "dark theme node {color:?} is {r:.2}:1 on the panel");
        }

    }

    #[test]
    fn colours_are_stable_and_parseable() {
        let style = Style::default();
        let color = style.color_for("crate");
        assert_eq!(color, style.color_for("crate"));
        assert!(color.iter().all(|c| (0.0..=1.0).contains(c)));
        assert!(parse_hex("nope").is_none());
        assert!(parse_hex("#12345").is_none());
    }
}
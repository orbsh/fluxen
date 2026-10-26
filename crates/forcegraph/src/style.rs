//! Renderer knobs. Mirrors `accrete::ForceGraphStyle` (the DSL side owns the
//! wire shape; this crate stays free of the DSL), so the mapping between the
//! two happens once, in the host component.

use crate::force;

/// Default node palette: muted slate → teal → amber → violet, chosen to stay
/// readable on a light page while the graph sits under dark labels.
const DEFAULT_PALETTE: [&str; 4] = ["#3f5b8b", "#2f7d78", "#a3702a", "#6b4a86"];

#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub node_size_px: f32,
    pub node_size_step_px: f32,
    pub node_tiers: u8,
    pub link_width_px: f32,
    pub link_opacity: f32,
    pub charge: f32,
    pub link_distance: f32,
    pub spring: f32,
    pub center: f32,
    pub damping: f32,
    pub labels: bool,
    pub label_px: f32,
    pub label_max_nodes: u32,
    pub auto_fit: bool,
    pub fit_padding_px: f32,
    pub background: Option<String>,
    pub palette: Vec<String>,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            node_size_px: 7.0,
            node_size_step_px: 3.0,
            node_tiers: 3,
            // 2.5 px / 0.6 alpha: at 1.5 px and 0.35 the edges were drawn but
            // invisible on a light page (measured: 540 → 2486 coloured pixels
            // when the width went to 16 px).
            link_width_px: 2.5,
            link_opacity: 0.6,
            // Calm by construction: repulsion strong enough to separate
            // clusters, springs loose enough not to fight it, damping high
            // enough that the alpha cooling actually settles instead of
            // ringing. These are the numbers after the first browser pass.
            charge: -2600.0,
            link_distance: 70.0,
            spring: 1.2,
            center: 0.03,
            damping: 2.6,
            labels: true,
            label_px: 12.0,
            label_max_nodes: 150,
            auto_fit: true,
            fit_padding_px: 40.0,
            background: None,
            palette: Vec::new(),
        }
    }
}

impl Style {
    pub fn params(&self) -> force::Params {
        force::Params {
            charge: self.charge,
            link_distance: self.link_distance,
            spring: self.spring,
            center: self.center,
            damping: self.damping,
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
        (self.node_size_px + self.node_size_step_px * step).max(1.0)
    }

    /// Colour for a node kind, cycling the palette in first-seen order.
    pub fn color_for(&self, kind: &str) -> [f32; 3] {
        let palette: Vec<&str> = if self.palette.is_empty() {
            DEFAULT_PALETTE.to_vec()
        } else {
            self.palette.iter().map(String::as_str).collect()
        };
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
        assert_eq!(style.radius_px(1.0, 9.0), 7.0);
        assert_eq!(style.radius_px(5.0, 9.0), 10.0);
        assert_eq!(style.radius_px(9.0, 9.0), 13.0);
        // A weight beyond the reference still stops at the last tier.
        assert_eq!(style.radius_px(99.0, 9.0), 13.0);
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
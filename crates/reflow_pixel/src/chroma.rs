//! Chroma key (green/blue screen) removal.
//!
//! Used by the ChromaKeyActor. Operates on RGBA rows for streaming.

use crate::color::rgb_to_hsv;

/// Chroma key configuration.
#[derive(Debug, Clone, Copy)]
pub struct ChromaKeyConfig {
    /// Target hue in degrees (120 = green, 240 = blue).
    pub target_hue: f32,
    /// Hue tolerance in degrees (pixels within target_hue ± tolerance are keyed).
    pub hue_tolerance: f32,
    /// Minimum saturation for a pixel to be considered part of the key (0.0–1.0).
    pub min_saturation: f32,
    /// Minimum value/brightness for a pixel to be considered part of the key (0.0–1.0).
    pub min_value: f32,
    /// Spill suppression strength (0.0 = none, 1.0 = full).
    pub spill_suppression: f32,
}

impl Default for ChromaKeyConfig {
    fn default() -> Self {
        Self {
            target_hue: 120.0, // green
            hue_tolerance: 30.0,
            min_saturation: 0.2,
            min_value: 0.2,
            spill_suppression: 0.5,
        }
    }
}

impl ChromaKeyConfig {
    /// Green screen preset.
    pub fn green() -> Self {
        Self::default()
    }

    /// Blue screen preset.
    pub fn blue() -> Self {
        Self {
            target_hue: 240.0,
            ..Self::default()
        }
    }
}

/// Apply chroma key to an RGBA row in-place.
///
/// Pixels matching the key color have their alpha set to 0 (transparent).
/// Edge pixels get partial alpha for smoother edges.
pub fn apply_chroma_key(row: &mut [u8], config: &ChromaKeyConfig) {
    for pixel in row.chunks_exact_mut(4) {
        let hsv = rgb_to_hsv(pixel[0], pixel[1], pixel[2]);

        // Circular hue distance
        let hue_diff = hue_distance(hsv.h, config.target_hue);

        if hue_diff < config.hue_tolerance
            && hsv.s >= config.min_saturation
            && hsv.v >= config.min_value
        {
            // How strongly this pixel matches the key (1.0 = perfect match, 0.0 = edge)
            let hue_strength = 1.0 - (hue_diff / config.hue_tolerance);
            let sat_strength =
                ((hsv.s - config.min_saturation) / (1.0 - config.min_saturation)).clamp(0.0, 1.0);
            let match_strength = hue_strength * sat_strength;

            // Reduce alpha proportionally
            let new_alpha = (pixel[3] as f32 * (1.0 - match_strength))
                .round()
                .clamp(0.0, 255.0) as u8;
            pixel[3] = new_alpha;

            // Spill suppression: desaturate toward the key color
            if config.spill_suppression > 0.0 {
                let gray = crate::color::rgba_to_gray(pixel[0], pixel[1], pixel[2]) as f32;
                let factor = 1.0 - (match_strength * config.spill_suppression);
                for c in &mut pixel[..3] {
                    let v = gray + (*c as f32 - gray) * factor;
                    *c = v.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}

/// Circular distance between two hue values (0–360).
fn hue_distance(h1: f32, h2: f32) -> f32 {
    let diff = (h1 - h2).abs();
    if diff > 180.0 {
        360.0 - diff
    } else {
        diff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pure_green_keyed() {
        let mut row = [0, 255, 0, 255]; // pure green, fully opaque
        apply_chroma_key(&mut row, &ChromaKeyConfig::green());
        assert!(
            row[3] < 30,
            "Pure green should be mostly transparent, got alpha={}",
            row[3]
        );
    }

    #[test]
    fn test_red_not_keyed() {
        let mut row = [255, 0, 0, 255];
        apply_chroma_key(&mut row, &ChromaKeyConfig::green());
        assert_eq!(row[3], 255, "Red should not be keyed");
    }

    #[test]
    fn test_blue_screen() {
        let mut row = [0, 0, 255, 255]; // pure blue
        apply_chroma_key(&mut row, &ChromaKeyConfig::blue());
        assert!(
            row[3] < 30,
            "Pure blue should be mostly transparent with blue config"
        );
    }

    #[test]
    fn test_dark_green_not_keyed() {
        // Very dark green (low value) should not be keyed
        let mut row = [0, 20, 0, 255];
        apply_chroma_key(&mut row, &ChromaKeyConfig::green());
        assert_eq!(
            row[3], 255,
            "Dark green should not be keyed (below min_value)"
        );
    }

    #[test]
    fn test_hue_distance() {
        assert!((hue_distance(10.0, 350.0) - 20.0).abs() < 1e-5);
        assert!((hue_distance(0.0, 180.0) - 180.0).abs() < 1e-5);
        assert!((hue_distance(120.0, 120.0)).abs() < 1e-5);
    }
}

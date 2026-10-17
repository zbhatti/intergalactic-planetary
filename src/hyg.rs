//! The HYG star catalog (~109k stars with distances), embedded in the binary.
//!
//! Built by `tools/build_hyg.py` from HYG v4.4 (CC BY-SA 4.0, (c) astronexus.com); see
//! `data/hyg_stars.bin.manifest.txt` for the source hash and counts.

use crate::constants::AU_PER_PC;
use crate::orbit::equatorial_to_world;
use bevy::math::DVec3;
use bevy::prelude::*;

static HYG_BIN: &[u8] = include_bytes!("../data/hyg_stars.bin");

const MAGIC: &[u8; 8] = b"IPHYG\0\0\0";
const VERSION: u32 = 2;
/// Record flag: no measured distance; placed at `tools/build_hyg.py`'s FALLBACK_PC.
const FLAG_DIST_UNKNOWN: u8 = 1;
/// Where `build_hyg.py` places naked-eye stars without a distance (FALLBACK_PC).
#[cfg(test)]
pub const FALLBACK_DIST_PC: f64 = 1000.0;
const RECORD_BYTES: usize = 28;
const HEADER_BYTES: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HygStar {
    /// World position, AU (heliocentric).
    pub pos: DVec3,
    /// Visual luminosity, solar units.
    pub lum: f32,
    /// Hipparcos number, 0 if none.
    pub hip: u32,
    pub hyg_id: u32,
    /// Spectral class letter (O B A F G K M), or ' ' if unknown.
    pub class: char,
    /// False for naked-eye stars without a measured distance, placed at [`FALLBACK_DIST_PC`].
    pub dist_known: bool,
    /// Measured radius (solar radii), when another catalog provides one (exoplanet hosts);
    /// otherwise radii are estimated from luminosity and class.
    pub radius_sun: Option<f32>,
}

#[derive(Resource, Debug, Default)]
pub struct HygCatalog {
    pub stars: Vec<HygStar>,
    /// (star index, proper name). Read by tests now; named-star entities (search/go-to) next.
    #[allow(dead_code)]
    pub names: Vec<(usize, String)>,
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    BadHeader,
    Truncated,
    BadName,
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, ParseError> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
        .ok_or(ParseError::Truncated)
}

fn f32_at(b: &[u8], at: usize) -> Result<f32, ParseError> {
    u32_at(b, at).map(f32::from_bits)
}

impl HygCatalog {
    pub fn parse(b: &[u8]) -> Result<Self, ParseError> {
        if b.get(..8) != Some(MAGIC) || u32_at(b, 8)? != VERSION {
            return Err(ParseError::BadHeader);
        }
        let count = u32_at(b, 12)? as usize;
        let name_count = u32_at(b, 16)? as usize;
        let mut stars = Vec::with_capacity(count);
        for i in 0..count {
            let at = HEADER_BYTES + i * RECORD_BYTES;
            let eq_pc = DVec3::new(
                f32_at(b, at)? as f64,
                f32_at(b, at + 4)? as f64,
                f32_at(b, at + 8)? as f64,
            );
            let class = *b.get(at + 24).ok_or(ParseError::Truncated)?;
            let flags = *b.get(at + 25).ok_or(ParseError::Truncated)?;
            stars.push(HygStar {
                pos: equatorial_to_world(eq_pc) * AU_PER_PC,
                lum: f32_at(b, at + 12)?,
                hip: u32_at(b, at + 16)?,
                hyg_id: u32_at(b, at + 20)?,
                class: if class == 0 { ' ' } else { class as char },
                dist_known: flags & FLAG_DIST_UNKNOWN == 0,
                radius_sun: None,
            });
        }
        let mut at = HEADER_BYTES + count * RECORD_BYTES;
        let mut names = Vec::with_capacity(name_count);
        for _ in 0..name_count {
            let index = u32_at(b, at)? as usize;
            let len = b
                .get(at + 4..at + 6)
                .map(|s| u16::from_le_bytes(s.try_into().unwrap()) as usize)
                .ok_or(ParseError::Truncated)?;
            let bytes = b.get(at + 6..at + 6 + len).ok_or(ParseError::Truncated)?;
            let name = std::str::from_utf8(bytes).map_err(|_| ParseError::BadName)?;
            if index >= count {
                return Err(ParseError::BadName);
            }
            names.push((index, name.to_string()));
            at += 6 + len;
        }
        Ok(Self { stars, names })
    }

    pub fn embedded() -> Self {
        Self::parse(HYG_BIN).expect("embedded HYG catalog is valid (checked by tests)")
    }

    #[allow(dead_code)]
    pub fn index_of_name(&self, name: &str) -> Option<usize> {
        self.names.iter().find(|(_, n)| n == name).map(|(i, _)| *i)
    }

    /// Apparent visual magnitude from `observer` (AU), from luminosity and distance.
    pub fn apparent_magnitude(&self, i: usize, observer: DVec3) -> f64 {
        let s = &self.stars[i];
        let d_pc = (s.pos.distance(observer) / AU_PER_PC).max(1e-6);
        4.85 - 2.5 * (s.lum as f64).log10() + 5.0 * d_pc.log10() - 5.0
    }

    /// Indices of stars brighter than `max_mag` as seen from the Sun.
    pub fn bright_indices(&self, max_mag: f64) -> Vec<usize> {
        (0..self.stars.len())
            .filter(|&i| self.apparent_magnitude(i, DVec3::ZERO) <= max_mag)
            .collect()
    }

    /// The candidate closest in direction to `dir` (unit, world) as seen from the Sun, if
    /// within `max_sep_deg`.
    pub fn match_direction(
        &self,
        candidates: &[usize],
        dir: DVec3,
        max_sep_deg: f64,
    ) -> Option<usize> {
        let min_cos = max_sep_deg.to_radians().cos();
        candidates
            .iter()
            .map(|&i| (i, self.stars[i].pos.normalize().dot(dir)))
            .filter(|&(_, c)| c >= min_cos)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stars::star_world_pos;

    #[test]
    fn test_embedded_catalog_parses() {
        let cat = HygCatalog::embedded();
        assert_eq!(
            cat.stars.len(),
            109_819,
            "matches data/hyg_stars.bin.manifest.txt"
        );
        assert_eq!(cat.names.len(), 544);
        let unknown = cat.stars.iter().filter(|s| !s.dist_known).count();
        assert_eq!(unknown, 430);
    }

    #[test]
    fn test_rejects_bad_header_and_truncation() {
        assert_eq!(
            HygCatalog::parse(b"nope").unwrap_err(),
            ParseError::BadHeader
        );
        let truncated = &HYG_BIN[..HEADER_BYTES + RECORD_BYTES * 3 - 1];
        assert_eq!(
            HygCatalog::parse(truncated).unwrap_err(),
            ParseError::Truncated
        );
    }

    #[test]
    fn test_sirius_lands_where_the_curated_catalog_puts_it() {
        // Guards the frame conversion: HYG is equatorial (+X RA 0h, +Y RA 6h, +Z north) and
        // must go through equatorial_to_world, or the sky is mirrored again.
        let cat = HygCatalog::embedded();
        let i = cat.index_of_name("Sirius").expect("Sirius in HYG");
        let hyg = cat.stars[i].pos;
        let curated = star_world_pos(6.7525, -16.7161, 2.637);
        let sep_deg = hyg.angle_between(curated).to_degrees();
        assert!(sep_deg < 0.1, "direction off by {sep_deg} deg");
        assert!((hyg.length() / curated.length() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_naked_eye_stars_without_distance_are_kept_in_the_sky() {
        // Polis (HIP 89341, mag 3.84) has no Hipparcos distance; it's placed at the fallback
        // distance with its real direction and apparent magnitude.
        let cat = HygCatalog::embedded();
        let i = cat.index_of_name("Polis").expect("Polis kept");
        let s = &cat.stars[i];
        assert!(!s.dist_known);
        assert!((s.pos.length() / AU_PER_PC - FALLBACK_DIST_PC).abs() < 1e-3);
        assert!((cat.apparent_magnitude(i, DVec3::ZERO) - 3.84).abs() < 0.05);
        assert!(cat.stars[cat.index_of_name("Sirius").unwrap()].dist_known);
    }

    #[test]
    fn test_apparent_magnitudes_from_earth() {
        let cat = HygCatalog::embedded();
        for (name, mag) in [("Sirius", -1.44), ("Vega", 0.03), ("Polaris", 1.97)] {
            let i = cat.index_of_name(name).unwrap();
            let m = cat.apparent_magnitude(i, DVec3::ZERO);
            assert!((m - mag).abs() < 0.1, "{name}: {m}");
        }
    }
}

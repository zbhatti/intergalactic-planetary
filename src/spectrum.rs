//! Visible spectra -> display colour.
//!
//! A spectrum is turned into a colour the way the eye does it: weighted by the CIE 1931 2-degree
//! colour-matching functions to get XYZ, then converted to linear sRGB. The matching functions use
//! the multi-lobe Gaussian fit of Wyman, Sloan & Shirley (2013, JCGT 2(2)), accurate to about 1%
//! of the tabulated CIE data, so no table needs to be embedded.
//!
//! Colours are normalised to unit luminance (Y = 1): brightness is applied separately. Spectra
//! outside the sRGB gamut (an Halpha line is redder than sRGB red) are clipped to the gamut.

/// One emission line: wavelength (nm) and intensity relative to the others.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub name: &'static str,
    pub nm: f64,
    pub rel: f64,
}

fn lobe(l: f64, mu: f64, s1: f64, s2: f64) -> f64 {
    let s = if l < mu { s1 } else { s2 };
    let t = (l - mu) / s;
    (-0.5 * t * t).exp()
}

/// CIE 1931 2-degree colour-matching functions at wavelength `nm`.
pub fn cie_xyz(nm: f64) -> [f64; 3] {
    let x = 1.056 * lobe(nm, 599.8, 37.9, 31.0) + 0.362 * lobe(nm, 442.0, 16.0, 26.7)
        - 0.065 * lobe(nm, 501.1, 20.4, 26.2);
    let y = 0.821 * lobe(nm, 568.8, 46.9, 40.5) + 0.286 * lobe(nm, 530.9, 16.3, 31.1);
    let z = 1.217 * lobe(nm, 437.0, 11.8, 36.0) + 0.681 * lobe(nm, 459.0, 26.0, 13.8);
    [x, y, z]
}

/// XYZ -> linear sRGB (D65).
pub fn xyz_to_linear_srgb([x, y, z]: [f64; 3]) -> [f64; 3] {
    [
        3.2406 * x - 1.5372 * y - 0.4986 * z,
        -0.9689 * x + 1.8758 * y + 0.0415 * z,
        0.0557 * x - 0.2040 * y + 1.0570 * z,
    ]
}

/// Clips negative (out-of-gamut) components and rescales to unit luminance.
fn to_unit_luminance(rgb: [f64; 3]) -> [f32; 3] {
    let c = rgb.map(|v| v.max(0.0));
    let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    if y <= 0.0 {
        return [0.0; 3];
    }
    c.map(|v| (v / y) as f32)
}

/// Colour of an emission-line spectrum, unit luminance.
pub fn lines_rgb(lines: &[Line]) -> [f32; 3] {
    let mut xyz = [0.0; 3];
    for l in lines {
        let c = cie_xyz(l.nm);
        for k in 0..3 {
            xyz[k] += l.rel * c[k];
        }
    }
    to_unit_luminance(xyz_to_linear_srgb(xyz))
}

/// Planck spectral radiance (arbitrary units) at `nm` for temperature `t` (K).
pub fn planck(nm: f64, t: f64) -> f64 {
    const C2: f64 = 1.438_776_877e7; // hc/k in nm K
    let l = nm;
    1.0 / (l.powi(5) * ((C2 / (l * t)).exp() - 1.0))
}

/// Colour of starlight (a blackbody at `teff`) after scattering by dust with efficiency
/// proportional to wavelength^-`scatter_exp`, unit luminance. `scatter_exp` 0 is the star itself.
pub fn scattered_starlight_rgb(teff: f64, scatter_exp: f64) -> [f32; 3] {
    let mut xyz = [0.0; 3];
    let mut nm = 380.0;
    while nm <= 780.0 {
        let s = planck(nm, teff) * (550.0 / nm).powf(scatter_exp);
        let c = cie_xyz(nm);
        for k in 0..3 {
            xyz[k] += s * c[k];
        }
        nm += 2.0;
    }
    to_unit_luminance(xyz_to_linear_srgb(xyz))
}

/// Visible emission lines of a typical Galactic H II region, relative to H-beta = 1.
/// Hydrogen: Case B recombination at T = 10^4 K, n = 10^2 cm^-3 (Storey & Hummer 1995), intrinsic,
/// i.e. before any reddening by dust on the way to us. Forbidden lines: representative values for
/// a low-to-moderate excitation nebula (Osterbrock & Ferland 2006); individual regions vary by
/// several times, and IP doesn't have per-region measurements yet. [O III] matters most for the
/// colour: the eye is ~4x more sensitive at 501 nm than at 656 nm, so at [O III] 5007/H-beta = 0.5
/// a region looks pink-magenta (Balmer red + blue), and above ~1.5 it looks teal, which is why
/// Orion's bright core looks greenish in a telescope while red-sensitive cameras record pink.
pub const HII_LINES: [Line; 11] = [
    Line {
        name: "H-alpha",
        nm: 656.28,
        rel: 2.86,
    },
    Line {
        name: "H-beta",
        nm: 486.13,
        rel: 1.0,
    },
    Line {
        name: "H-gamma",
        nm: 434.05,
        rel: 0.468,
    },
    Line {
        name: "H-delta",
        nm: 410.17,
        rel: 0.259,
    },
    Line {
        name: "[O III] 5007",
        nm: 500.68,
        rel: 0.5,
    },
    Line {
        name: "[O III] 4959",
        nm: 495.89,
        rel: 0.168,
    },
    Line {
        name: "[N II] 6584",
        nm: 658.35,
        rel: 0.85,
    },
    Line {
        name: "[N II] 6548",
        nm: 654.80,
        rel: 0.28,
    },
    Line {
        name: "[S II] 6716",
        nm: 671.64,
        rel: 0.25,
    },
    Line {
        name: "[S II] 6731",
        nm: 673.08,
        rel: 0.2,
    },
    Line {
        name: "He I 5876",
        nm: 587.56,
        rel: 0.11,
    },
];

/// Dust scattering efficiency exponent in the visible (Q_sca ~ lambda^-1 for interstellar grains,
/// much shallower than Rayleigh's lambda^-4): reflection nebulae are bluer than their stars.
pub const DUST_SCATTER_EXP: f64 = 1.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cie_fit_peaks() {
        // y-bar peaks at ~555 nm with value ~1.0; x-bar's main lobe ~600 nm.
        let y555 = cie_xyz(555.0)[1];
        assert!((y555 - 1.0).abs() < 0.03, "{y555}");
        assert!(cie_xyz(600.0)[0] > cie_xyz(500.0)[0]);
        assert!(cie_xyz(445.0)[2] > 1.5);
        // Nothing much outside the visible band.
        assert!(cie_xyz(800.0).iter().all(|&v| v < 0.01));
    }

    #[test]
    fn test_line_colours() {
        let red = lines_rgb(&[Line {
            name: "a",
            nm: 656.28,
            rel: 1.0,
        }]);
        assert!(red[0] > 3.0 * red[1] && red[2] < 0.05, "{red:?}");
        let teal = lines_rgb(&[Line {
            name: "o",
            nm: 500.68,
            rel: 1.0,
        }]);
        assert!(teal[1] > teal[0] && teal[2] > teal[0], "{teal:?}");
        // A low-excitation H II region is pink-magenta: Balmer red and blue, little green.
        let hii = lines_rgb(&HII_LINES);
        assert!(hii[0] > hii[1] && hii[2] > hii[1], "{hii:?}");
        // Strong [O III] turns it teal (as Orion's core looks in a telescope).
        let mut high = HII_LINES;
        high[4].rel = 3.0;
        high[5].rel = 1.0;
        let teal = lines_rgb(&high);
        assert!(teal[1] > teal[0], "{teal:?}");
    }

    #[test]
    fn test_unit_luminance() {
        let c = lines_rgb(&HII_LINES);
        let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        assert!((y - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_blackbody_and_reflection_colours() {
        // The Sun is close to white in sRGB (D65 is ~6500 K).
        let sun = scattered_starlight_rgb(5772.0, 0.0);
        assert!(sun.iter().all(|&v| (v - 1.0).abs() < 0.25), "{sun:?}");
        // A B star is blue-white, an M star orange-red.
        let b = scattered_starlight_rgb(15000.0, 0.0);
        assert!(b[2] > b[0], "{b:?}");
        let m = scattered_starlight_rgb(3200.0, 0.0);
        assert!(m[0] > m[2] * 2.0, "{m:?}");
        // Scattering makes a reflection nebula bluer than the star lighting it.
        let neb = scattered_starlight_rgb(15000.0, DUST_SCATTER_EXP);
        assert!(neb[2] / neb[0] > b[2] / b[0]);
    }
}

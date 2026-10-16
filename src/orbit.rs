//! Reference frames and Keplerian orbits.
//!
//! World frame (see coords.rs) is equatorial J2000/ICRF, rotated so +Y is the north celestial
//! pole: `world = (x_eq, z_eq, -y_eq)`. That's a proper rotation (about X by -90 degrees), so the sky
//! has its real handedness. An earlier version mapped `(x, z, y)`, which mirrored the sky.
//!
//! Orbits are evaluated once at [`SCENE_JD`] for now (static scene); Phase 5 will advance time.

use bevy::math::DVec3;

pub const J2000_JD: f64 = 2_451_545.0;
/// The date the scene shows: 2026-06-25 00:00 TT.
pub const SCENE_JD: f64 = 2_461_216.5;
/// Mean obliquity of the ecliptic at J2000.
pub const OBLIQUITY_DEG: f64 = 23.439_291;

/// Equatorial (ICRF) cartesian -> world.
pub fn equatorial_to_world(v: DVec3) -> DVec3 {
    DVec3::new(v.x, v.z, -v.y)
}

/// Ecliptic J2000 cartesian -> equatorial (rotation about X by the obliquity).
pub fn ecliptic_to_equatorial(v: DVec3) -> DVec3 {
    let (s, c) = OBLIQUITY_DEG.to_radians().sin_cos();
    DVec3::new(v.x, v.y * c - v.z * s, v.y * s + v.z * c)
}

pub fn ecliptic_to_world(v: DVec3) -> DVec3 {
    equatorial_to_world(ecliptic_to_equatorial(v))
}

/// Unit vector toward RA/Dec (degrees), equatorial frame.
pub fn radec_to_equatorial(ra_deg: f64, dec_deg: f64) -> DVec3 {
    let (ra, dec) = (ra_deg.to_radians(), dec_deg.to_radians());
    DVec3::new(dec.cos() * ra.cos(), dec.cos() * ra.sin(), dec.sin())
}

/// The plane an orbit's i/node/peri are measured in.
#[derive(Clone, Copy, Debug)]
pub enum RefPlane {
    /// Ecliptic J2000 (heliocentric orbits, Earth's Moon).
    Ecliptic,
    /// Plane with this north pole (RA/Dec, degrees, ICRF): a planet's equator or Laplace
    /// plane. Node is measured from the plane's ascending node on the ICRF equator.
    Pole { ra_deg: f64, dec_deg: f64 },
}

#[derive(Clone, Copy, Debug)]
pub struct Elements {
    pub a_au: f64,
    pub e: f64,
    pub i_deg: f64,
    pub node_deg: f64,
    /// Argument of periapsis.
    pub peri_deg: f64,
    /// Mean anomaly at `epoch_jd`.
    pub m0_deg: f64,
    pub epoch_jd: f64,
    pub period_days: f64,
    pub plane: RefPlane,
}

/// Eccentric anomaly from mean anomaly (radians), Newton's method.
pub fn solve_kepler(m: f64, e: f64) -> f64 {
    let mut ecc = if e < 0.8 { m } else { std::f64::consts::PI };
    for _ in 0..30 {
        let d = (ecc - e * ecc.sin() - m) / (1.0 - e * ecc.cos());
        ecc -= d;
        if d.abs() < 1e-14 {
            break;
        }
    }
    ecc
}

impl Elements {
    /// Position relative to the orbited body, in the reference plane's own coordinates.
    pub fn position_in_plane(&self, jd: f64) -> DVec3 {
        let n = 360.0 / self.period_days;
        let m = (self.m0_deg + n * (jd - self.epoch_jd))
            .rem_euclid(360.0)
            .to_radians();
        let ecc = solve_kepler(m, self.e);
        // Perifocal coordinates.
        let px = self.a_au * (ecc.cos() - self.e);
        let py = self.a_au * (1.0 - self.e * self.e).sqrt() * ecc.sin();
        let (so, co) = self.node_deg.to_radians().sin_cos();
        let (si, ci) = self.i_deg.to_radians().sin_cos();
        let (sw, cw) = self.peri_deg.to_radians().sin_cos();
        // Rz(node) * Rx(i) * Rz(peri) applied to (px, py, 0).
        let x1 = px * cw - py * sw;
        let y1 = px * sw + py * cw;
        DVec3::new(x1 * co - y1 * ci * so, x1 * so + y1 * ci * co, y1 * si)
    }

    /// Position relative to the orbited body, world frame.
    pub fn position_world(&self, jd: f64) -> DVec3 {
        let v = self.position_in_plane(jd);
        match self.plane {
            RefPlane::Ecliptic => ecliptic_to_world(v),
            RefPlane::Pole { ra_deg, dec_deg } => {
                let pole = radec_to_equatorial(ra_deg, dec_deg);
                let x_axis = DVec3::Z.cross(pole).normalize();
                let y_axis = pole.cross(x_axis);
                equatorial_to_world(x_axis * v.x + y_axis * v.y + pole * v.z)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_frame_is_a_rotation_not_a_mirror() {
        let (x, y, z) = (
            equatorial_to_world(DVec3::X),
            equatorial_to_world(DVec3::Y),
            equatorial_to_world(DVec3::Z),
        );
        // Right-handed in, right-handed out.
        assert!((x.cross(y) - z).length() < 1e-12);
        // North celestial pole is world up.
        assert!((z - DVec3::Y).length() < 1e-12);
    }

    #[test]
    fn test_ecliptic_pole_is_tilted_by_obliquity() {
        let ecl_pole = ecliptic_to_world(DVec3::Z);
        let angle = ecl_pole.angle_between(DVec3::Y).to_degrees();
        assert!((angle - OBLIQUITY_DEG).abs() < 1e-9);
    }

    #[test]
    fn test_kepler_solver() {
        for &e in &[0.0, 0.1, 0.5, 0.9, 0.99] {
            for k in 0..12 {
                let m = k as f64 * 0.5;
                let ecc = solve_kepler(m, e);
                assert!((ecc - e * ecc.sin() - m).abs() < 1e-12, "e={e} m={m}");
            }
        }
    }

    #[test]
    fn test_circular_orbit_radius_and_period() {
        let el = Elements {
            a_au: 2.0,
            e: 0.0,
            i_deg: 0.0,
            node_deg: 0.0,
            peri_deg: 0.0,
            m0_deg: 0.0,
            epoch_jd: J2000_JD,
            period_days: 100.0,
            plane: RefPlane::Ecliptic,
        };
        let p0 = el.position_world(J2000_JD);
        assert!((p0.length() - 2.0).abs() < 1e-12);
        assert!((el.position_world(J2000_JD + 100.0) - p0).length() < 1e-9);
        let quarter = el.position_world(J2000_JD + 25.0);
        assert!(p0.dot(quarter).abs() < 1e-9);
    }

    #[test]
    fn test_pole_plane_orbit_is_perpendicular_to_pole() {
        // Uranus's moons orbit in its equator: perpendicular to Uranus's pole.
        let (ra, dec) = (257.311, -15.175);
        let el = Elements {
            a_au: 1.0,
            e: 0.0,
            i_deg: 0.0,
            node_deg: 0.0,
            peri_deg: 0.0,
            m0_deg: 0.0,
            epoch_jd: J2000_JD,
            period_days: 10.0,
            plane: RefPlane::Pole {
                ra_deg: ra,
                dec_deg: dec,
            },
        };
        let pole = equatorial_to_world(radec_to_equatorial(ra, dec));
        for k in 0..8 {
            let p = el.position_world(J2000_JD + k as f64 * 1.25);
            assert!(p.dot(pole).abs() < 1e-12);
        }
    }
}

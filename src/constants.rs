//! Physical constants. World units are AU (f64); see `coords.rs`.

pub const KM_PER_AU: f64 = 149_597_870.7;
pub const C_AU_PER_S: f64 = 299_792.458 / KM_PER_AU;
pub const AU_PER_LY: f64 = 63_241.077;
/// 1 parsec = 648000/pi AU (IAU 2015 definition).
pub const AU_PER_PC: f64 = 206_264.806_247;

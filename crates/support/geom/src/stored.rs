//! What a PowerPoint file stores of a value (task card C8-slide-5), one function per kind of
//! quantity: the unit the file counts in (`*_units`) and the value read back from it.
//!
//! The rule: a value enters the model as the file stores it. A command that computes a length, an
//! angle or a fraction (a box grown to fit its text, a size scaled, a shrink-to-fit scale) gives
//! the model the value these functions answer, so the document held in memory is the document
//! the saved file opens as. A difference between the two reads as a change by someone else when
//! the file is read again (the agents' premise guard compares them), so a value the file cannot
//! hold never enters the model. The PowerPoint writer and reader count in the same units through
//! these functions.

use crate::{emu_to_pt, pt_to_emu};

/// The largest coordinate a file holds, in EMU (`ST_Coordinate`).
const MAX_EMU: i64 = 27_273_042_316_900;

/// A length (points) in EMU, as a file stores it: rounded, clamped to `ST_Coordinate`.
pub fn length_units(pt: f64) -> i64 {
    pt_to_emu(pt).clamp(-MAX_EMU, MAX_EMU)
}

/// A length (points) as a file stores it: a whole number of EMU (1/12,700 pt).
pub fn length(pt: f64) -> f64 {
    emu_to_pt(length_units(pt))
}

/// A length that is never negative (`ST_PositiveCoordinate`: a size, a column width).
pub fn extent(pt: f64) -> f64 {
    emu_to_pt(length_units(pt).max(0))
}

/// An angle (degrees) in 1/60,000 degree, as a file stores a rotation: within 0..360.
pub fn angle_units(deg: f64) -> i64 {
    if !deg.is_finite() {
        return 0;
    }
    ((deg.rem_euclid(360.0) * 60_000.0).round() as i64).clamp(0, 21_599_999)
}

/// An angle (degrees) as a file stores a rotation: a whole number of 1/60,000 degree in 0..360.
pub fn angle(deg: f64) -> f64 {
    angle_units(deg) as f64 / 60_000.0
}

/// A fraction in 1/100,000 (a percentage in thousandths), as a file stores a scale.
pub fn fraction_units(v: f64) -> i64 {
    if v.is_finite() { (v * 100_000.0).round().clamp(-2.0e9, 2.0e9) as i64 } else { 0 }
}

/// A fraction as a file stores it: a whole number of 1/100,000.
pub fn fraction(v: f64) -> f64 {
    fraction_units(v) as f64 / 100_000.0
}

/// Whether `pt` already is a length a file holds.
pub fn is_length(pt: f64) -> bool {
    length(pt) == pt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EMU_PER_PT;

    #[test]
    fn each_kind_is_what_the_file_reads_back() {
        // A box a text grew to fit, a resize, a rotation, a shrink scale.
        assert_eq!(length(93.599_999_999_999_98), 93.6);
        assert_eq!(length(71.999_999_999_999_99), 72.0);
        assert_eq!(length(61.111), 776_110.0 / EMU_PER_PT);
        assert_eq!(extent(-3.0), 0.0);
        assert_eq!(angle(12.345_000_000_000_027), 12.345);
        assert_eq!(angle(-10.0), 350.0);
        assert_eq!(fraction(0.967_919_921_875_000_1), 0.96792);
        assert_eq!(fraction(0.025_664_062_499_999_93), 0.02566);
        // Stored values are fixed points.
        for v in [0.0, 1.0, 93.6, 72.0, 776_110.0 / EMU_PER_PT] {
            assert!(is_length(v));
            assert_eq!(length(length(v)), length(v));
        }
        assert_eq!(angle(angle(359.999_999_9)), angle(359.999_999_9));
        assert_eq!(length(f64::NAN), 0.0);
    }
}

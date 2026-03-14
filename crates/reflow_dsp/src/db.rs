//! Decibel ↔ linear conversion utilities.

/// Minimum dB value returned by [`linear_to_db`] for zero/negative input.
pub const MIN_DB: f64 = -150.0;

/// Convert linear amplitude to decibels.
///
/// Returns [`MIN_DB`] for zero or negative values.
#[inline]
pub fn linear_to_db(linear: f64) -> f64 {
    if linear <= 0.0 {
        MIN_DB
    } else {
        20.0 * linear.log10()
    }
}

/// Convert decibels to linear amplitude.
#[inline]
pub fn db_to_linear(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

/// Convert linear power to decibels (for RMS/power quantities).
#[inline]
pub fn power_to_db(power: f64) -> f64 {
    if power <= 0.0 {
        MIN_DB
    } else {
        10.0 * power.log10()
    }
}

/// Convert decibels to linear power.
#[inline]
pub fn db_to_power(db: f64) -> f64 {
    10.0_f64.powf(db / 10.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unity_is_zero_db() {
        assert!((linear_to_db(1.0)).abs() < 1e-10);
        assert!((db_to_linear(0.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_double_is_6db() {
        let db = linear_to_db(2.0);
        assert!((db - 6.0206).abs() < 0.001);
    }

    #[test]
    fn test_half_is_minus_6db() {
        let db = linear_to_db(0.5);
        assert!((db - (-6.0206)).abs() < 0.001);
    }

    #[test]
    fn test_roundtrip() {
        for &db in &[-60.0, -20.0, -6.0, 0.0, 6.0, 12.0, 24.0] {
            let lin = db_to_linear(db);
            let back = linear_to_db(lin);
            assert!((back - db).abs() < 1e-10, "Roundtrip failed for {}dB", db);
        }
    }

    #[test]
    fn test_zero_is_min_db() {
        assert_eq!(linear_to_db(0.0), MIN_DB);
        assert_eq!(linear_to_db(-1.0), MIN_DB);
    }

    #[test]
    fn test_power_roundtrip() {
        for &db in &[-40.0, -10.0, 0.0, 3.0, 10.0] {
            let power = db_to_power(db);
            let back = power_to_db(power);
            assert!((back - db).abs() < 1e-10);
        }
    }
}

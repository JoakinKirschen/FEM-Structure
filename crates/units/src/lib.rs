//! SI-backed quantity types for the structural core.
//!
//! Values are stored and serialized in coherent SI units. Conversions belong at
//! import, UI, CLI, and reporting boundaries; the solver operates on typed SI
//! quantities without display rounding.
//!
//! The following intentionally does not compile:
//!
//! ```compile_fail
//! use structural_units::{Force, Length};
//! let force = Force::from_newtons(10.0);
//! let length = Length::from_metres(2.0);
//! let _invalid = force + length;
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantityError {
    NonFinite,
    InvalidDecimalPlaces,
}

impl fmt::Display for QuantityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite => write!(f, "quantity must be finite"),
            Self::InvalidDecimalPlaces => write!(f, "decimal places must be in 0..=15"),
        }
    }
}

impl std::error::Error for QuantityError {}

macro_rules! quantity {
    ($name:ident, $si_ctor:ident, $si_getter:ident, $symbol:literal) => {
        #[derive(
            Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, PartialOrd,
        )]
        #[serde(transparent)]
        pub struct $name(f64);

        impl $name {
            pub const ZERO: Self = Self(0.0);

            /// Creates an SI quantity without validation. Intended for constants
            /// and trusted internal arithmetic.
            pub const fn $si_ctor(value: f64) -> Self {
                Self(value)
            }

            /// Validated constructor for untrusted input boundaries.
            pub fn try_from_si(value: f64) -> Result<Self, QuantityError> {
                if value.is_finite() {
                    Ok(Self(value))
                } else {
                    Err(QuantityError::NonFinite)
                }
            }

            pub const fn $si_getter(self) -> f64 {
                self.0
            }

            pub fn is_finite(self) -> bool {
                self.0.is_finite()
            }

            pub fn abs(self) -> Self {
                Self(self.0.abs())
            }
        }

        impl Add for $name {
            type Output = Self;
            fn add(self, rhs: Self) -> Self::Output {
                Self(self.0 + rhs.0)
            }
        }

        impl AddAssign for $name {
            fn add_assign(&mut self, rhs: Self) {
                self.0 += rhs.0;
            }
        }

        impl Sub for $name {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self::Output {
                Self(self.0 - rhs.0)
            }
        }

        impl SubAssign for $name {
            fn sub_assign(&mut self, rhs: Self) {
                self.0 -= rhs.0;
            }
        }

        impl Neg for $name {
            type Output = Self;
            fn neg(self) -> Self::Output {
                Self(-self.0)
            }
        }

        impl Mul<f64> for $name {
            type Output = Self;
            fn mul(self, rhs: f64) -> Self::Output {
                Self(self.0 * rhs)
            }
        }

        impl Div<f64> for $name {
            type Output = Self;
            fn div(self, rhs: f64) -> Self::Output {
                Self(self.0 / rhs)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{} {}", self.0, $symbol)
            }
        }
    };
}

quantity!(Length, from_metres, metres, "m");
quantity!(Force, from_newtons, newtons, "N");
quantity!(Stiffness, from_newtons_per_metre, newtons_per_metre, "N/m");
quantity!(Stress, from_pascals, pascals, "Pa");
quantity!(Area, from_square_metres, square_metres, "m²");
quantity!(
    SecondMomentOfArea,
    from_metres_to_fourth,
    metres_to_fourth,
    "m⁴"
);
quantity!(MassDensity, from_kilograms_per_cubic_metre, kilograms_per_cubic_metre, "kg/m³");
quantity!(Moment, from_newton_metres, newton_metres, "N·m");

impl Length {
    pub fn from_millimetres(value: f64) -> Self {
        Self(value * 1.0e-3)
    }

    pub fn millimetres(self) -> f64 {
        self.0 * 1.0e3
    }
}

impl Force {
    pub fn from_kilonewtons(value: f64) -> Self {
        Self(value * 1.0e3)
    }

    pub fn kilonewtons(self) -> f64 {
        self.0 * 1.0e-3
    }
}

impl Stiffness {
    pub fn from_kilonewtons_per_metre(value: f64) -> Self {
        Self(value * 1.0e3)
    }

    pub fn kilonewtons_per_metre(self) -> f64 {
        self.0 * 1.0e-3
    }
}

impl Area {
    pub fn from_square_millimetres(value: f64) -> Self {
        Self(value * 1.0e-6)
    }

    pub fn square_millimetres(self) -> f64 {
        self.0 * 1.0e6
    }
}

impl SecondMomentOfArea {
    pub fn from_millimetres_to_fourth(value: f64) -> Self {
        Self(value * 1.0e-12)
    }

    pub fn millimetres_to_fourth(self) -> f64 {
        self.0 * 1.0e12
    }
}

impl Moment {
    pub fn from_kilonewton_metres(value: f64) -> Self {
        Self(value * 1.0e3)
    }

    pub fn kilonewton_metres(self) -> f64 {
        self.0 * 1.0e-3
    }
}

impl Stress {
    pub fn from_gigapascals(value: f64) -> Self {
        Self(value * 1.0e9)
    }

    pub fn gigapascals(self) -> f64 {
        self.0 * 1.0e-9
    }

    pub fn from_megapascals(value: f64) -> Self {
        Self(value * 1.0e6)
    }

    pub fn megapascals(self) -> f64 {
        self.0 * 1.0e-6
    }
}

/// Dimensionally valid Hooke relation: displacement = force / stiffness.
impl Div<Stiffness> for Force {
    type Output = Length;

    fn div(self, rhs: Stiffness) -> Self::Output {
        Length::from_metres(self.0 / rhs.0)
    }
}

/// Dimensionally valid Hooke relation: force = stiffness * displacement.
impl Mul<Length> for Stiffness {
    type Output = Force;

    fn mul(self, rhs: Length) -> Self::Output {
        Force::from_newtons(self.0 * rhs.0)
    }
}

impl Mul<Stiffness> for Length {
    type Output = Force;

    fn mul(self, rhs: Stiffness) -> Self::Output {
        rhs * self
    }
}

/// Explicit presentation/export rounding. Solver state must remain unrounded.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoundingMode {
    NearestHalfAwayFromZero,
    TowardZero,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoundingPolicy {
    pub decimal_places: u8,
    pub mode: RoundingMode,
}

impl RoundingPolicy {
    pub fn new(decimal_places: u8, mode: RoundingMode) -> Result<Self, QuantityError> {
        if decimal_places <= 15 {
            Ok(Self {
                decimal_places,
                mode,
            })
        } else {
            Err(QuantityError::InvalidDecimalPlaces)
        }
    }

    pub fn apply(self, value: f64) -> f64 {
        let factor = 10_f64.powi(i32::from(self.decimal_places));
        let scaled = value * factor;
        let rounded = match self.mode {
            RoundingMode::NearestHalfAwayFromZero => scaled.round(),
            RoundingMode::TowardZero => scaled.trunc(),
            RoundingMode::Up => scaled.ceil(),
            RoundingMode::Down => scaled.floor(),
        };
        rounded / factor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_boundary_units_to_si() {
        assert_eq!(Length::from_millimetres(250.0).metres(), 0.25);
        assert_eq!(Force::from_kilonewtons(125.0).newtons(), 125_000.0);
        assert_eq!(Stress::from_megapascals(355.0).pascals(), 355_000_000.0);
        assert_eq!(Area::from_square_millimetres(2_000.0).square_metres(), 0.002);
        assert_eq!(
            SecondMomentOfArea::from_millimetres_to_fourth(8.0e6).metres_to_fourth(),
            8.0e-6
        );
        assert_eq!(Moment::from_kilonewton_metres(12.0).newton_metres(), 12_000.0);
    }

    #[test]
    fn hooke_relation_returns_a_length() {
        let displacement =
            Force::from_kilonewtons(100.0) / Stiffness::from_newtons_per_metre(20_000_000.0);
        assert!((displacement.millimetres() - 5.0).abs() < 1.0e-12);
    }

    #[test]
    fn rounding_is_explicit_and_not_part_of_quantity_arithmetic() {
        let policy =
            RoundingPolicy::new(2, RoundingMode::NearestHalfAwayFromZero).unwrap();
        assert_eq!(policy.apply(12.345), 12.35);
        assert_eq!(policy.apply(-12.345), -12.35);
    }

    #[test]
    fn rejects_non_finite_boundary_values() {
        assert_eq!(
            Force::try_from_si(f64::NAN),
            Err(QuantityError::NonFinite)
        );
    }

    #[test]
    fn serde_representation_remains_an_si_number() {
        let json = serde_json::to_string(&Force::from_kilonewtons(2.5)).unwrap();
        assert_eq!(json, "2500.0");
    }
}

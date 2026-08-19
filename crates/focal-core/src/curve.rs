//! Production tone-curve evaluation.
//!
//! Curve inputs and outputs are normalised canonical encoded Adobe RGB
//! (1998) values. This module deliberately contains only the production
//! evaluator: Smooth interpolation and the Linked RGB, Luma, and Per-channel
//! RGB application modes. GUI-oriented handles and experimental interpolation
//! modes remain in `FocalCurve`.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

/// Adobe RGB (1998) coefficients for the encoded-domain Luma mode.
pub const ADOBE_RGB_LUMA_COEFFICIENTS: [f32; 3] = [0.297_355, 0.627_372, 0.075_273];

/// The production curve application mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CurveMode {
    /// Apply one curve independently to red, green, and blue.
    #[default]
    LinkedRgb,
    /// Adjust Adobe RGB encoded Luma while preserving channel proportions.
    Luma,
    /// Apply an independent curve to each channel.
    PerChannelRgb,
}

/// A channel selector for accessing a per-channel curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CurveChannel {
    Red,
    Green,
    Blue,
}

/// A normalised curve control point.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    pub x: f32,
    pub y: f32,
}

/// Errors returned when constructing a production curve.
#[derive(Clone, Debug, PartialEq)]
pub enum CurveError {
    TooFewPoints,
    NonFinitePoint { index: usize },
    PointOutOfRange { index: usize },
    DuplicateX { index: usize },
}

impl fmt::Display for CurveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewPoints => write!(formatter, "a curve requires at least two points"),
            Self::NonFinitePoint { index } => {
                write!(formatter, "curve point {index} contains a non-finite value")
            }
            Self::PointOutOfRange { index } => {
                write!(
                    formatter,
                    "curve point {index} must be within the 0..=1 domain"
                )
            }
            Self::DuplicateX { index } => {
                write!(
                    formatter,
                    "curve point {index} has the same x value as its predecessor"
                )
            }
        }
    }
}

impl std::error::Error for CurveError {}

/// A production Smooth tone curve.
///
/// The evaluator uses safeguarded cubic Hermite interpolation. Its tangents
/// are limited to the neighbouring secants, so each interval remains between
/// its two control-point values even when the curve is edited into an
/// inversion. Values outside the first and last control-point x positions use
/// constant tails, matching the `FocalCurve` evaluator.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SmoothCurve {
    points: Vec<CurvePoint>,
}

#[derive(Deserialize)]
struct SmoothCurveData {
    points: Vec<CurvePoint>,
}

impl<'de> Deserialize<'de> for SmoothCurve {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let data = SmoothCurveData::deserialize(deserializer)?;
        Self::from_points(data.points).map_err(serde::de::Error::custom)
    }
}

impl Default for SmoothCurve {
    fn default() -> Self {
        Self::identity()
    }
}

impl SmoothCurve {
    /// Creates a curve, sorting points by x as the `FocalCurve` constructor does.
    ///
    /// # Errors
    ///
    /// Returns [`CurveError`] if there are fewer than two points, a point is
    /// non-finite or outside the normalised domain, or two points share an x
    /// coordinate.
    pub fn from_points(mut points: Vec<CurvePoint>) -> Result<Self, CurveError> {
        if points.len() < 2 {
            return Err(CurveError::TooFewPoints);
        }
        for (index, point) in points.iter().enumerate() {
            if !point.x.is_finite() || !point.y.is_finite() {
                return Err(CurveError::NonFinitePoint { index });
            }
            if !(0.0..=1.0).contains(&point.x) || !(0.0..=1.0).contains(&point.y) {
                return Err(CurveError::PointOutOfRange { index });
            }
        }

        points.sort_by(|left, right| left.x.total_cmp(&right.x));
        if let Some(index) = points.windows(2).position(|pair| pair[0].x >= pair[1].x) {
            return Err(CurveError::DuplicateX { index: index + 1 });
        }
        Ok(Self { points })
    }

    /// Returns the five-point identity curve used by the existing harness.
    #[must_use]
    pub fn identity() -> Self {
        Self {
            points: [0.0, 0.25, 0.5, 0.75, 1.0]
                .into_iter()
                .map(|value| CurvePoint { x: value, y: value })
                .collect(),
        }
    }

    #[must_use]
    pub fn points(&self) -> &[CurvePoint] {
        &self.points
    }

    /// Evaluates the curve in the normalised encoded Adobe RGB domain.
    #[must_use]
    pub fn evaluate(&self, x: f32) -> f32 {
        if !x.is_finite() {
            return 0.0;
        }
        let x = x.clamp(0.0, 1.0);
        if x <= self.points[0].x {
            return self.points[0].y;
        }
        if x >= self.points[self.points.len() - 1].x {
            return self.points[self.points.len() - 1].y;
        }

        let segment = self.segment_for_x(x);
        let left = self.points[segment];
        let right = self.points[segment + 1];
        let width = right.x - left.x;
        let t = (x - left.x) / width;
        let value = hermite_value(
            left.y,
            right.y,
            width,
            self.tangent(segment),
            self.tangent(segment + 1),
            t,
        );
        value.clamp(left.y.min(right.y), left.y.max(right.y))
    }

    fn segment_for_x(&self, x: f32) -> usize {
        self.points
            .windows(2)
            .position(|pair| x <= pair[1].x)
            .unwrap_or(self.points.len() - 2)
    }

    fn secant(&self, index: usize) -> f32 {
        let left = self.points[index];
        let right = self.points[index + 1];
        (right.y - left.y) / (right.x - left.x).max(f32::EPSILON)
    }

    fn tangent(&self, index: usize) -> f32 {
        if index == 0 {
            return self.secant(0);
        }
        if index + 1 == self.points.len() {
            return self.secant(index - 1);
        }
        limited_tangent(
            self.secant(index - 1),
            self.secant(index),
            self.points[index].x - self.points[index - 1].x,
            self.points[index + 1].x - self.points[index].x,
        )
    }
}

/// The complete set of production curves.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CurveSet {
    pub linked: SmoothCurve,
    pub luma: SmoothCurve,
    pub red: SmoothCurve,
    pub green: SmoothCurve,
    pub blue: SmoothCurve,
}

impl CurveSet {
    #[must_use]
    pub fn curve(&self, mode: CurveMode, channel: CurveChannel) -> &SmoothCurve {
        match mode {
            CurveMode::LinkedRgb => &self.linked,
            CurveMode::Luma => &self.luma,
            CurveMode::PerChannelRgb => match channel {
                CurveChannel::Red => &self.red,
                CurveChannel::Green => &self.green,
                CurveChannel::Blue => &self.blue,
            },
        }
    }

    /// Applies the selected production curve to one encoded Adobe RGB pixel.
    #[must_use]
    pub fn apply(&self, mode: CurveMode, rgb: [f32; 3]) -> [f32; 3] {
        match mode {
            CurveMode::LinkedRgb => [
                self.linked.evaluate(rgb[0]),
                self.linked.evaluate(rgb[1]),
                self.linked.evaluate(rgb[2]),
            ],
            CurveMode::PerChannelRgb => [
                self.red.evaluate(rgb[0]),
                self.green.evaluate(rgb[1]),
                self.blue.evaluate(rgb[2]),
            ],
            CurveMode::Luma => {
                let luma = adobe_rgb_luma(rgb);
                let adjusted = self.luma.evaluate(luma);
                if luma > 0.0 {
                    let scale = adjusted / luma;
                    [
                        (rgb[0] * scale).clamp(0.0, 1.0),
                        (rgb[1] * scale).clamp(0.0, 1.0),
                        (rgb[2] * scale).clamp(0.0, 1.0),
                    ]
                } else {
                    [adjusted; 3]
                }
            }
        }
    }
}

/// Calculates encoded Adobe RGB (1998) Luma and bounds it to the curve
/// domain.
#[must_use]
pub fn adobe_rgb_luma(rgb: [f32; 3]) -> f32 {
    let value = ADOBE_RGB_LUMA_COEFFICIENTS[0] * rgb[0]
        + ADOBE_RGB_LUMA_COEFFICIENTS[1] * rgb[1]
        + ADOBE_RGB_LUMA_COEFFICIENTS[2] * rgb[2];
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn limited_tangent(left: f32, right: f32, left_width: f32, right_width: f32) -> f32 {
    if left == 0.0 || right == 0.0 || left.signum() != right.signum() {
        return 0.0;
    }
    let harmonic = (left_width + right_width) / (left_width / left + right_width / right);
    let limit = 3.0 * left.abs().min(right.abs());
    harmonic.clamp(-limit, limit)
}

fn hermite_value(
    left: f32,
    right: f32,
    width: f32,
    left_tangent: f32,
    right_tangent: f32,
    t: f32,
) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    h00 * left + h10 * width * left_tangent + h01 * right + h11 * width * right_tangent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_curve_is_identity_at_and_between_points() {
        let curve = SmoothCurve::identity();
        for x in [0.0, 0.03, 0.25, 0.41, 0.75, 1.0] {
            assert!((curve.evaluate(x) - x).abs() < 1e-5, "x={x}");
        }
    }

    #[test]
    fn constructor_matches_focalcurve_sorting_and_rejects_invalid_points() {
        let curve = SmoothCurve::from_points(vec![
            CurvePoint { x: 1.0, y: 1.0 },
            CurvePoint { x: 0.0, y: 0.0 },
        ])
        .expect("points are sorted by x");
        assert_eq!(curve.points()[0], CurvePoint { x: 0.0, y: 0.0 });
        assert_eq!(
            SmoothCurve::from_points(vec![CurvePoint { x: 0.0, y: 0.0 }]),
            Err(CurveError::TooFewPoints)
        );
        assert_eq!(
            SmoothCurve::from_points(vec![
                CurvePoint { x: 0.0, y: 0.0 },
                CurvePoint {
                    x: f32::NAN,
                    y: 0.5
                },
            ]),
            Err(CurveError::NonFinitePoint { index: 1 })
        );
        assert_eq!(
            SmoothCurve::from_points(vec![
                CurvePoint { x: 0.0, y: 0.0 },
                CurvePoint { x: 0.0, y: 0.5 },
            ]),
            Err(CurveError::DuplicateX { index: 1 })
        );
    }

    #[test]
    fn smooth_interpolation_stays_between_adjacent_control_values() {
        let curve = SmoothCurve::from_points(vec![
            CurvePoint { x: 0.0, y: 0.0 },
            CurvePoint { x: 0.3, y: 0.9 },
            CurvePoint { x: 1.0, y: 1.0 },
        ])
        .expect("valid curve");
        for pair in curve.points().windows(2) {
            for step in 0..=100_u16 {
                let x = pair[0].x + (pair[1].x - pair[0].x) * f32::from(step) / 100.0;
                let value = curve.evaluate(x);
                assert!(value >= pair[0].y.min(pair[1].y) - 1e-5);
                assert!(value <= pair[0].y.max(pair[1].y) + 1e-5);
            }
        }
    }

    #[test]
    fn linked_and_per_channel_modes_apply_their_selected_curves() {
        let linked = SmoothCurve::from_points(vec![
            CurvePoint { x: 0.0, y: 0.0 },
            CurvePoint { x: 1.0, y: 0.5 },
        ])
        .expect("valid curve");
        let red = SmoothCurve::from_points(vec![
            CurvePoint { x: 0.0, y: 0.25 },
            CurvePoint { x: 1.0, y: 0.75 },
        ])
        .expect("valid curve");
        let curves = CurveSet {
            linked,
            red,
            ..CurveSet::default()
        };
        let linked_output = curves.apply(CurveMode::LinkedRgb, [0.2, 0.4, 0.8]);
        for (actual, expected) in linked_output.into_iter().zip([0.1, 0.2, 0.4]) {
            assert!((actual - expected).abs() < 1e-5);
        }
        let channel_output = curves.apply(CurveMode::PerChannelRgb, [0.2, 0.4, 0.8]);
        assert!((channel_output[0] - 0.35).abs() < 1e-5);
        assert!((channel_output[1] - 0.4).abs() < 1e-5);
        assert!((channel_output[2] - 0.8).abs() < 1e-5);
    }

    #[test]
    fn luma_uses_adobe_rgb_coefficients_and_preserves_channel_ratios() {
        let curves = CurveSet::default();
        let rgb = [0.0001, 0.0, 0.0];
        let output = curves.apply(CurveMode::Luma, rgb);
        assert!((adobe_rgb_luma(rgb) - ADOBE_RGB_LUMA_COEFFICIENTS[0] * rgb[0]).abs() < 1e-8);
        assert!((output[0] - rgb[0]).abs() < 1e-6);
        assert!(output[1] < 1e-8 && output[2] < 1e-8);
    }

    #[test]
    fn curve_set_round_trips_through_json() {
        let encoded = serde_json::to_string(&CurveSet::default()).expect("serialisable curves");
        let decoded: CurveSet = serde_json::from_str(&encoded).expect("valid curve JSON");
        assert_eq!(decoded, CurveSet::default());
    }

    #[test]
    fn deserialisation_rejects_invalid_curve_points() {
        let result = serde_json::from_str::<SmoothCurve>(
            r#"{"points":[{"x":0.0,"y":0.0},{"x":0.0,"y":0.5}]}"#,
        );
        assert!(result.is_err());
    }
}

//! Curve evaluation independent of egui.
//!
//! The curve domain is canonical encoded Adobe RGB (1998): `0.0` is encoded
//! black and `1.0` is encoded white. It is deliberately not a scene-linear or
//! unbounded RAW domain.

#![allow(clippy::cast_precision_loss)]

const MIN_X_GAP: f32 = 0.008;
const MIN_INSERT_GAP: f32 = 0.012;
pub const CURVE_DOMAIN_LABEL: &str = "canonical encoded Adobe RGB (1998)";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveMode {
    LinkedRgb,
    Luminance,
    PerChannelRgb,
}

impl CurveMode {
    pub const ALL: [Self; 3] = [Self::LinkedRgb, Self::Luminance, Self::PerChannelRgb];

    pub const fn label(self) -> &'static str {
        match self {
            Self::LinkedRgb => "Linked RGB",
            Self::Luminance => "Luminance",
            Self::PerChannelRgb => "Per-channel RGB",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::LinkedRgb => "One curve is applied independently to red, green, and blue.",
            Self::Luminance => "Brightness changes are applied to a perceived-luminance estimate.",
            Self::PerChannelRgb => "Red, green, and blue each have an independent curve.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveChannel {
    Red,
    Green,
    Blue,
}

impl CurveChannel {
    pub const ALL: [Self; 3] = [Self::Red, Self::Green, Self::Blue];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Red => "Red",
            Self::Green => "Green",
            Self::Blue => "Blue",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LuminanceDefinition {
    AdobeRgb,
    Rec709,
    EqualEnergy,
}

impl LuminanceDefinition {
    pub const ALL: [Self; 3] = [Self::AdobeRgb, Self::Rec709, Self::EqualEnergy];

    pub const fn label(self) -> &'static str {
        match self {
            Self::AdobeRgb => "Adobe RGB (1998)",
            Self::Rec709 => "Rec. 709",
            Self::EqualEnergy => "Equal energy",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveInterpolation {
    Smooth,
    Linear,
    Bezier,
}

impl CurveInterpolation {
    pub const ALL: [Self; 3] = [Self::Smooth, Self::Linear, Self::Bezier];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Smooth => "Smooth cubic",
            Self::Linear => "Linear",
            Self::Bezier => "Bezier handles",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Smooth => "Safeguarded cubic interpolation between control points.",
            Self::Linear => "Straight line segments between control points.",
            Self::Bezier => "Piecewise cubic Bezier segments with independently draggable handles.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BezierHandleKind {
    Incoming,
    Outgoing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BezierHandles {
    pub incoming: Option<ControlPoint>,
    pub outgoing: Option<ControlPoint>,
}

impl BezierHandles {
    fn get(self, kind: BezierHandleKind) -> Option<ControlPoint> {
        match kind {
            BezierHandleKind::Incoming => self.incoming,
            BezierHandleKind::Outgoing => self.outgoing,
        }
    }

    fn set(&mut self, kind: BezierHandleKind, value: Option<ControlPoint>) {
        match kind {
            BezierHandleKind::Incoming => self.incoming = value,
            BezierHandleKind::Outgoing => self.outgoing = value,
        }
    }
}

fn order_segment_handle_x(handles: &mut [BezierHandles]) {
    for segment in 0..handles.len().saturating_sub(1) {
        let (left, right) = handles.split_at_mut(segment + 1);
        let Some(outgoing) = left[segment].outgoing else {
            continue;
        };
        let Some(incoming) = right[0].incoming else {
            continue;
        };
        if outgoing.x > incoming.x {
            let middle = (outgoing.x + incoming.x) * 0.5;
            left[segment].outgoing = Some(ControlPoint {
                x: middle,
                ..outgoing
            });
            right[0].incoming = Some(ControlPoint {
                x: middle,
                ..incoming
            });
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    points: Vec<ControlPoint>,
    bezier_handles: Vec<BezierHandles>,
    tensions: Vec<f32>,
}

impl Default for Curve {
    fn default() -> Self {
        Self::identity()
    }
}

impl Curve {
    pub fn identity() -> Self {
        Self::from_points(
            [0.0, 0.25, 0.5, 0.75, 1.0]
                .into_iter()
                .map(|value| ControlPoint { x: value, y: value })
                .collect(),
        )
        .expect("identity points are valid")
    }

    pub fn from_points(mut points: Vec<ControlPoint>) -> Option<Self> {
        if points.len() < 2
            || points.iter().any(|point| {
                !point.x.is_finite()
                    || !point.y.is_finite()
                    || !(0.0..=1.0).contains(&point.x)
                    || !(0.0..=1.0).contains(&point.y)
            })
        {
            return None;
        }

        points.sort_by(|a, b| a.x.total_cmp(&b.x));
        if points.windows(2).any(|pair| pair[0].x >= pair[1].x) {
            return None;
        }
        let mut curve = Self {
            bezier_handles: default_handles(&points, true),
            tensions: default_tensions(&points),
            points,
        };
        curve.normalise_handles();
        Some(curve)
    }

    pub fn points(&self) -> &[ControlPoint] {
        &self.points
    }

    pub fn bezier_handles(&self) -> &[BezierHandles] {
        &self.bezier_handles
    }

    pub fn handle(&self, index: usize, kind: BezierHandleKind) -> Option<ControlPoint> {
        self.bezier_handles.get(index)?.get(kind)
    }

    pub fn tension(&self, index: usize) -> f32 {
        self.tensions.get(index).copied().unwrap_or(1.0)
    }

    pub fn set_tension(&mut self, index: usize, tension: f32) {
        if let Some(value) = self.tensions.get_mut(index)
            && tension.is_finite()
        {
            *value = tension.clamp(0.1, 4.0);
        }
    }

    #[allow(dead_code)]
    pub fn evaluate(&self, x: f32) -> f32 {
        self.evaluate_with_interpolation(x, CurveInterpolation::Smooth)
    }

    pub fn evaluate_with_interpolation(&self, x: f32, interpolation: CurveInterpolation) -> f32 {
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
        let h = right.x - left.x;
        let t = (x - left.x) / h;

        match interpolation {
            CurveInterpolation::Linear => left.y + (right.y - left.y) * t,
            CurveInterpolation::Smooth => {
                let value = hermite_value(
                    left.y,
                    right.y,
                    h,
                    self.tangent(segment),
                    self.tangent(segment + 1),
                    t,
                );
                // A user-created inversion is still represented because this
                // clamp is local to the interval, not global monotonicity.
                value.clamp(left.y.min(right.y), left.y.max(right.y))
            }
            CurveInterpolation::Bezier => {
                let (outgoing, incoming) = self.bezier_controls(segment);
                let t = solve_bezier_parameter(x, left.x, outgoing.x, incoming.x, right.x);
                cubic_value(left.y, outgoing.y, incoming.y, right.y, t).clamp(0.0, 1.0)
            }
        }
    }

    pub fn derivative_at(&self, x: f32, interpolation: CurveInterpolation) -> f32 {
        if !x.is_finite() {
            return 0.0;
        }
        // Outside the anchor span the tone curve has constant tails. At the
        // anchors themselves use the one-sided segment derivative so an
        // identity curve correctly reports slope 1 at both ends.
        if x < self.points[0].x || x > self.points[self.points.len() - 1].x {
            return 0.0;
        }
        let x = x.clamp(0.0, 1.0);
        let segment = self.segment_for_x(x);
        let left = self.points[segment];
        let right = self.points[segment + 1];
        let h = right.x - left.x;
        let t = (x - left.x) / h;
        match interpolation {
            CurveInterpolation::Linear => (right.y - left.y) / h,
            CurveInterpolation::Smooth => hermite_derivative(
                left.y,
                right.y,
                h,
                self.tangent(segment),
                self.tangent(segment + 1),
                t,
            ),
            CurveInterpolation::Bezier => {
                let (outgoing, incoming) = self.bezier_controls(segment);
                let t = solve_bezier_parameter(x, left.x, outgoing.x, incoming.x, right.x);
                let dx = cubic_derivative(left.x, outgoing.x, incoming.x, right.x, t);
                let dy = cubic_derivative(left.y, outgoing.y, incoming.y, right.y, t);
                if dx.abs() <= f32::EPSILON {
                    0.0
                } else {
                    dy / dx
                }
            }
        }
    }

    pub fn derivative_anchor_values(&self, interpolation: CurveInterpolation) -> Vec<f32> {
        self.points
            .iter()
            .enumerate()
            .map(|(index, point)| match interpolation {
                CurveInterpolation::Linear => {
                    if index == 0 {
                        self.secant(0)
                    } else if index == self.points.len() - 1 {
                        self.secant(index - 1)
                    } else {
                        (self.secant(index - 1) + self.secant(index)) * 0.5
                    }
                }
                CurveInterpolation::Smooth | CurveInterpolation::Bezier => {
                    let epsilon = 0.000_001;
                    let sample_x = if index == 0 {
                        (point.x + epsilon).min(self.points[index + 1].x)
                    } else if index == self.points.len() - 1 {
                        (point.x - epsilon).max(self.points[index - 1].x)
                    } else {
                        point.x
                    };
                    self.derivative_at(sample_x, interpolation)
                }
            })
            .collect()
    }

    #[allow(dead_code)]
    pub fn sample(&self, count: usize) -> Vec<[f32; 2]> {
        self.sample_with_interpolation(count, CurveInterpolation::Smooth)
    }

    pub fn sample_with_interpolation(
        &self,
        count: usize,
        interpolation: CurveInterpolation,
    ) -> Vec<[f32; 2]> {
        (0..count.max(2))
            .map(|index| {
                let x = index as f32 / (count.max(2) - 1) as f32;
                [x, self.evaluate_with_interpolation(x, interpolation)]
            })
            .collect()
    }

    pub fn insert_point(&mut self, x: f32, y: f32) -> bool {
        if !x.is_finite() || !y.is_finite() {
            return false;
        }
        let x = x.clamp(0.0, 1.0);
        let y = y.clamp(0.0, 1.0);
        let Some(index) = self.insertion_index(x) else {
            return false;
        };
        self.points.insert(index, ControlPoint { x, y });
        self.bezier_handles.insert(index, BezierHandles::default());
        self.tensions.insert(index, 1.0);
        self.reset_new_handle_defaults(index);
        self.normalise_handles();
        true
    }

    pub fn insert_point_on_curve(&mut self, x: f32, interpolation: CurveInterpolation) -> bool {
        if !x.is_finite() {
            return false;
        }
        let x = x.clamp(0.0, 1.0);
        let y = self.evaluate_with_interpolation(x, interpolation);
        self.insert_point(x, y)
    }

    pub fn delete_point(&mut self, index: usize) -> bool {
        if index == 0 || index + 1 >= self.points.len() {
            return false;
        }
        self.points.remove(index);
        self.bezier_handles.remove(index);
        self.tensions.remove(index);
        self.normalise_handles();
        true
    }

    pub fn reset_handle(&mut self, index: usize, kind: BezierHandleKind) -> bool {
        let Some(value) = default_handle(&self.points, index, kind, true) else {
            return false;
        };
        let Some(handles) = self.bezier_handles.get_mut(index) else {
            return false;
        };
        handles.set(kind, Some(value));
        self.normalise_handle_positions();
        self.align_handle_pair(index, Some(kind));
        true
    }

    pub fn dragged_from(initial: &Self, selected: usize, target_x: f32, target_y: f32) -> Self {
        if !target_x.is_finite() || !target_y.is_finite() {
            return initial.clone();
        }
        let mut result = initial.clone();
        if selected >= result.points.len() {
            return result;
        }

        let point_x = clamp_ordered_x(&initial.points, selected, target_x);
        let delta = target_y.clamp(0.0, 1.0) - initial.points[selected].y;
        for (index, point) in result.points.iter_mut().enumerate() {
            let weight = if index == selected { 1.0 } else { 0.0 };
            point.y = (initial.points[index].y + delta * weight).clamp(0.0, 1.0);
        }
        result.points[selected].x = point_x;
        result.points[selected].y = target_y.clamp(0.0, 1.0);
        result.translate_handles_from(initial);
        result.normalise_handles();
        result
    }

    pub fn dragged_handle_from(
        initial: &Self,
        index: usize,
        kind: BezierHandleKind,
        target_x: f32,
        target_y: f32,
    ) -> Self {
        if !target_x.is_finite() || !target_y.is_finite() {
            return initial.clone();
        }
        let mut result = initial.clone();
        let Some(mut handle) = result.handle(index, kind) else {
            return result;
        };
        let Some(point) = result.points.get(index).copied() else {
            return result;
        };
        let minimum = if kind == BezierHandleKind::Incoming {
            result.points[index.saturating_sub(1)].x
        } else {
            point.x
        };
        let maximum = if kind == BezierHandleKind::Incoming {
            point.x
        } else {
            result.points[(index + 1).min(result.points.len() - 1)].x
        };
        handle.x = if minimum <= maximum {
            target_x.clamp(minimum, maximum)
        } else {
            handle.x
        };
        handle.y = target_y.clamp(0.0, 1.0);
        result.bezier_handles[index].set(kind, Some(handle));
        result.normalise_handle_positions();
        result.align_handle_pair(index, Some(kind));
        result
    }

    pub fn derivative_curve(&self, interpolation: CurveInterpolation) -> DerivativeCurve {
        DerivativeCurve::from_points(
            self.points
                .iter()
                .copied()
                .zip(self.derivative_anchor_values(interpolation))
                .map(|(point, y)| ControlPoint { x: point.x, y })
                .collect(),
        )
        .expect("curve derivative points are ordered")
    }

    pub fn apply_derivative_edit(
        initial: &Self,
        initial_derivative: &DerivativeCurve,
        edited_derivative: &DerivativeCurve,
        interpolation: CurveInterpolation,
    ) -> Self {
        if initial.points.len() != initial_derivative.points.len()
            || initial.points.len() != edited_derivative.points.len()
        {
            return initial.clone();
        }
        let mut result = initial.clone();
        for (point, edited) in result.points.iter_mut().zip(&edited_derivative.points) {
            point.x = edited.x;
        }
        let mut accumulated_delta = 0.0;
        for index in 0..result.points.len() - 1 {
            let old_area = initial_derivative.integrate(
                initial.points[index].x,
                initial.points[index + 1].x,
                interpolation,
            );
            let new_area = edited_derivative.integrate(
                edited_derivative.points[index].x,
                edited_derivative.points[index + 1].x,
                interpolation,
            );
            accumulated_delta += new_area - old_area;
            result.points[index + 1].y =
                (initial.points[index + 1].y + accumulated_delta).clamp(0.0, 1.0);
        }
        result.translate_handles_from(initial);
        result.normalise_handles();
        result
    }

    fn segment_for_x(&self, x: f32) -> usize {
        self.points
            .windows(2)
            .position(|pair| x <= pair[1].x)
            .unwrap_or(self.points.len() - 2)
    }

    fn insertion_index(&self, x: f32) -> Option<usize> {
        if self
            .points
            .iter()
            .any(|point| (point.x - x).abs() < MIN_INSERT_GAP)
        {
            return None;
        }
        Some(
            self.points
                .iter()
                .position(|point| point.x > x)
                .unwrap_or(self.points.len()),
        )
    }

    fn secant(&self, index: usize) -> f32 {
        let left = self.points[index];
        let right = self.points[index + 1];
        (right.y - left.y) / (right.x - left.x).max(f32::EPSILON)
    }

    fn tangent(&self, index: usize) -> f32 {
        if index == 0 {
            return self.secant(0) * self.tension(index);
        }
        if index + 1 == self.points.len() {
            return self.secant(index - 1) * self.tension(index);
        }
        limited_tangent(
            self.secant(index - 1),
            self.secant(index),
            self.points[index].x - self.points[index - 1].x,
            self.points[index + 1].x - self.points[index].x,
        ) * self.tension(index)
    }

    fn bezier_controls(&self, segment: usize) -> (ControlPoint, ControlPoint) {
        let left = self.points[segment];
        let right = self.points[segment + 1];
        (
            self.handle(segment, BezierHandleKind::Outgoing)
                .unwrap_or(left),
            self.handle(segment + 1, BezierHandleKind::Incoming)
                .unwrap_or(right),
        )
    }

    fn reset_new_handle_defaults(&mut self, index: usize) {
        for kind in [BezierHandleKind::Incoming, BezierHandleKind::Outgoing] {
            if let Some(value) = default_handle(&self.points, index, kind, true) {
                self.bezier_handles[index].set(kind, Some(value));
            }
        }
    }

    fn translate_handles_from(&mut self, initial: &Self) {
        for index in 0..self.points.len() {
            let dx = self.points[index].x - initial.points[index].x;
            let dy = self.points[index].y - initial.points[index].y;
            for kind in [BezierHandleKind::Incoming, BezierHandleKind::Outgoing] {
                if let Some(mut handle) = self.bezier_handles[index].get(kind) {
                    handle.x += dx;
                    handle.y += dy;
                    self.bezier_handles[index].set(kind, Some(handle));
                }
            }
        }
    }

    fn normalise_handles(&mut self) {
        self.normalise_handle_positions();
        for index in 1..self.points.len().saturating_sub(1) {
            self.align_handle_pair(index, None);
        }
        order_segment_handle_x(&mut self.bezier_handles);
    }

    fn normalise_handle_positions(&mut self) {
        for index in 0..self.points.len() {
            for kind in [BezierHandleKind::Incoming, BezierHandleKind::Outgoing] {
                let Some(mut handle) = self.bezier_handles[index].get(kind) else {
                    continue;
                };
                let point = self.points[index];
                let minimum = if kind == BezierHandleKind::Incoming {
                    self.points[index.saturating_sub(1)].x
                } else {
                    point.x
                };
                let maximum = if kind == BezierHandleKind::Incoming {
                    point.x
                } else {
                    self.points[(index + 1).min(self.points.len() - 1)].x
                };
                handle.x = handle.x.clamp(minimum.min(maximum), minimum.max(maximum));
                if !handle.y.is_finite() {
                    handle.y = point.y;
                }
                self.bezier_handles[index].set(kind, Some(handle));
            }
        }
        order_segment_handle_x(&mut self.bezier_handles);
    }

    fn align_handle_pair(&mut self, index: usize, preferred: Option<BezierHandleKind>) {
        if index == 0 || index + 1 >= self.points.len() {
            return;
        }
        let anchor = self.points[index];
        let incoming = self.bezier_handles[index].incoming.unwrap_or(anchor);
        let outgoing = self.bezier_handles[index].outgoing.unwrap_or(anchor);
        let incoming_vector = (incoming.x - anchor.x, incoming.y - anchor.y);
        let outgoing_vector = (outgoing.x - anchor.x, outgoing.y - anchor.y);
        let raw_outgoing = match preferred {
            Some(BezierHandleKind::Incoming) => (-incoming_vector.0, -incoming_vector.1),
            Some(BezierHandleKind::Outgoing) => outgoing_vector,
            None => {
                if outgoing_vector.0.abs() + outgoing_vector.1.abs() > f32::EPSILON {
                    outgoing_vector
                } else {
                    (-incoming_vector.0, -incoming_vector.1)
                }
            }
        };
        let raw_length = (raw_outgoing.0 * raw_outgoing.0 + raw_outgoing.1 * raw_outgoing.1).sqrt();
        let (unit_x, unit_y) = if raw_length > f32::EPSILON {
            (raw_outgoing.0 / raw_length, raw_outgoing.1 / raw_length)
        } else {
            (1.0, 0.0)
        };
        let (unit_x, unit_y) = if unit_x < 0.0 {
            (-unit_x, -unit_y)
        } else {
            (unit_x, unit_y)
        };
        let incoming_length =
            (incoming_vector.0 * incoming_vector.0 + incoming_vector.1 * incoming_vector.1).sqrt();
        let outgoing_length =
            (outgoing_vector.0 * outgoing_vector.0 + outgoing_vector.1 * outgoing_vector.1).sqrt();
        let outgoing_limit = if unit_x > f32::EPSILON {
            (self.points[index + 1].x - anchor.x) / unit_x
        } else {
            f32::INFINITY
        };
        let incoming_limit = if unit_x > f32::EPSILON {
            (anchor.x - self.points[index - 1].x) / unit_x
        } else {
            f32::INFINITY
        };
        let outgoing_length = outgoing_length.min(outgoing_limit.max(0.0));
        let incoming_length = incoming_length.min(incoming_limit.max(0.0));
        self.bezier_handles[index].outgoing = Some(ControlPoint {
            x: anchor.x + unit_x * outgoing_length,
            y: anchor.y + unit_y * outgoing_length,
        });
        self.bezier_handles[index].incoming = Some(ControlPoint {
            x: anchor.x - unit_x * incoming_length,
            y: anchor.y - unit_y * incoming_length,
        });
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DerivativeCurve {
    points: Vec<ControlPoint>,
    bezier_handles: Vec<BezierHandles>,
    tensions: Vec<f32>,
}

impl DerivativeCurve {
    pub fn from_points(mut points: Vec<ControlPoint>) -> Option<Self> {
        if points.len() < 2
            || points.iter().any(|point| {
                !point.x.is_finite() || !point.y.is_finite() || !(0.0..=1.0).contains(&point.x)
            })
        {
            return None;
        }
        points.sort_by(|a, b| a.x.total_cmp(&b.x));
        if points.windows(2).any(|pair| pair[0].x >= pair[1].x) {
            return None;
        }
        let mut curve = Self {
            bezier_handles: default_handles(&points, false),
            tensions: default_tensions(&points),
            points,
        };
        curve.normalise_handles();
        Some(curve)
    }

    pub fn points(&self) -> &[ControlPoint] {
        &self.points
    }

    pub fn set_point_y_near(&mut self, x: f32, y: f32, tolerance: f32) -> bool {
        if !x.is_finite() || !y.is_finite() || !tolerance.is_finite() || tolerance < 0.0 {
            return false;
        }
        let Some(point) = self
            .points
            .iter_mut()
            .find(|point| (point.x - x).abs() < tolerance)
        else {
            return false;
        };
        point.y = y;
        true
    }

    pub fn bezier_handles(&self) -> &[BezierHandles] {
        &self.bezier_handles
    }

    pub fn handle(&self, index: usize, kind: BezierHandleKind) -> Option<ControlPoint> {
        self.bezier_handles.get(index)?.get(kind)
    }

    pub fn tension(&self, index: usize) -> f32 {
        self.tensions.get(index).copied().unwrap_or(1.0)
    }

    pub fn set_tension(&mut self, index: usize, tension: f32) {
        if let Some(value) = self.tensions.get_mut(index)
            && tension.is_finite()
        {
            *value = tension.clamp(0.1, 4.0);
        }
    }

    pub fn evaluate_with_interpolation(&self, x: f32, interpolation: CurveInterpolation) -> f32 {
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
        let segment = self
            .points
            .windows(2)
            .position(|pair| x <= pair[1].x)
            .unwrap_or(self.points.len() - 2);
        let left = self.points[segment];
        let right = self.points[segment + 1];
        let h = right.x - left.x;
        let t = (x - left.x) / h;
        match interpolation {
            CurveInterpolation::Linear => left.y + (right.y - left.y) * t,
            CurveInterpolation::Smooth => hermite_value(
                left.y,
                right.y,
                h,
                self.tangent(segment),
                self.tangent(segment + 1),
                t,
            )
            .clamp(left.y.min(right.y), left.y.max(right.y)),
            CurveInterpolation::Bezier => {
                let outgoing = self
                    .handle(segment, BezierHandleKind::Outgoing)
                    .unwrap_or(left);
                let incoming = self
                    .handle(segment + 1, BezierHandleKind::Incoming)
                    .unwrap_or(right);
                let t = solve_bezier_parameter(x, left.x, outgoing.x, incoming.x, right.x);
                cubic_value(left.y, outgoing.y, incoming.y, right.y, t)
            }
        }
    }

    pub fn sample_with_interpolation(
        &self,
        count: usize,
        interpolation: CurveInterpolation,
    ) -> Vec<[f32; 2]> {
        (0..count.max(2))
            .map(|index| {
                let x = index as f32 / (count.max(2) - 1) as f32;
                [x, self.evaluate_with_interpolation(x, interpolation)]
            })
            .collect()
    }

    pub fn insert_point_on_curve(&mut self, x: f32, interpolation: CurveInterpolation) -> bool {
        if !x.is_finite() {
            return false;
        }
        let x = x.clamp(0.0, 1.0);
        let Some(index) = self.insertion_index(x) else {
            return false;
        };
        let y = self.evaluate_with_interpolation(x, interpolation);
        self.points.insert(index, ControlPoint { x, y });
        self.bezier_handles.insert(index, BezierHandles::default());
        self.tensions.insert(index, 1.0);
        self.reset_new_handle_defaults(index);
        self.normalise_handles();
        true
    }

    pub fn delete_point(&mut self, index: usize) -> bool {
        if index == 0 || index + 1 >= self.points.len() {
            return false;
        }
        self.points.remove(index);
        self.bezier_handles.remove(index);
        self.tensions.remove(index);
        self.normalise_handles();
        true
    }

    pub fn reset_handle(&mut self, index: usize, kind: BezierHandleKind) -> bool {
        let Some(value) = default_handle(&self.points, index, kind, false) else {
            return false;
        };
        let Some(handles) = self.bezier_handles.get_mut(index) else {
            return false;
        };
        handles.set(kind, Some(value));
        self.normalise_handle_positions();
        self.align_handle_pair(index, Some(kind));
        true
    }

    pub fn dragged_from(initial: &Self, selected: usize, target_x: f32, target_y: f32) -> Self {
        if !target_x.is_finite() || !target_y.is_finite() {
            return initial.clone();
        }
        let mut result = initial.clone();
        if selected >= result.points.len() {
            return result;
        }
        let point_x = clamp_ordered_x(&initial.points, selected, target_x);
        let delta = target_y - initial.points[selected].y;
        for (index, point) in result.points.iter_mut().enumerate() {
            let weight = if index == selected { 1.0 } else { 0.0 };
            point.y = initial.points[index].y + delta * weight;
        }
        result.points[selected].x = point_x;
        result.points[selected].y = target_y;
        result.translate_handles_from(initial);
        result.normalise_handles();
        result
    }

    pub fn dragged_handle_from(
        initial: &Self,
        index: usize,
        kind: BezierHandleKind,
        target_x: f32,
        target_y: f32,
    ) -> Self {
        if !target_x.is_finite() || !target_y.is_finite() {
            return initial.clone();
        }
        let mut result = initial.clone();
        let Some(mut handle) = result.handle(index, kind) else {
            return result;
        };
        let Some(point) = result.points.get(index).copied() else {
            return result;
        };
        let minimum = if kind == BezierHandleKind::Incoming {
            result.points[index.saturating_sub(1)].x
        } else {
            point.x
        };
        let maximum = if kind == BezierHandleKind::Incoming {
            point.x
        } else {
            result.points[(index + 1).min(result.points.len() - 1)].x
        };
        handle.x = if minimum <= maximum {
            target_x.clamp(minimum, maximum)
        } else {
            handle.x
        };
        handle.y = target_y;
        result.bezier_handles[index].set(kind, Some(handle));
        result.normalise_handle_positions();
        result.align_handle_pair(index, Some(kind));
        result
    }

    fn integrate(&self, start: f32, end: f32, interpolation: CurveInterpolation) -> f32 {
        let width = end - start;
        if width.abs() <= f32::EPSILON {
            return 0.0;
        }
        // Trapezoidal integration is sufficient for this interactive reference
        // and lets derivative handle edits affect the underlying tone curve.
        let steps = 24;
        let mut area = 0.0;
        let step = width / steps as f32;
        for index in 0..steps {
            let x0 = start + index as f32 * step;
            let x1 = x0 + step;
            area += (self.evaluate_with_interpolation(x0, interpolation)
                + self.evaluate_with_interpolation(x1, interpolation))
                * 0.5
                * step;
        }
        area
    }

    fn insertion_index(&self, x: f32) -> Option<usize> {
        if self
            .points
            .iter()
            .any(|point| (point.x - x).abs() < MIN_INSERT_GAP)
        {
            return None;
        }
        Some(
            self.points
                .iter()
                .position(|point| point.x > x)
                .unwrap_or(self.points.len()),
        )
    }

    fn tangent(&self, index: usize) -> f32 {
        if index == 0 {
            return ((self.points[1].y - self.points[0].y)
                / (self.points[1].x - self.points[0].x).max(f32::EPSILON))
                * self.tension(index);
        }
        if index + 1 == self.points.len() {
            return ((self.points[index].y - self.points[index - 1].y)
                / (self.points[index].x - self.points[index - 1].x).max(f32::EPSILON))
                * self.tension(index);
        }
        let left = (self.points[index].y - self.points[index - 1].y)
            / (self.points[index].x - self.points[index - 1].x).max(f32::EPSILON);
        let right = (self.points[index + 1].y - self.points[index].y)
            / (self.points[index + 1].x - self.points[index].x).max(f32::EPSILON);
        limited_tangent(
            left,
            right,
            self.points[index].x - self.points[index - 1].x,
            self.points[index + 1].x - self.points[index].x,
        ) * self.tension(index)
    }

    fn reset_new_handle_defaults(&mut self, index: usize) {
        for kind in [BezierHandleKind::Incoming, BezierHandleKind::Outgoing] {
            if let Some(value) = default_handle(&self.points, index, kind, false) {
                self.bezier_handles[index].set(kind, Some(value));
            }
        }
    }

    fn translate_handles_from(&mut self, initial: &Self) {
        for index in 0..self.points.len() {
            let dx = self.points[index].x - initial.points[index].x;
            let dy = self.points[index].y - initial.points[index].y;
            for kind in [BezierHandleKind::Incoming, BezierHandleKind::Outgoing] {
                if let Some(mut handle) = self.bezier_handles[index].get(kind) {
                    handle.x += dx;
                    handle.y += dy;
                    self.bezier_handles[index].set(kind, Some(handle));
                }
            }
        }
    }

    fn normalise_handles(&mut self) {
        self.normalise_handle_positions();
        for index in 1..self.points.len().saturating_sub(1) {
            self.align_handle_pair(index, None);
        }
        order_segment_handle_x(&mut self.bezier_handles);
    }

    fn normalise_handle_positions(&mut self) {
        for index in 0..self.points.len() {
            for kind in [BezierHandleKind::Incoming, BezierHandleKind::Outgoing] {
                let Some(mut handle) = self.bezier_handles[index].get(kind) else {
                    continue;
                };
                let point = self.points[index];
                let minimum = if kind == BezierHandleKind::Incoming {
                    self.points[index.saturating_sub(1)].x
                } else {
                    point.x
                };
                let maximum = if kind == BezierHandleKind::Incoming {
                    point.x
                } else {
                    self.points[(index + 1).min(self.points.len() - 1)].x
                };
                handle.x = handle.x.clamp(minimum.min(maximum), minimum.max(maximum));
                if !handle.y.is_finite() {
                    handle.y = point.y;
                }
                self.bezier_handles[index].set(kind, Some(handle));
            }
        }
        order_segment_handle_x(&mut self.bezier_handles);
    }

    fn align_handle_pair(&mut self, index: usize, preferred: Option<BezierHandleKind>) {
        if index == 0 || index + 1 >= self.points.len() {
            return;
        }
        let anchor = self.points[index];
        let incoming = self.bezier_handles[index].incoming.unwrap_or(anchor);
        let outgoing = self.bezier_handles[index].outgoing.unwrap_or(anchor);
        let incoming_vector = (incoming.x - anchor.x, incoming.y - anchor.y);
        let outgoing_vector = (outgoing.x - anchor.x, outgoing.y - anchor.y);
        let raw_outgoing = match preferred {
            Some(BezierHandleKind::Incoming) => (-incoming_vector.0, -incoming_vector.1),
            Some(BezierHandleKind::Outgoing) => outgoing_vector,
            None => {
                if outgoing_vector.0.abs() + outgoing_vector.1.abs() > f32::EPSILON {
                    outgoing_vector
                } else {
                    (-incoming_vector.0, -incoming_vector.1)
                }
            }
        };
        let raw_length = (raw_outgoing.0 * raw_outgoing.0 + raw_outgoing.1 * raw_outgoing.1).sqrt();
        let (unit_x, unit_y) = if raw_length > f32::EPSILON {
            (raw_outgoing.0 / raw_length, raw_outgoing.1 / raw_length)
        } else {
            (1.0, 0.0)
        };
        let (unit_x, unit_y) = if unit_x < 0.0 {
            (-unit_x, -unit_y)
        } else {
            (unit_x, unit_y)
        };
        let incoming_length =
            (incoming_vector.0 * incoming_vector.0 + incoming_vector.1 * incoming_vector.1).sqrt();
        let outgoing_length =
            (outgoing_vector.0 * outgoing_vector.0 + outgoing_vector.1 * outgoing_vector.1).sqrt();
        let outgoing_limit = if unit_x > f32::EPSILON {
            (self.points[index + 1].x - anchor.x) / unit_x
        } else {
            f32::INFINITY
        };
        let incoming_limit = if unit_x > f32::EPSILON {
            (anchor.x - self.points[index - 1].x) / unit_x
        } else {
            f32::INFINITY
        };
        let outgoing_length = outgoing_length.min(outgoing_limit.max(0.0));
        let incoming_length = incoming_length.min(incoming_limit.max(0.0));
        self.bezier_handles[index].outgoing = Some(ControlPoint {
            x: anchor.x + unit_x * outgoing_length,
            y: anchor.y + unit_y * outgoing_length,
        });
        self.bezier_handles[index].incoming = Some(ControlPoint {
            x: anchor.x - unit_x * incoming_length,
            y: anchor.y - unit_y * incoming_length,
        });
    }
}

#[allow(clippy::field_reassign_with_default)]
fn default_handles(points: &[ControlPoint], clamp_y: bool) -> Vec<BezierHandles> {
    points
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let mut handles = BezierHandles::default();
            handles.incoming = default_handle(points, index, BezierHandleKind::Incoming, clamp_y);
            handles.outgoing = default_handle(points, index, BezierHandleKind::Outgoing, clamp_y);
            handles
        })
        .collect()
}

fn default_tensions(points: &[ControlPoint]) -> Vec<f32> {
    vec![1.0; points.len()]
}

fn default_handle(
    points: &[ControlPoint],
    index: usize,
    kind: BezierHandleKind,
    clamp_y: bool,
) -> Option<ControlPoint> {
    let point = *points.get(index)?;
    let (other, direction) = match kind {
        BezierHandleKind::Incoming if index > 0 => (points[index - 1], -1.0),
        BezierHandleKind::Outgoing if index + 1 < points.len() => (points[index + 1], 1.0),
        _ => return None,
    };
    let dx = (other.x - point.x).abs() / 3.0;
    let delta_x = other.x - point.x;
    let slope = (other.y - point.y) / delta_x;
    let mut handle = ControlPoint {
        x: point.x + direction * dx,
        y: point.y + direction * slope * dx,
    };
    if clamp_y {
        handle.y = handle.y.clamp(0.0, 1.0);
    }
    Some(handle)
}

fn clamp_ordered_x(points: &[ControlPoint], selected: usize, target: f32) -> f32 {
    let minimum = if selected == 0 {
        0.0
    } else {
        points[selected - 1].x + MIN_X_GAP
    };
    let maximum = if selected + 1 == points.len() {
        1.0
    } else {
        points[selected + 1].x - MIN_X_GAP
    };
    if minimum <= maximum {
        target.clamp(minimum, maximum)
    } else {
        points[selected].x
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

fn hermite_derivative(
    left: f32,
    right: f32,
    width: f32,
    left_tangent: f32,
    right_tangent: f32,
    t: f32,
) -> f32 {
    let t2 = t * t;
    let dh00 = 6.0 * t2 - 6.0 * t;
    let dh10 = 3.0 * t2 - 4.0 * t + 1.0;
    let dh01 = -6.0 * t2 + 6.0 * t;
    let dh11 = 3.0 * t2 - 2.0 * t;
    (dh00 * left + dh10 * width * left_tangent + dh01 * right + dh11 * width * right_tangent)
        / width
}

fn cubic_value(left: f32, out: f32, incoming: f32, right: f32, t: f32) -> f32 {
    let one = 1.0 - t;
    one * one * one * left
        + 3.0 * one * one * t * out
        + 3.0 * one * t * t * incoming
        + t * t * t * right
}

fn cubic_derivative(left: f32, out: f32, incoming: f32, right: f32, t: f32) -> f32 {
    let one = 1.0 - t;
    3.0 * one * one * (out - left)
        + 6.0 * one * t * (incoming - out)
        + 3.0 * t * t * (right - incoming)
}

fn solve_bezier_parameter(x: f32, left: f32, out: f32, incoming: f32, right: f32) -> f32 {
    let mut low = 0.0;
    let mut high = 1.0;
    for _ in 0..28 {
        let middle = (low + high) * 0.5;
        let value = cubic_value(left, out, incoming, right, middle);
        if value < x {
            low = middle;
        } else {
            high = middle;
        }
    }
    (low + high) * 0.5
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CurveSet {
    pub linked: Curve,
    pub luminance: Curve,
    pub red: Curve,
    pub green: Curve,
    pub blue: Curve,
}

impl CurveSet {
    pub fn reset_mode(&mut self, mode: CurveMode) {
        match mode {
            CurveMode::LinkedRgb => self.linked = Curve::default(),
            CurveMode::Luminance => self.luminance = Curve::default(),
            CurveMode::PerChannelRgb => {
                self.red = Curve::default();
                self.green = Curve::default();
                self.blue = Curve::default();
            }
        }
    }

    pub fn curve(&self, mode: CurveMode, channel: CurveChannel) -> &Curve {
        match mode {
            CurveMode::LinkedRgb => &self.linked,
            CurveMode::Luminance => &self.luminance,
            CurveMode::PerChannelRgb => match channel {
                CurveChannel::Red => &self.red,
                CurveChannel::Green => &self.green,
                CurveChannel::Blue => &self.blue,
            },
        }
    }

    pub fn curve_mut(&mut self, mode: CurveMode, channel: CurveChannel) -> &mut Curve {
        match mode {
            CurveMode::LinkedRgb => &mut self.linked,
            CurveMode::Luminance => &mut self.luminance,
            CurveMode::PerChannelRgb => match channel {
                CurveChannel::Red => &mut self.red,
                CurveChannel::Green => &mut self.green,
                CurveChannel::Blue => &mut self.blue,
            },
        }
    }

    #[allow(dead_code)]
    pub fn apply(&self, mode: CurveMode, rgb: [f32; 3]) -> [f32; 3] {
        self.apply_with_luminance_and_interpolation(
            mode,
            rgb,
            LuminanceDefinition::Rec709,
            CurveInterpolation::Smooth,
        )
    }

    #[allow(dead_code)]
    pub fn apply_with_luminance(
        &self,
        mode: CurveMode,
        rgb: [f32; 3],
        luminance_definition: LuminanceDefinition,
    ) -> [f32; 3] {
        self.apply_with_luminance_and_interpolation(
            mode,
            rgb,
            luminance_definition,
            CurveInterpolation::Smooth,
        )
    }

    pub fn apply_with_luminance_and_interpolation(
        &self,
        mode: CurveMode,
        rgb: [f32; 3],
        luminance_definition: LuminanceDefinition,
        interpolation: CurveInterpolation,
    ) -> [f32; 3] {
        match mode {
            CurveMode::LinkedRgb => [
                self.linked
                    .evaluate_with_interpolation(rgb[0], interpolation),
                self.linked
                    .evaluate_with_interpolation(rgb[1], interpolation),
                self.linked
                    .evaluate_with_interpolation(rgb[2], interpolation),
            ],
            CurveMode::PerChannelRgb => [
                self.red.evaluate_with_interpolation(rgb[0], interpolation),
                self.green
                    .evaluate_with_interpolation(rgb[1], interpolation),
                self.blue.evaluate_with_interpolation(rgb[2], interpolation),
            ],
            CurveMode::Luminance => {
                let luminance = luma_with_definition(rgb, luminance_definition);
                let adjusted = self
                    .luminance
                    .evaluate_with_interpolation(luminance, interpolation);
                if luminance > 0.0 {
                    let scale = adjusted / luminance;
                    [rgb[0] * scale, rgb[1] * scale, rgb[2] * scale]
                } else {
                    [adjusted, adjusted, adjusted]
                }
            }
        }
    }
}

pub fn luma(rgb: [f32; 3]) -> f32 {
    luma_with_definition(rgb, LuminanceDefinition::AdobeRgb)
}

pub fn luma_with_definition(rgb: [f32; 3], definition: LuminanceDefinition) -> f32 {
    let value = match definition {
        LuminanceDefinition::AdobeRgb => {
            0.297_355 * rgb[0] + 0.627_372 * rgb[1] + 0.075_273 * rgb[2]
        }
        LuminanceDefinition::Rec709 => 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2],
        LuminanceDefinition::EqualEnergy => (rgb[0] + rgb[1] + rgb[2]) / 3.0,
    };
    value.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::{
        BezierHandleKind, CURVE_DOMAIN_LABEL, ControlPoint, Curve, CurveChannel,
        CurveInterpolation, CurveMode, CurveSet, DerivativeCurve, LuminanceDefinition,
    };

    #[test]
    fn identity_curve_is_identity_at_and_between_points() {
        let curve = Curve::identity();
        for interpolation in CurveInterpolation::ALL {
            for x in [
                f32::NEG_INFINITY,
                -0.01,
                0.0,
                0.03,
                0.25,
                0.41,
                0.75,
                1.0,
                1.01,
                f32::INFINITY,
                f32::NAN,
            ] {
                let expected = if x.is_finite() {
                    x.clamp(0.0, 1.0)
                } else {
                    0.0
                };
                assert!(
                    (curve.evaluate_with_interpolation(x, interpolation) - expected).abs() < 1e-4,
                    "{interpolation:?} x={x}"
                );
            }
        }
    }

    #[test]
    fn curve_construction_rejects_each_invalid_partition() {
        assert!(Curve::from_points(Vec::new()).is_none());
        for point in [
            ControlPoint { x: -0.001, y: 0.5 },
            ControlPoint { x: 0.5, y: -0.001 },
            ControlPoint { x: 1.001, y: 0.5 },
            ControlPoint { x: 0.5, y: 1.001 },
            ControlPoint {
                x: f32::NAN,
                y: 0.5,
            },
            ControlPoint {
                x: 0.5,
                y: f32::INFINITY,
            },
        ] {
            assert!(Curve::from_points(vec![ControlPoint { x: 0.0, y: 0.0 }, point]).is_none());
        }
        assert!(
            Curve::from_points(vec![
                ControlPoint { x: 0.0, y: 0.0 },
                ControlPoint { x: 0.5, y: 0.5 },
                ControlPoint { x: 0.5, y: 0.75 },
            ])
            .is_none()
        );
    }

    #[test]
    fn sampling_and_derivatives_have_defined_count_and_tail_boundaries() {
        let curve = Curve::identity();
        assert_eq!(
            curve
                .sample_with_interpolation(0, CurveInterpolation::Smooth)
                .len(),
            2
        );
        assert_eq!(
            curve
                .sample_with_interpolation(1, CurveInterpolation::Linear)
                .len(),
            2
        );
        assert_eq!(
            curve
                .sample_with_interpolation(5, CurveInterpolation::Bezier)
                .len(),
            5
        );
        for interpolation in CurveInterpolation::ALL {
            assert!(curve.derivative_at(-0.1, interpolation).abs() < f32::EPSILON);
            assert!(curve.derivative_at(1.1, interpolation).abs() < f32::EPSILON);
            assert!(curve.derivative_at(f32::NAN, interpolation).abs() < f32::EPSILON);
            assert!((curve.derivative_at(0.0, interpolation) - 1.0).abs() < 1.0e-4);
            assert!((curve.derivative_at(1.0, interpolation) - 1.0).abs() < 1.0e-4);
        }
    }

    #[test]
    fn non_finite_point_edits_are_rejected_without_breaking_order() {
        let mut curve = Curve::identity();
        assert!(!curve.insert_point(f32::NAN, 0.5));
        assert!(!curve.insert_point(0.5, f32::INFINITY));
        assert!(curve.points().windows(2).all(|pair| pair[0].x < pair[1].x));
    }

    #[test]
    fn non_finite_drag_targets_leave_the_curve_unchanged() {
        let curve = Curve::identity();
        assert_eq!(Curve::dragged_from(&curve, 2, f32::NAN, 0.8), curve);
        assert_eq!(Curve::dragged_from(&curve, 2, 0.5, f32::INFINITY), curve);
    }

    #[test]
    fn interpolation_choice_changes_the_curve_shape() {
        let curve = Curve::from_points(vec![
            ControlPoint { x: 0.0, y: 0.0 },
            ControlPoint { x: 0.3, y: 0.9 },
            ControlPoint { x: 1.0, y: 1.0 },
        ])
        .expect("valid curve");
        let smooth = curve.evaluate_with_interpolation(0.15, CurveInterpolation::Smooth);
        let linear = curve.evaluate_with_interpolation(0.15, CurveInterpolation::Linear);
        assert!((linear - 0.45).abs() < 1e-6);
        assert!((smooth - linear).abs() > 1e-3);
    }

    #[test]
    fn points_remain_ordered_and_endpoints_can_move_horizontally() {
        let curve = Curve::identity();
        let moved_first = Curve::dragged_from(&curve, 0, 0.18, 0.1);
        let moved_last = Curve::dragged_from(&curve, 4, 0.82, 0.9);
        assert!((moved_first.points()[0].x - 0.18).abs() < 1e-6);
        assert!((moved_last.points()[4].x - 0.82).abs() < 1e-6);
        assert!(
            moved_first
                .points()
                .windows(2)
                .all(|pair| pair[0].x < pair[1].x)
        );
        assert!(
            moved_last
                .points()
                .windows(2)
                .all(|pair| pair[0].x < pair[1].x)
        );
    }

    #[test]
    fn tightly_spaced_points_do_not_panic_when_dragged() {
        let curve = Curve::from_points(vec![
            ControlPoint { x: 0.0, y: 0.0 },
            ControlPoint { x: 0.004, y: 0.2 },
            ControlPoint { x: 0.008, y: 0.8 },
            ControlPoint { x: 1.0, y: 1.0 },
        ])
        .expect("valid curve");
        let moved = Curve::dragged_from(&curve, 1, 0.99, 0.5);
        assert!(moved.points().windows(2).all(|pair| pair[0].x < pair[1].x));
    }

    #[test]
    fn bezier_handles_are_independent_and_resettable() {
        let curve = Curve::identity();
        let outgoing = curve
            .handle(2, BezierHandleKind::Outgoing)
            .expect("interior outgoing handle");
        let incoming = curve
            .handle(2, BezierHandleKind::Incoming)
            .expect("interior incoming handle");
        let moved =
            Curve::dragged_handle_from(&curve, 2, BezierHandleKind::Outgoing, outgoing.x, 0.9);
        let moved_incoming = moved.handle(2, BezierHandleKind::Incoming).unwrap();
        let moved_outgoing = moved.handle(2, BezierHandleKind::Outgoing).unwrap();
        let anchor = curve.points()[2];
        let incoming_vector = (moved_incoming.x - anchor.x, moved_incoming.y - anchor.y);
        let outgoing_vector = (moved_outgoing.x - anchor.x, moved_outgoing.y - anchor.y);
        assert!(
            (incoming_vector.0 * outgoing_vector.1 - incoming_vector.1 * outgoing_vector.0).abs()
                < 1e-5
        );
        assert!(
            incoming_vector.0 * outgoing_vector.0 + incoming_vector.1 * outgoing_vector.1 < 0.0
        );
        let incoming_length =
            (incoming_vector.0 * incoming_vector.0 + incoming_vector.1 * incoming_vector.1).sqrt();
        let original_incoming_length = ((incoming.x - anchor.x) * (incoming.x - anchor.x)
            + (incoming.y - anchor.y) * (incoming.y - anchor.y))
            .sqrt();
        assert!((incoming_length - original_incoming_length).abs() < 1e-5);
        assert!(
            (moved.evaluate_with_interpolation(0.58, CurveInterpolation::Bezier) - 0.58).abs()
                > 1e-3
        );
        let mut reset = moved;
        assert!(reset.reset_handle(2, BezierHandleKind::Outgoing));
        assert_eq!(reset.handle(2, BezierHandleKind::Outgoing), Some(outgoing));
    }

    #[test]
    fn smooth_point_tension_is_adjustable_without_changing_the_anchor() {
        let mut curve = Curve::from_points(vec![
            ControlPoint { x: 0.0, y: 0.0 },
            ControlPoint { x: 0.4, y: 0.9 },
            ControlPoint { x: 1.0, y: 1.0 },
        ])
        .expect("valid curve");
        curve.set_tension(1, 2.5);
        assert!((curve.tension(1) - 2.5).abs() < 1e-6);
        assert_eq!(curve.points()[1], ControlPoint { x: 0.4, y: 0.9 });
        curve.set_tension(1, 0.0);
        assert!((curve.tension(1) - 0.1).abs() < 1.0e-6);
        curve.set_tension(1, 5.0);
        assert!((curve.tension(1) - 4.0).abs() < 1.0e-6);
        curve.set_tension(1, f32::NAN);
        assert!((curve.tension(1) - 4.0).abs() < 1.0e-6);
        assert!((curve.tension(99) - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn insertion_follows_the_existing_curve_and_endpoints_cannot_be_deleted() {
        let mut curve = Curve::identity();
        assert!(curve.insert_point_on_curve(0.62, CurveInterpolation::Bezier));
        let inserted = curve
            .points()
            .iter()
            .find(|point| (point.x - 0.62).abs() < 1e-6)
            .unwrap();
        assert!((inserted.y - 0.62).abs() < 1e-4);
        assert!(!curve.delete_point(0));
        assert!(!curve.delete_point(curve.points().len() - 1));
        assert!(curve.delete_point(3));
    }

    #[test]
    fn derivative_identity_is_horizontal_at_slope_one() {
        let curve = Curve::identity();
        let derivative = curve.derivative_curve(CurveInterpolation::Bezier);
        assert!(
            derivative
                .points()
                .iter()
                .all(|point| (point.y - 1.0).abs() < 1e-4)
        );
    }

    #[test]
    fn derivative_edits_modify_the_underlying_tone_curve() {
        let initial = Curve::identity();
        let initial_derivative = initial.derivative_curve(CurveInterpolation::Linear);
        let mut edited = initial_derivative.clone();
        edited.points[2].y = 2.0;
        let result = Curve::apply_derivative_edit(
            &initial,
            &initial_derivative,
            &edited,
            CurveInterpolation::Linear,
        );
        assert!((result.points()[2].y - 0.5).abs() > 1e-3);
    }

    #[test]
    fn low_derivative_point_does_not_create_unintended_cubic_overshoot() {
        let tone = Curve::identity();
        let derivative = tone.derivative_curve(CurveInterpolation::Smooth);
        let edited = DerivativeCurve::dragged_from(&derivative, 2, 0.5, -0.5);
        for pair in edited.points().windows(2) {
            for step in 0..=100 {
                let x = pair[0].x + (pair[1].x - pair[0].x) * step as f32 / 100.0;
                let value = edited.evaluate_with_interpolation(x, CurveInterpolation::Smooth);
                assert!(value >= pair[0].y.min(pair[1].y) - 1e-5);
                assert!(value <= pair[0].y.max(pair[1].y) + 1e-5);
            }
        }
    }

    #[test]
    fn luma_adjustment_preserves_channel_ratios_before_output_conversion() {
        let curves = CurveSet {
            luminance: Curve::from_points(vec![
                ControlPoint { x: 0.0, y: 0.0 },
                ControlPoint { x: 0.5, y: 0.9 },
                ControlPoint { x: 1.0, y: 1.0 },
            ])
            .unwrap(),
            ..CurveSet::default()
        };
        let input = [1.0, 0.5, 0.25];
        let output = curves.apply_with_luminance_and_interpolation(
            CurveMode::Luminance,
            input,
            LuminanceDefinition::AdobeRgb,
            CurveInterpolation::Linear,
        );
        assert!((output[0] / output[1] - 2.0).abs() < 1e-5);
        assert!((output[1] / output[2] - 2.0).abs() < 1e-5);
        assert!(
            output[0] > 1.0,
            "ratio preservation must precede gamut handling"
        );
    }

    #[test]
    fn derivative_point_updates_reject_non_finite_values() {
        let mut derivative = Curve::identity().derivative_curve(CurveInterpolation::Smooth);
        let before = derivative.points().to_vec();
        assert!(!derivative.set_point_y_near(0.5, f32::NAN, 1e-5));
        assert_eq!(derivative.points(), before);
    }

    #[test]
    fn bezier_segment_handles_remain_ordered_on_x() {
        let curve = Curve::identity();
        let moved_outgoing =
            Curve::dragged_handle_from(&curve, 1, BezierHandleKind::Outgoing, 0.49, 0.25);
        let moved_both =
            Curve::dragged_handle_from(&moved_outgoing, 2, BezierHandleKind::Incoming, 0.26, 0.5);
        let outgoing = moved_both.handle(1, BezierHandleKind::Outgoing).unwrap();
        let incoming = moved_both.handle(2, BezierHandleKind::Incoming).unwrap();
        assert!(outgoing.x <= incoming.x);
    }

    #[test]
    fn curve_domain_contract_names_adobe_rgb() {
        assert_eq!(CURVE_DOMAIN_LABEL, "canonical encoded Adobe RGB (1998)");
        for mode in CurveMode::ALL {
            assert!(!mode.label().is_empty());
            assert!(!mode.description().is_empty());
        }
        for channel in CurveChannel::ALL {
            assert!(!channel.label().is_empty());
        }
        for luminance in LuminanceDefinition::ALL {
            assert!(!luminance.label().is_empty());
        }
        for interpolation in CurveInterpolation::ALL {
            assert!(!interpolation.label().is_empty());
            assert!(!interpolation.description().is_empty());
        }
    }

    #[test]
    fn adobe_rgb_luma_uses_the_project_coefficients() {
        let red = super::luma_with_definition([1.0, 0.0, 0.0], LuminanceDefinition::AdobeRgb);
        let green = super::luma_with_definition([0.0, 1.0, 0.0], LuminanceDefinition::AdobeRgb);
        let blue = super::luma_with_definition([0.0, 0.0, 1.0], LuminanceDefinition::AdobeRgb);
        assert!((red - 0.297_355).abs() < 1e-6);
        assert!((green - 0.627_372).abs() < 1e-6);
        assert!((blue - 0.075_273).abs() < 1e-6);
        for definition in LuminanceDefinition::ALL {
            assert!(super::luma_with_definition([0.0, 0.0, 0.0], definition).abs() < f32::EPSILON);
            assert!(
                (super::luma_with_definition([1.0, 1.0, 1.0], definition) - 1.0).abs()
                    < f32::EPSILON
            );
            assert!(
                (0.0..=1.0).contains(&super::luma_with_definition([-1.0, 2.0, 0.0], definition))
            );
        }
    }

    #[test]
    fn luminance_identity_preserves_a_very_dark_saturated_pixel() {
        let curves = CurveSet::default();
        let rgb = [0.0001, 0.0, 0.0];
        let output = curves.apply_with_luminance_and_interpolation(
            CurveMode::Luminance,
            rgb,
            LuminanceDefinition::Rec709,
            CurveInterpolation::Smooth,
        );
        assert!((output[0] - rgb[0]).abs() < 1e-6);
        assert!(output[1] < 1e-8 && output[2] < 1e-8);
    }

    #[test]
    fn mode_application_keeps_linked_channels_independent() {
        let linked = Curve::from_points(vec![
            ControlPoint { x: 0.0, y: 0.0 },
            ControlPoint { x: 1.0, y: 0.5 },
        ])
        .expect("valid curve");
        let curves = CurveSet {
            linked,
            ..CurveSet::default()
        };
        let output = curves.apply(CurveMode::LinkedRgb, [0.2, 0.4, 0.8]);
        assert!(output[0] < output[1]);
        assert!(output[1] < output[2]);
        assert_eq!(CurveChannel::Blue.label(), "Blue");
    }
}

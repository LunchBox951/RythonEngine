//! Tagged-enum action values produced by input bindings.
//!
//! Hardware samples and modifier pipelines run on `[f32; 3]`, then narrow to
//! the `ValueKind` of the owning action at the output of a binding.

/// The kind (dimensionality) declared by an `InputAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueKind {
    Button,
    Axis1D,
    Axis2D,
    Axis3D,
}

/// A single input value produced (or consumed) by an action.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActionValue {
    Button(bool),
    Axis1D(f32),
    Axis2D([f32; 2]),
    Axis3D([f32; 3]),
}

impl ActionValue {
    pub fn kind(&self) -> ValueKind {
        match self {
            Self::Button(_) => ValueKind::Button,
            Self::Axis1D(_) => ValueKind::Axis1D,
            Self::Axis2D(_) => ValueKind::Axis2D,
            Self::Axis3D(_) => ValueKind::Axis3D,
        }
    }

    /// Widen the value into the internal `[f32; 3]` modifier-pipeline representation.
    pub fn as_axis3d(&self) -> [f32; 3] {
        match *self {
            Self::Button(b) => [if b { 1.0 } else { 0.0 }, 0.0, 0.0],
            Self::Axis1D(x) => [x, 0.0, 0.0],
            Self::Axis2D([x, y]) => [x, y, 0.0],
            Self::Axis3D(v) => v,
        }
    }

    /// Narrow a `[f32; 3]` pipeline result into the target `ValueKind`.
    ///
    /// Threshold for `Button` tripping is `abs(x) > BUTTON_THRESHOLD` (the
    /// standard trigger-style actuation point).
    pub fn from_axis3d(kind: ValueKind, v: [f32; 3]) -> Self {
        match kind {
            ValueKind::Button => Self::Button(v[0].abs() > BUTTON_THRESHOLD),
            ValueKind::Axis1D => Self::Axis1D(v[0]),
            ValueKind::Axis2D => Self::Axis2D([v[0], v[1]]),
            ValueKind::Axis3D => Self::Axis3D(v),
        }
    }

    /// The default zero value for the given kind.
    pub fn zero(kind: ValueKind) -> Self {
        match kind {
            ValueKind::Button => Self::Button(false),
            ValueKind::Axis1D => Self::Axis1D(0.0),
            ValueKind::Axis2D => Self::Axis2D([0.0, 0.0]),
            ValueKind::Axis3D => Self::Axis3D([0.0, 0.0, 0.0]),
        }
    }

    /// Whether this value is actuated past the default threshold. Used for
    /// consumption decisions between priority-layered contexts.
    pub fn is_actuated(&self) -> bool {
        self.is_actuated_with(BUTTON_THRESHOLD)
    }

    /// Whether this value is actuated past an explicit threshold (axis magnitude).
    pub fn is_actuated_with(&self, threshold: f32) -> bool {
        match *self {
            Self::Button(b) => b,
            Self::Axis1D(x) => x.abs() > threshold,
            Self::Axis2D([x, y]) => (x * x + y * y).sqrt() > threshold,
            Self::Axis3D([x, y, z]) => (x * x + y * y + z * z).sqrt() > threshold,
        }
    }

    /// Accumulate another sample of the same kind into this one.
    ///
    /// Rule: max-abs per axis for axis kinds, logical-OR for buttons.
    /// Used when a single action has multiple bindings active simultaneously.
    pub fn accumulate(&mut self, other: Self) {
        match (self, other) {
            (Self::Button(a), Self::Button(b)) => *a = *a || b,
            (Self::Axis1D(a), Self::Axis1D(b)) => {
                if b.abs() > a.abs() {
                    *a = b;
                }
            }
            (Self::Axis2D(a), Self::Axis2D(b)) => {
                if b[0].abs() > a[0].abs() {
                    a[0] = b[0];
                }
                if b[1].abs() > a[1].abs() {
                    a[1] = b[1];
                }
            }
            (Self::Axis3D(a), Self::Axis3D(b)) => {
                for i in 0..3 {
                    if b[i].abs() > a[i].abs() {
                        a[i] = b[i];
                    }
                }
            }
            // Kind mismatches shouldn't occur in well-formed data; silently keep the accumulator.
            _ => {}
        }
    }
}

/// Default actuation threshold for button-kind tripping and is_actuated checks.
pub const BUTTON_THRESHOLD: f32 = 0.5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_roundtrip() {
        for k in [
            ValueKind::Button,
            ValueKind::Axis1D,
            ValueKind::Axis2D,
            ValueKind::Axis3D,
        ] {
            assert_eq!(ActionValue::zero(k).kind(), k);
        }
    }

    #[test]
    fn as_axis3d_button() {
        assert_eq!(ActionValue::Button(true).as_axis3d(), [1.0, 0.0, 0.0]);
        assert_eq!(ActionValue::Button(false).as_axis3d(), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn as_axis3d_widens_lower_dims() {
        assert_eq!(ActionValue::Axis1D(0.7).as_axis3d(), [0.7, 0.0, 0.0]);
        assert_eq!(ActionValue::Axis2D([1.0, -0.5]).as_axis3d(), [1.0, -0.5, 0.0]);
        assert_eq!(
            ActionValue::Axis3D([0.1, 0.2, 0.3]).as_axis3d(),
            [0.1, 0.2, 0.3]
        );
    }

    #[test]
    fn from_axis3d_narrows() {
        assert_eq!(
            ActionValue::from_axis3d(ValueKind::Button, [0.9, 0.0, 0.0]),
            ActionValue::Button(true)
        );
        assert_eq!(
            ActionValue::from_axis3d(ValueKind::Button, [0.1, 0.0, 0.0]),
            ActionValue::Button(false)
        );
        assert_eq!(
            ActionValue::from_axis3d(ValueKind::Axis1D, [0.3, 0.8, 0.9]),
            ActionValue::Axis1D(0.3)
        );
        assert_eq!(
            ActionValue::from_axis3d(ValueKind::Axis2D, [0.3, 0.8, 0.9]),
            ActionValue::Axis2D([0.3, 0.8])
        );
        assert_eq!(
            ActionValue::from_axis3d(ValueKind::Axis3D, [0.3, 0.8, 0.9]),
            ActionValue::Axis3D([0.3, 0.8, 0.9])
        );
    }

    #[test]
    fn is_actuated_button() {
        assert!(ActionValue::Button(true).is_actuated());
        assert!(!ActionValue::Button(false).is_actuated());
    }

    #[test]
    fn is_actuated_axis1d() {
        assert!(!ActionValue::Axis1D(0.4).is_actuated());
        assert!(ActionValue::Axis1D(0.6).is_actuated());
        assert!(ActionValue::Axis1D(-0.9).is_actuated());
    }

    #[test]
    fn is_actuated_axis2d_uses_magnitude() {
        // magnitude(0.4, 0.4) = ~0.566 > 0.5
        assert!(ActionValue::Axis2D([0.4, 0.4]).is_actuated());
        // magnitude(0.3, 0.3) = ~0.424 < 0.5
        assert!(!ActionValue::Axis2D([0.3, 0.3]).is_actuated());
    }

    #[test]
    fn accumulate_buttons_or() {
        let mut a = ActionValue::Button(false);
        a.accumulate(ActionValue::Button(true));
        assert_eq!(a, ActionValue::Button(true));
    }

    #[test]
    fn accumulate_axis1d_max_abs() {
        let mut a = ActionValue::Axis1D(0.3);
        a.accumulate(ActionValue::Axis1D(-0.7));
        assert_eq!(a, ActionValue::Axis1D(-0.7));
        a.accumulate(ActionValue::Axis1D(0.5));
        assert_eq!(a, ActionValue::Axis1D(-0.7));
    }

    #[test]
    fn accumulate_axis2d_per_axis_max_abs() {
        let mut a = ActionValue::Axis2D([0.2, -0.9]);
        a.accumulate(ActionValue::Axis2D([-0.8, 0.1]));
        assert_eq!(a, ActionValue::Axis2D([-0.8, -0.9]));
    }
}

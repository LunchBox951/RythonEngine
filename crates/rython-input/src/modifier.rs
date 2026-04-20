//! Per-binding modifiers applied to raw hardware samples before they reach
//! the action accumulator.
//!
//! Modifiers operate on the internal `[f32; 3]` pipeline form. Narrowing to
//! the owning action's `ValueKind` happens downstream in `value.rs`.

/// Axis-reorder spec for the `Swizzle` modifier. Lists the source axis
/// (0 = X, 1 = Y, 2 = Z) for each output component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwizzleOrder {
    pub x_from: u8,
    pub y_from: u8,
    pub z_from: u8,
}

impl SwizzleOrder {
    pub const XYZ: Self = Self {
        x_from: 0,
        y_from: 1,
        z_from: 2,
    };
    pub const YXZ: Self = Self {
        x_from: 1,
        y_from: 0,
        z_from: 2,
    };
    pub const ZXY: Self = Self {
        x_from: 2,
        y_from: 0,
        z_from: 1,
    };
    pub const YZX: Self = Self {
        x_from: 1,
        y_from: 2,
        z_from: 0,
    };

    pub fn from_str(order: &str) -> Option<Self> {
        match order.to_ascii_uppercase().as_str() {
            "XYZ" => Some(Self::XYZ),
            "YXZ" => Some(Self::YXZ),
            "ZXY" => Some(Self::ZXY),
            "YZX" => Some(Self::YZX),
            _ => None,
        }
    }

    fn apply(&self, v: [f32; 3]) -> [f32; 3] {
        [
            v[self.x_from as usize],
            v[self.y_from as usize],
            v[self.z_from as usize],
        ]
    }
}

/// A stateless per-binding sample transformation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Modifier {
    /// Multiply each component by -1 if the corresponding flag is set.
    Negate { x: bool, y: bool, z: bool },

    /// Multiply each component by a constant.
    Scale([f32; 3]),

    /// Deadzone. When `radial` is false, each axis is independently rescaled
    /// so that `[lower, upper]` maps to `[0, 1]` (sign-preserving). When
    /// `radial` is true, the magnitude of `[x, y, z]` is rescaled instead
    /// (appropriate for analog sticks).
    DeadZone {
        lower: f32,
        upper: f32,
        radial: bool,
    },

    /// Permute the output components. See `SwizzleOrder`.
    Swizzle(SwizzleOrder),
}

impl Modifier {
    pub fn apply(&self, v: [f32; 3]) -> [f32; 3] {
        match *self {
            Self::Negate { x, y, z } => [
                if x { -v[0] } else { v[0] },
                if y { -v[1] } else { v[1] },
                if z { -v[2] } else { v[2] },
            ],
            Self::Scale(s) => [v[0] * s[0], v[1] * s[1], v[2] * s[2]],
            Self::DeadZone {
                lower,
                upper,
                radial,
            } => {
                if radial {
                    apply_radial_deadzone(v, lower, upper)
                } else {
                    [
                        apply_axial_deadzone(v[0], lower, upper),
                        apply_axial_deadzone(v[1], lower, upper),
                        apply_axial_deadzone(v[2], lower, upper),
                    ]
                }
            }
            Self::Swizzle(order) => order.apply(v),
        }
    }
}

fn apply_axial_deadzone(x: f32, lower: f32, upper: f32) -> f32 {
    let mag = x.abs();
    if mag <= lower {
        return 0.0;
    }
    let sign = x.signum();
    let range = (upper - lower).max(f32::EPSILON);
    let scaled = ((mag - lower) / range).clamp(0.0, 1.0);
    sign * scaled
}

fn apply_radial_deadzone(v: [f32; 3], lower: f32, upper: f32) -> [f32; 3] {
    let mag_sq = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    let mag = mag_sq.sqrt();
    if mag <= lower {
        return [0.0, 0.0, 0.0];
    }
    let range = (upper - lower).max(f32::EPSILON);
    let scaled = ((mag - lower) / range).clamp(0.0, 1.0);
    let factor = scaled / mag;
    [v[0] * factor, v[1] * factor, v[2] * factor]
}

/// Run a sequence of modifiers in declaration order.
pub fn apply_pipeline(modifiers: &[Modifier], v: [f32; 3]) -> [f32; 3] {
    modifiers.iter().fold(v, |acc, m| m.apply(acc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negate_flips_selected_axes() {
        let m = Modifier::Negate {
            x: true,
            y: false,
            z: true,
        };
        assert_eq!(m.apply([0.5, 0.5, 0.5]), [-0.5, 0.5, -0.5]);
    }

    #[test]
    fn scale_multiplies_per_axis() {
        let m = Modifier::Scale([2.0, 0.5, -1.0]);
        assert_eq!(m.apply([1.0, 2.0, 3.0]), [2.0, 1.0, -3.0]);
    }

    #[test]
    fn axial_deadzone_zeroes_small() {
        let m = Modifier::DeadZone {
            lower: 0.2,
            upper: 1.0,
            radial: false,
        };
        assert_eq!(m.apply([0.1, 0.0, 0.0])[0], 0.0);
        assert_eq!(m.apply([-0.1, 0.0, 0.0])[0], 0.0);
    }

    #[test]
    fn axial_deadzone_rescales_above() {
        let m = Modifier::DeadZone {
            lower: 0.2,
            upper: 1.0,
            radial: false,
        };
        // At lower bound: 0 out.
        let low = m.apply([0.2, 0.0, 0.0])[0];
        assert!(low.abs() < 1e-5);
        // At upper bound: 1 out.
        let hi = m.apply([1.0, 0.0, 0.0])[0];
        assert!((hi - 1.0).abs() < 1e-5);
        // Midpoint maps through the linear rescale.
        let mid = m.apply([0.6, 0.0, 0.0])[0];
        assert!((mid - 0.5).abs() < 1e-5);
        // Sign preserved.
        let neg = m.apply([-0.6, 0.0, 0.0])[0];
        assert!((neg - -0.5).abs() < 1e-5);
    }

    #[test]
    fn radial_deadzone_zeroes_inside_circle() {
        let m = Modifier::DeadZone {
            lower: 0.3,
            upper: 1.0,
            radial: true,
        };
        // Inside the radial deadzone — each component inside, magnitude inside.
        let out = m.apply([0.2, 0.1, 0.0]);
        assert_eq!(out, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn radial_deadzone_rescales_outside_circle() {
        let m = Modifier::DeadZone {
            lower: 0.25,
            upper: 1.0,
            radial: true,
        };
        // Unit-length vector should stay unit-length (upper bound → 1.0).
        let out = m.apply([1.0, 0.0, 0.0]);
        assert!((out[0] - 1.0).abs() < 1e-5);
        assert!(out[1].abs() < 1e-5);
    }

    #[test]
    fn swizzle_yxz() {
        let m = Modifier::Swizzle(SwizzleOrder::YXZ);
        assert_eq!(m.apply([1.0, 2.0, 3.0]), [2.0, 1.0, 3.0]);
    }

    #[test]
    fn swizzle_parse() {
        assert_eq!(SwizzleOrder::from_str("xyz"), Some(SwizzleOrder::XYZ));
        assert_eq!(SwizzleOrder::from_str("YXZ"), Some(SwizzleOrder::YXZ));
        assert_eq!(SwizzleOrder::from_str("XXY"), None);
    }

    #[test]
    fn pipeline_applies_in_order() {
        // Start with 0.6 on X; scale by 2, then negate X -> -1.2.
        let pipeline = [
            Modifier::Scale([2.0, 1.0, 1.0]),
            Modifier::Negate {
                x: true,
                y: false,
                z: false,
            },
        ];
        assert_eq!(apply_pipeline(&pipeline, [0.6, 0.0, 0.0]), [-1.2, 0.0, 0.0]);
    }

    #[test]
    fn pipeline_empty_passthrough() {
        assert_eq!(apply_pipeline(&[], [0.1, 0.2, 0.3]), [0.1, 0.2, 0.3]);
    }
}

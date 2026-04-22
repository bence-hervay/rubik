use core::fmt;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Axis {
    /// Left-to-right axis. `depth = 0` is the L slice, `depth = n - 1` is R.
    X,
    /// Down-to-up axis. `depth = 0` is the D slice, `depth = n - 1` is U.
    Y,
    /// Back-to-front axis. `depth = 0` is the B slice, `depth = n - 1` is F.
    Z,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum MoveAngle {
    Positive = 1,
    Double = 2,
    Negative = 3,
}

impl MoveAngle {
    pub const ALL: [Self; 3] = [Self::Positive, Self::Double, Self::Negative];

    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    pub const fn quarter_turns(self) -> u8 {
        self.as_u8()
    }

    pub const fn inverse(self) -> Self {
        match self {
            Self::Positive => Self::Negative,
            Self::Double => Self::Double,
            Self::Negative => Self::Positive,
        }
    }
}

impl fmt::Display for MoveAngle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Positive => write!(f, "positive"),
            Self::Double => write!(f, "double"),
            Self::Negative => write!(f, "negative"),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Move {
    pub axis: Axis,
    pub depth: usize,
    pub angle: MoveAngle,
}

impl Move {
    pub const fn new(axis: Axis, depth: usize, angle: MoveAngle) -> Self {
        Self { axis, depth, angle }
    }

    pub const fn inverse(self) -> Self {
        Self {
            axis: self.axis,
            depth: self.depth,
            angle: self.angle.inverse(),
        }
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}[{}] {}", self.axis, self.depth, self.angle)
    }
}

#[cfg(test)]
mod tests {
    use super::MoveAngle;

    #[test]
    fn move_angles_have_fixed_integer_assignments() {
        assert_eq!(MoveAngle::Positive.as_u8(), 1);
        assert_eq!(MoveAngle::Double.as_u8(), 2);
        assert_eq!(MoveAngle::Negative.as_u8(), 3);
    }

    #[test]
    fn move_angle_inverse_flips_quarter_turns() {
        assert_eq!(MoveAngle::Positive.inverse(), MoveAngle::Negative);
        assert_eq!(MoveAngle::Double.inverse(), MoveAngle::Double);
        assert_eq!(MoveAngle::Negative.inverse(), MoveAngle::Positive);
    }
}

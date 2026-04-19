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

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Angle {
    /// Positive quarter turn around the selected axis.
    Positive,
    /// Negative quarter turn around the selected axis.
    Negative,
    /// Double turn around the selected axis.
    Double,
}

impl Angle {
    pub const fn quarter_turns(self) -> u8 {
        match self {
            Self::Positive => 1,
            Self::Negative => 3,
            Self::Double => 2,
        }
    }

    pub const fn inverse(self) -> Self {
        match self {
            Self::Positive => Self::Negative,
            Self::Negative => Self::Positive,
            Self::Double => Self::Double,
        }
    }
}

impl fmt::Display for Angle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Positive => write!(f, "positive"),
            Self::Negative => write!(f, "negative"),
            Self::Double => write!(f, "double"),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Move {
    pub axis: Axis,
    pub depth: usize,
    pub angle: Angle,
}

impl Move {
    pub const fn new(axis: Axis, depth: usize, angle: Angle) -> Self {
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

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
pub enum TurnAmount {
    /// Clockwise when looking from the positive end of the axis toward the cube.
    Cw,
    Half,
    /// Counter-clockwise when looking from the positive end of the axis toward the cube.
    Ccw,
}

impl TurnAmount {
    pub const fn quarter_turns(self) -> u8 {
        match self {
            Self::Cw => 1,
            Self::Half => 2,
            Self::Ccw => 3,
        }
    }

    pub const fn inverse(self) -> Self {
        match self {
            Self::Cw => Self::Ccw,
            Self::Half => Self::Half,
            Self::Ccw => Self::Cw,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Move {
    pub axis: Axis,
    pub depth: usize,
    pub amount: TurnAmount,
}

impl Move {
    pub const fn new(axis: Axis, depth: usize, amount: TurnAmount) -> Self {
        Self {
            axis,
            depth,
            amount,
        }
    }

    pub const fn inverse(self) -> Self {
        Self {
            axis: self.axis,
            depth: self.depth,
            amount: self.amount.inverse(),
        }
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}[{}] {:?}", self.axis, self.depth, self.amount)
    }
}

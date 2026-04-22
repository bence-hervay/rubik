use core::fmt;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ExecutionMode {
    Standard,
    Optimized,
}

impl ExecutionMode {
    pub const fn records_moves(self) -> bool {
        match self {
            Self::Standard => true,
            Self::Optimized => false,
        }
    }
}

impl fmt::Display for ExecutionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Standard => f.write_str("standard"),
            Self::Optimized => f.write_str("optimized"),
        }
    }
}

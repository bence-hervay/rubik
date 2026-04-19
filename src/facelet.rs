use core::fmt;

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Default)]
pub enum Facelet {
    #[default]
    White = 0,
    Yellow = 1,
    Red = 2,
    Orange = 3,
    Blue = 4,
    Green = 5,
}

impl Facelet {
    pub const ALL: [Self; 6] = [
        Self::White,
        Self::Yellow,
        Self::Red,
        Self::Orange,
        Self::Blue,
        Self::Green,
    ];

    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::White,
            1 => Self::Yellow,
            2 => Self::Red,
            3 => Self::Orange,
            4 => Self::Blue,
            5 => Self::Green,
            _ => panic!("invalid facelet value {value}"),
        }
    }

    pub const fn as_char(self) -> char {
        match self {
            Self::White => 'W',
            Self::Yellow => 'Y',
            Self::Red => 'R',
            Self::Orange => 'O',
            Self::Blue => 'B',
            Self::Green => 'G',
        }
    }
}

impl fmt::Display for Facelet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_char())
    }
}

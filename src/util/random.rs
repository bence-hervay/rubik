pub trait RandomSource {
    fn next_u64(&mut self) -> u64;
}

#[derive(Copy, Clone, Debug)]
pub struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    pub const fn new(seed: u64) -> Self {
        let state = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
        Self { state }
    }
}

impl RandomSource for XorShift64 {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}

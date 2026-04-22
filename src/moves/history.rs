use crate::moves::Move;

#[derive(Clone, Debug, Default)]
pub struct MoveHistory {
    moves: Vec<Move>,
}

impl MoveHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.moves.len()
    }

    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    pub fn clear(&mut self) {
        self.moves.clear();
    }

    pub fn push(&mut self, mv: Move) {
        self.moves.push(mv);
    }

    pub fn pop(&mut self) -> Option<Move> {
        self.moves.pop()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Move> {
        self.moves.iter()
    }

    pub fn as_slice(&self) -> &[Move] {
        &self.moves
    }
}

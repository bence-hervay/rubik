use crate::{
    conventions::face_layer_move,
    cube::Cube,
    face::FaceId,
    moves::{Move, MoveAngle},
    simulation::derived::{
        edge_cubie_index, edge_cubie_location, edge_cubie_sets_match, facelet_location,
        facelet_locations_are_unique, trace_position, unique_edge_cubies, FacePosition,
        trace_facelet_location_through_moves, EdgeCubieLocation, FaceletUpdate,
    },
    storage::FaceletArray,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum EdgeThreeCycleValidationError {
    WingCycleRequiresSideLengthAtLeastFour,
    RowMustBeInnerLayer,
    WingRowCannotBeOddMiddleLayer,
    MiddleCycleRequiresOddSideLengthAtLeastThree,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct EdgeThreeCycle {
    kind: EdgeThreeCycleKind,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum EdgeThreeCycleKind {
    FrontRightWing { row: usize },
    FrontRightMiddle { direction: EdgeThreeCycleDirection },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum EdgeThreeCycleDirection {
    Positive,
    Negative,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EdgeThreeCyclePlan {
    side_length: usize,
    cycle: Option<EdgeThreeCycle>,
    moves: Vec<Move>,
    cubies: [EdgeCubieLocation; 3],
    updates: [FaceletUpdate; 6],
}
impl EdgeThreeCycle {
    /// A sparse big-cube wing three-cycle in the canonical front/right working
    /// area.
    ///
    /// `row` is an inner edge row on the front-left/front-right edge band. On
    /// odd cubes the exact middle row is not a wing row and is rejected when a
    /// plan is built.
    pub const fn front_right_wing(row: usize) -> Self {
        Self {
            kind: EdgeThreeCycleKind::FrontRightWing { row },
        }
    }

    /// An exact middle-edge 3-cycle in the canonical front/right working area.
    pub const fn front_right_middle(direction: EdgeThreeCycleDirection) -> Self {
        Self {
            kind: EdgeThreeCycleKind::FrontRightMiddle { direction },
        }
    }

    pub const fn kind(self) -> EdgeThreeCycleKind {
        self.kind
    }

    pub const fn row(self) -> Option<usize> {
        match self.kind {
            EdgeThreeCycleKind::FrontRightWing { row } => Some(row),
            EdgeThreeCycleKind::FrontRightMiddle { .. } => None,
        }
    }

    pub fn moves(self, side_length: usize) -> Vec<Move> {
        edge_three_cycle_moves(side_length, self)
    }
}

impl EdgeThreeCycleDirection {
    pub const ALL: [Self; 2] = [Self::Positive, Self::Negative];

    pub const fn inverse(self) -> Self {
        match self {
            Self::Positive => Self::Negative,
            Self::Negative => Self::Positive,
        }
    }
}

impl EdgeThreeCycle {
    pub fn try_validate(self, side_length: usize) -> Result<(), EdgeThreeCycleValidationError> {
        try_validate_edge_three_cycle(side_length, self)
    }
}

impl EdgeThreeCyclePlan {
    /// Builds the sparse plan for a named edge three-cycle recipe.
    ///
    /// This path scans only edge stickers, so it is suitable for precomputing
    /// plans on very large cubes.
    pub fn from_cycle(side_length: usize, cycle: EdgeThreeCycle) -> Self {
        Self::try_from_cycle(side_length, cycle).unwrap_or_else(|error| panic!("{error}"))
    }

    pub fn try_from_cycle(
        side_length: usize,
        cycle: EdgeThreeCycle,
    ) -> Result<Self, EdgeThreeCycleValidationError> {
        try_validate_edge_three_cycle(side_length, cycle)?;
        Ok(try_edge_three_cycle_plan_from_edge_positions(
            side_length,
            Some(cycle),
            cycle.moves(side_length),
        )
        .expect("edge three-cycle recipe must produce exactly one edge cubie 3-cycle"))
    }

    /// Builds a plan from an arbitrary literal move sequence, accepting it only
    /// if the full cube permutation is exactly one edge-cubie 3-cycle.
    pub fn from_moves(side_length: usize, moves: Vec<Move>) -> Self {
        Self::from_moves_for_cycle(side_length, None, moves)
    }

    fn from_moves_for_cycle(
        side_length: usize,
        cycle: Option<EdgeThreeCycle>,
        moves: Vec<Move>,
    ) -> Self {
        try_edge_three_cycle_plan_from_moves(side_length, cycle, moves)
            .expect("move sequence must be exactly one edge cubie 3-cycle")
    }

    pub fn try_from_moves(side_length: usize, moves: Vec<Move>) -> Option<Self> {
        try_edge_three_cycle_plan_from_moves(side_length, None, moves)
    }

    pub fn side_length(&self) -> usize {
        self.side_length
    }

    pub fn cycle(&self) -> Option<EdgeThreeCycle> {
        self.cycle
    }

    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    pub fn cubies(&self) -> &[EdgeCubieLocation; 3] {
        &self.cubies
    }

    pub fn updates(&self) -> &[FaceletUpdate; 6] {
        &self.updates
    }

    pub fn is_valid(&self) -> bool {
        if self
            .cycle
            .is_some_and(|cycle| try_validate_edge_three_cycle(self.side_length, cycle).is_err())
        {
            return false;
        }

        build_edge_three_cycle_plan(
            self.side_length,
            self.cycle,
            self.moves.clone(),
            self.updates.into_iter().collect(),
        )
        .is_some()
    }

    pub(crate) fn inverted(&self) -> Self {
        let updates = self
            .updates
            .map(|update| FaceletUpdate {
                from: update.to,
                to: update.from,
            })
            .into_iter()
            .collect();

        let moves = self
            .moves
            .iter()
            .rev()
            .copied()
            .map(Move::inverse)
            .collect();

        build_edge_three_cycle_plan(self.side_length, None, moves, updates)
            .expect("inverse of an exact edge three-cycle must stay exact")
    }

    pub(crate) fn conjugated_by_moves(&self, setup_moves: &[Move]) -> Self {
        if setup_moves.is_empty() {
            return self.clone();
        }

        let updates = self
            .updates
            .map(|update| FaceletUpdate {
                from: trace_facelet_location_through_moves(
                    self.side_length,
                    update.from,
                    setup_moves,
                ),
                to: trace_facelet_location_through_moves(self.side_length, update.to, setup_moves),
            })
            .into_iter()
            .collect();

        let mut moves = Vec::with_capacity(setup_moves.len() * 2 + self.moves.len());
        moves.extend(setup_moves.iter().rev().copied().map(Move::inverse));
        moves.extend(self.moves.iter().copied());
        moves.extend(setup_moves.iter().copied());

        build_edge_three_cycle_plan(self.side_length, None, moves, updates)
            .expect("conjugating an exact edge three-cycle must stay exact")
    }
}

impl<S: FaceletArray> Cube<S> {
    pub fn edge_three_cycle_moves(&self, cycle: EdgeThreeCycle) -> Vec<Move> {
        validate_edge_three_cycle(self.n, cycle);
        cycle.moves(self.n)
    }

    /// Precomputes the six sparse sticker updates for a named edge three-cycle.
    pub fn edge_three_cycle_plan(&self, cycle: EdgeThreeCycle) -> EdgeThreeCyclePlan {
        EdgeThreeCyclePlan::from_cycle(self.n, cycle)
    }

    /// Attempts to prove that a literal move sequence is exactly one edge
    /// cubie 3-cycle, then returns its sparse update plan.
    pub fn try_edge_three_cycle_plan_from_moves(
        &self,
        moves: Vec<Move>,
    ) -> Option<EdgeThreeCyclePlan> {
        EdgeThreeCyclePlan::try_from_moves(self.n, moves)
    }

    pub fn edge_three_cycle_plan_from_moves(&self, moves: Vec<Move>) -> EdgeThreeCyclePlan {
        EdgeThreeCyclePlan::from_moves(self.n, moves)
    }

    /// Convenience wrapper that builds then applies a named edge three-cycle.
    /// Reuse `EdgeThreeCyclePlan` directly in hot solving loops.
    pub fn apply_edge_three_cycle_untracked(&mut self, cycle: EdgeThreeCycle) {
        let plan = self.edge_three_cycle_plan(cycle);
        self.apply_edge_three_cycle_plan_untracked(&plan);
    }

    pub fn apply_edge_three_cycle_literal_untracked(&mut self, cycle: EdgeThreeCycle) {
        let plan = self.edge_three_cycle_plan(cycle);
        self.apply_edge_three_cycle_plan_literal_untracked(&plan);
    }

    pub fn apply_edge_three_cycle_plan_literal_untracked(&mut self, plan: &EdgeThreeCyclePlan) {
        debug_assert_eq!(
            plan.side_length, self.n,
            "edge three-cycle plan side length must match the cube"
        );
        self.apply_moves_untracked(plan.moves().iter().copied());
    }

    /// Applies a precomputed edge three-cycle plan with delayed reads, so the
    /// cyclic overwrite order cannot corrupt source stickers.
    pub fn apply_edge_three_cycle_plan_untracked(&mut self, plan: &EdgeThreeCyclePlan) {
        debug_assert_eq!(
            plan.side_length, self.n,
            "edge three-cycle plan side length must match the cube"
        );

        let values = plan.updates.map(|update| {
            self.position(FacePosition {
                face: update.from.face,
                row: update.from.row,
                col: update.from.col,
            })
        });

        for (update, value) in plan.updates.iter().copied().zip(values) {
            self.set_position(
                FacePosition {
                    face: update.to.face,
                    row: update.to.row,
                    col: update.to.col,
                },
                value,
            );
        }
    }
}

fn validate_edge_three_cycle(n: usize, cycle: EdgeThreeCycle) {
    try_validate_edge_three_cycle(n, cycle).unwrap_or_else(|error| panic!("{error}"));
}

fn try_validate_edge_three_cycle(
    n: usize,
    cycle: EdgeThreeCycle,
) -> Result<(), EdgeThreeCycleValidationError> {
    match cycle.kind {
        EdgeThreeCycleKind::FrontRightWing { row } => {
            if n < 4 {
                return Err(EdgeThreeCycleValidationError::WingCycleRequiresSideLengthAtLeastFour);
            }
            if !(row > 0 && row + 1 < n) {
                return Err(EdgeThreeCycleValidationError::RowMustBeInnerLayer);
            }
            if n % 2 == 1 && row == n / 2 {
                return Err(EdgeThreeCycleValidationError::WingRowCannotBeOddMiddleLayer);
            }
        }
        EdgeThreeCycleKind::FrontRightMiddle { .. } => {
            if n < 3 || n % 2 == 0 {
                return Err(
                    EdgeThreeCycleValidationError::MiddleCycleRequiresOddSideLengthAtLeastThree,
                );
            }
        }
    }

    Ok(())
}

fn edge_three_cycle_moves(n: usize, cycle: EdgeThreeCycle) -> Vec<Move> {
    validate_edge_three_cycle(n, cycle);

    match cycle.kind {
        EdgeThreeCycleKind::FrontRightWing { row } => edge_wing_three_cycle_moves(n, row),
        EdgeThreeCycleKind::FrontRightMiddle { direction } => {
            edge_middle_three_cycle_moves(n, direction)
        }
    }
}

fn edge_wing_three_cycle_moves(n: usize, row: usize) -> Vec<Move> {
    let mirror = n - 1 - row;
    let mut moves = Vec::with_capacity(18);
    moves.push(face_layer_move(n, FaceId::D, row, MoveAngle::Positive));
    moves.push(face_layer_move(n, FaceId::D, mirror, MoveAngle::Positive));
    moves.extend(flip_right_edge_moves(n));
    moves.push(face_layer_move(n, FaceId::D, row, MoveAngle::Negative));
    moves.extend(unflip_right_edge_moves(n));
    moves.push(face_layer_move(n, FaceId::D, mirror, MoveAngle::Negative));
    moves
}

fn edge_middle_three_cycle_moves(n: usize, direction: EdgeThreeCycleDirection) -> Vec<Move> {
    let base = edge_middle_three_cycle_base_moves(n, direction);
    [base.clone(), base].concat()
}

fn edge_middle_three_cycle_base_moves(n: usize, direction: EdgeThreeCycleDirection) -> Vec<Move> {
    let middle = n / 2;
    match direction {
        EdgeThreeCycleDirection::Positive => vec![
            face_layer_move(n, FaceId::L, middle, MoveAngle::Double),
            face_layer_move(n, FaceId::F, 0, MoveAngle::Positive),
            face_layer_move(n, FaceId::L, middle, MoveAngle::Negative),
            face_layer_move(n, FaceId::F, 0, MoveAngle::Double),
            face_layer_move(n, FaceId::L, middle, MoveAngle::Positive),
            face_layer_move(n, FaceId::F, 0, MoveAngle::Positive),
            face_layer_move(n, FaceId::L, middle, MoveAngle::Double),
        ],
        EdgeThreeCycleDirection::Negative => vec![
            face_layer_move(n, FaceId::L, middle, MoveAngle::Double),
            face_layer_move(n, FaceId::F, 0, MoveAngle::Negative),
            face_layer_move(n, FaceId::L, middle, MoveAngle::Positive),
            face_layer_move(n, FaceId::F, 0, MoveAngle::Double),
            face_layer_move(n, FaceId::L, middle, MoveAngle::Negative),
            face_layer_move(n, FaceId::F, 0, MoveAngle::Negative),
            face_layer_move(n, FaceId::L, middle, MoveAngle::Double),
        ],
    }
}

pub(crate) fn flip_right_edge_moves(n: usize) -> [Move; 7] {
    [
        face_layer_move(n, FaceId::R, 0, MoveAngle::Positive),
        face_layer_move(n, FaceId::U, 0, MoveAngle::Positive),
        face_layer_move(n, FaceId::R, 0, MoveAngle::Negative),
        face_layer_move(n, FaceId::F, 0, MoveAngle::Positive),
        face_layer_move(n, FaceId::R, 0, MoveAngle::Negative),
        face_layer_move(n, FaceId::F, 0, MoveAngle::Negative),
        face_layer_move(n, FaceId::R, 0, MoveAngle::Positive),
    ]
}

pub(crate) fn unflip_right_edge_moves(n: usize) -> [Move; 7] {
    [
        face_layer_move(n, FaceId::R, 0, MoveAngle::Negative),
        face_layer_move(n, FaceId::F, 0, MoveAngle::Positive),
        face_layer_move(n, FaceId::R, 0, MoveAngle::Positive),
        face_layer_move(n, FaceId::F, 0, MoveAngle::Negative),
        face_layer_move(n, FaceId::R, 0, MoveAngle::Positive),
        face_layer_move(n, FaceId::U, 0, MoveAngle::Negative),
        face_layer_move(n, FaceId::R, 0, MoveAngle::Negative),
    ]
}

fn try_edge_three_cycle_plan_from_moves(
    n: usize,
    cycle: Option<EdgeThreeCycle>,
    moves: Vec<Move>,
) -> Option<EdgeThreeCyclePlan> {
    if n < 3 || moves.is_empty() || moves.iter().any(|mv| mv.depth >= n) {
        return None;
    }

    let updates = move_sequence_updates(n, &moves)?;
    build_edge_three_cycle_plan(n, cycle, moves, updates)
}

fn try_edge_three_cycle_plan_from_edge_positions(
    n: usize,
    cycle: Option<EdgeThreeCycle>,
    moves: Vec<Move>,
) -> Option<EdgeThreeCyclePlan> {
    if n < 3 || moves.is_empty() || moves.iter().any(|mv| mv.depth >= n) {
        return None;
    }

    let updates = edge_position_updates(n, &moves);
    build_edge_three_cycle_plan(n, cycle, moves, updates)
}

fn build_edge_three_cycle_plan(
    n: usize,
    cycle: Option<EdgeThreeCycle>,
    moves: Vec<Move>,
    updates: Vec<FaceletUpdate>,
) -> Option<EdgeThreeCyclePlan> {
    let updates: [FaceletUpdate; 6] = updates.try_into().ok()?;

    if !facelet_locations_are_unique(updates.iter().map(|update| update.from))
        || !facelet_locations_are_unique(updates.iter().map(|update| update.to))
    {
        return None;
    }

    let source_cubies = unique_edge_cubies(n, updates.iter().map(|update| update.from))?;
    let destination_cubies = unique_edge_cubies(n, updates.iter().map(|update| update.to))?;
    if !edge_cubie_sets_match(source_cubies, destination_cubies) {
        return None;
    }

    let mut destination_for_source = [None; 3];
    let mut source_counts = [0usize; 3];

    for update in updates {
        let source = edge_cubie_location(n, update.from)?;
        let destination = edge_cubie_location(n, update.to)?;
        let source_index = edge_cubie_index(source_cubies, source)?;

        source_counts[source_index] += 1;
        match destination_for_source[source_index] {
            Some(existing) if existing != destination => return None,
            Some(_) => {}
            None => destination_for_source[source_index] = Some(destination),
        }
    }

    if source_counts != [2, 2, 2] {
        return None;
    }

    let first = source_cubies[0];
    let second = destination_for_source[0]?;
    if second == first {
        return None;
    }
    let second_index = edge_cubie_index(source_cubies, second)?;
    let third = destination_for_source[second_index]?;
    if third == first || third == second {
        return None;
    }
    let third_index = edge_cubie_index(source_cubies, third)?;
    if destination_for_source[third_index]? != first {
        return None;
    }

    Some(EdgeThreeCyclePlan {
        side_length: n,
        cycle,
        moves,
        cubies: [first, second, third],
        updates,
    })
}

pub(crate) fn edge_three_cycle_plan_from_updates(
    n: usize,
    cycle: Option<EdgeThreeCycle>,
    moves: Vec<Move>,
    updates: Vec<FaceletUpdate>,
) -> EdgeThreeCyclePlan {
    build_edge_three_cycle_plan(n, cycle, moves, updates)
        .expect("explicit edge three-cycle updates must define a valid exact three-cycle")
}

pub(crate) fn move_sequence_updates(n: usize, moves: &[Move]) -> Option<Vec<FaceletUpdate>> {
    if n == 0 {
        return None;
    }

    let mut updates = Vec::new();
    for face in FaceId::ALL {
        for row in 0..n {
            for col in 0..n {
                let from = FacePosition { face, row, col };
                let to = trace_position(n, from, moves.iter().copied());
                if from != to {
                    updates.push(FaceletUpdate {
                        from: facelet_location(from),
                        to: facelet_location(to),
                    });
                }
            }
        }
    }

    Some(updates)
}

fn edge_position_updates(n: usize, moves: &[Move]) -> Vec<FaceletUpdate> {
    let mut updates = Vec::new();

    for face in FaceId::ALL {
        for offset in 1..n - 1 {
            for (row, col) in [(0, offset), (n - 1, offset), (offset, 0), (offset, n - 1)] {
                let from = FacePosition { face, row, col };
                let to = trace_position(n, from, moves.iter().copied());
                if from != to {
                    updates.push(FaceletUpdate {
                        from: facelet_location(from),
                        to: facelet_location(to),
                    });
                }
            }
        }
    }

    updates
}

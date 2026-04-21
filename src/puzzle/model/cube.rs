use core::fmt;
use core::fmt::Write;

use crate::{
    face::{Face, FaceId},
    facelet::Facelet,
    geometry,
    history::MoveHistory,
    line::{
        cycle_four_lines, cycle_four_lines_with_threads, with_line_cycle_runner, LineCycleRunner,
        StripSpec,
    },
    moves::{Axis, Move, MoveAngle},
    random::RandomSource,
    storage::FaceletArray,
    threading::default_thread_count,
};

pub const DEFAULT_SCRAMBLE_ROUNDS: usize = 6;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ColorScheme {
    pub u: Facelet,
    pub d: Facelet,
    pub r: Facelet,
    pub l: Facelet,
    pub f: Facelet,
    pub b: Facelet,
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self {
            u: Facelet::White,
            d: Facelet::Yellow,
            r: Facelet::Red,
            l: Facelet::Orange,
            f: Facelet::Green,
            b: Facelet::Blue,
        }
    }
}

impl ColorScheme {
    pub const fn color_of(self, face: FaceId) -> Facelet {
        match face {
            FaceId::U => self.u,
            FaceId::D => self.d,
            FaceId::R => self.r,
            FaceId::L => self.l,
            FaceId::F => self.f,
            FaceId::B => self.b,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FaceCommutator {
    destination: FaceId,
    helper: FaceId,
    slice_angle: MoveAngle,
    expanded_template: CenterCommutatorTemplate,
    normalized_template: CenterCommutatorTemplate,
}

impl FaceCommutator {
    pub fn new(destination: FaceId, helper: FaceId, slice_angle: MoveAngle) -> Self {
        assert_ne!(
            destination, helper,
            "destination and helper faces must differ"
        );
        assert_ne!(
            destination,
            opposite_face(helper),
            "destination and helper faces must be perpendicular"
        );

        Self {
            destination,
            helper,
            slice_angle,
            expanded_template: CenterCommutatorTemplate::expanded(destination, helper, slice_angle),
            normalized_template: CenterCommutatorTemplate::normalized(
                destination,
                helper,
                slice_angle,
            ),
        }
    }

    pub fn destination(self) -> FaceId {
        self.destination
    }

    pub fn helper(self) -> FaceId {
        self.helper
    }

    pub fn slice_angle(self) -> MoveAngle {
        self.slice_angle
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct FaceletLocation {
    pub face: FaceId,
    pub row: usize,
    pub col: usize,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct FaceletUpdate {
    pub from: FaceletLocation,
    pub to: FaceletLocation,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct EdgeCubieLocation {
    /// The two stickers belonging to the same physical edge cubie, stored in a
    /// stable canonical order.
    pub stickers: [FaceletLocation; 2],
}

impl EdgeCubieLocation {
    pub const fn stickers(self) -> [FaceletLocation; 2] {
        self.stickers
    }
}

pub(crate) fn edge_cubie_for_facelet_location(
    side_length: usize,
    location: FaceletLocation,
) -> Option<EdgeCubieLocation> {
    edge_cubie_location(side_length, location)
}

pub(crate) fn edge_cubie_orbit_index(
    side_length: usize,
    cubie: EdgeCubieLocation,
) -> Option<usize> {
    let [first, second] = cubie.stickers();
    let first = edge_facelet_orbit_index(side_length, first)?;
    let second = edge_facelet_orbit_index(side_length, second)?;
    if first == second { Some(first) } else { None }
}

pub(crate) fn trace_edge_cubie_through_move(
    side_length: usize,
    cubie: EdgeCubieLocation,
    mv: Move,
) -> EdgeCubieLocation {
    let [first, second] = cubie.stickers();
    let first = trace_position_through_move(
        side_length,
        FacePosition {
            face: first.face,
            row: first.row,
            col: first.col,
        },
        mv,
    );
    let second = trace_position_through_move(
        side_length,
        FacePosition {
            face: second.face,
            row: second.row,
            col: second.col,
        },
        mv,
    );
    let first_location = facelet_location(first);
    let second_location = facelet_location(second);
    let turning_face = if mv.depth == 0 {
        Some(geometry::negative_axis_face(mv.axis))
    } else if mv.depth + 1 == side_length {
        Some(geometry::positive_axis_face(mv.axis))
    } else {
        None
    };
    let anchor = if turning_face == Some(cubie.stickers()[0].face) {
        first_location
    } else if turning_face == Some(cubie.stickers()[1].face) {
        second_location
    } else {
        first_location
    };

    edge_cubie_location(side_length, anchor).expect("traced edge sticker must stay on an edge cubie")
}

#[cfg(test)]
pub(crate) fn trace_facelet_location_through_moves(
    side_length: usize,
    location: FaceletLocation,
    moves: &[Move],
) -> FaceletLocation {
    let position = trace_position(
        side_length,
        FacePosition {
            face: location.face,
            row: location.row,
            col: location.col,
        },
        moves.iter().copied(),
    );
    facelet_location(position)
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct EdgeThreeCycle {
    kind: EdgeThreeCycleKind,
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

impl EdgeThreeCycleDirection {
    pub const ALL: [Self; 2] = [Self::Positive, Self::Negative];

    pub const fn inverse(self) -> Self {
        match self {
            Self::Positive => Self::Negative,
            Self::Negative => Self::Positive,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EdgeThreeCyclePlan {
    side_length: usize,
    cycle: Option<EdgeThreeCycle>,
    moves: Vec<Move>,
    cubies: [EdgeCubieLocation; 3],
    updates: [FaceletUpdate; 6],
}

impl EdgeThreeCyclePlan {
    /// Builds the sparse plan for a named edge three-cycle recipe.
    ///
    /// This path scans only edge stickers, so it is suitable for precomputing
    /// plans on very large cubes.
    pub fn from_cycle(side_length: usize, cycle: EdgeThreeCycle) -> Self {
        validate_edge_three_cycle(side_length, cycle);
        try_edge_three_cycle_plan_from_edge_positions(
            side_length,
            Some(cycle),
            cycle.moves(side_length),
        )
        .expect("edge three-cycle recipe must produce exactly one edge cubie 3-cycle")
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
}

#[derive(Clone, Debug)]
pub struct Cube<S: FaceletArray> {
    n: usize,
    faces: [Face<S>; 6],
    history: MoveHistory,
}

impl<S: FaceletArray> Cube<S> {
    pub fn new_solved(n: usize) -> Self {
        Self::new_solved_with_threads(n, default_thread_count())
    }

    pub fn new_solved_with_threads(n: usize, thread_count: usize) -> Self {
        Self::new_with_scheme_with_threads(n, ColorScheme::default(), thread_count)
    }

    pub fn new_with_scheme(n: usize, scheme: ColorScheme) -> Self {
        Self::new_with_scheme_with_threads(n, scheme, default_thread_count())
    }

    pub fn new_with_scheme_with_threads(
        n: usize,
        scheme: ColorScheme,
        thread_count: usize,
    ) -> Self {
        assert!(n > 0, "cube side length must be > 0");
        assert!(thread_count > 0, "thread count must be greater than zero");

        Self {
            n,
            faces: [
                Face::new_with_threads(FaceId::U, n, scheme.u, thread_count),
                Face::new_with_threads(FaceId::D, n, scheme.d, thread_count),
                Face::new_with_threads(FaceId::R, n, scheme.r, thread_count),
                Face::new_with_threads(FaceId::L, n, scheme.l, thread_count),
                Face::new_with_threads(FaceId::F, n, scheme.f, thread_count),
                Face::new_with_threads(FaceId::B, n, scheme.b, thread_count),
            ],
            history: MoveHistory::new(),
        }
    }

    pub fn side_len(&self) -> usize {
        self.n
    }

    pub fn face(&self, id: FaceId) -> &Face<S> {
        &self.faces[id.index()]
    }

    pub fn face_mut(&mut self, id: FaceId) -> &mut Face<S> {
        &mut self.faces[id.index()]
    }

    pub fn faces(&self) -> &[Face<S>; 6] {
        &self.faces
    }

    pub fn history(&self) -> &MoveHistory {
        &self.history
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    pub fn apply_move(&mut self, mv: Move) {
        self.apply_move_untracked(mv);
        self.history.push(mv);
    }

    pub fn apply_move_with_threads(&mut self, mv: Move, thread_count: usize) {
        self.apply_move_untracked_with_threads(mv, thread_count);
        self.history.push(mv);
    }

    pub fn apply_move_untracked(&mut self, mv: Move) {
        self.apply_move_untracked_with_threads(mv, default_thread_count());
    }

    pub fn apply_move_untracked_with_threads(&mut self, mv: Move, thread_count: usize) {
        assert!(thread_count > 0, "thread count must be greater than zero");

        if thread_count == 1 {
            self.apply_move_untracked_linear(mv);
            return;
        }

        self.validate_move(mv);
        let specs = self.plan_move(mv);
        self.rotate_outer_face_meta(mv);
        self.apply_side_cycle_with_threads(specs, mv.angle, thread_count);
    }

    fn apply_move_untracked_linear(&mut self, mv: Move) {
        self.validate_move(mv);
        let specs = self.plan_move(mv);
        self.rotate_outer_face_meta(mv);
        self.apply_side_cycle(specs, mv.angle);
    }

    pub fn apply_moves<I>(&mut self, moves: I)
    where
        I: IntoIterator<Item = Move>,
    {
        for mv in moves {
            self.apply_move(mv);
        }
    }

    pub fn apply_moves_untracked<I>(&mut self, moves: I)
    where
        I: IntoIterator<Item = Move>,
    {
        self.apply_moves_untracked_with_threads(moves, default_thread_count());
    }

    pub fn apply_moves_untracked_with_threads<I>(&mut self, moves: I, thread_count: usize)
    where
        I: IntoIterator<Item = Move>,
    {
        assert!(thread_count > 0, "thread count must be greater than zero");

        if thread_count == 1 {
            for mv in moves {
                self.apply_move_untracked_linear(mv);
            }
            return;
        }

        with_line_cycle_runner::<S, _, _>(self.n, thread_count, |runner| {
            for mv in moves {
                self.apply_move_untracked_with_runner(mv, runner);
            }
        });
    }

    pub fn plan_move(&self, mv: Move) -> [StripSpec; 4] {
        self.validate_move(mv);
        geometry::plan_positive_quarter_turn(mv.axis, mv.depth, self.n)
    }

    pub fn rotate_outer_face_meta(&mut self, mv: Move) {
        self.validate_move(mv);

        if mv.depth == self.n - 1 {
            let face = geometry::positive_axis_face(mv.axis);
            self.faces[face.index()].rotate_meta_by(mv.angle);
        }

        if mv.depth == 0 {
            let face = geometry::negative_axis_face(mv.axis);
            self.faces[face.index()].rotate_meta_by(mv.angle.inverse());
        }
    }

    pub fn random_move<R: RandomSource>(&self, rng: &mut R) -> Move {
        let axis = match (rng.next_u64() % 3) as u8 {
            0 => Axis::X,
            1 => Axis::Y,
            _ => Axis::Z,
        };

        let depth = (rng.next_u64() as usize) % self.n;
        Move::new(axis, depth, random_move_angle(rng))
    }

    pub fn random_outer_face_move<R: RandomSource>(&self, face: FaceId, rng: &mut R) -> Move {
        face_layer_move(self.n, face, 0, random_move_angle(rng))
    }

    pub fn scramble<R: RandomSource>(&mut self, rng: &mut R) {
        self.scramble_with_rounds(rng, DEFAULT_SCRAMBLE_ROUNDS);
    }

    pub fn scramble_with_rounds<R: RandomSource>(&mut self, rng: &mut R, rounds: usize) {
        for _ in 0..rounds {
            self.scramble_random_moves(rng, self.n);

            for face in FaceId::ALL {
                let mv = self.random_outer_face_move(face, rng);
                self.apply_move(mv);
            }
        }
    }

    pub fn scramble_random_moves<R: RandomSource>(&mut self, rng: &mut R, count: usize) {
        for _ in 0..count {
            let mv = self.random_move(rng);
            self.apply_move(mv);
        }
    }

    pub fn is_solved(&self) -> bool {
        for id in FaceId::ALL {
            let face = self.face(id);
            let target = face.get(0, 0);

            for row in 0..self.n {
                for col in 0..self.n {
                    if face.get(row, col) != target {
                        return false;
                    }
                }
            }
        }

        true
    }

    pub fn estimated_storage_bytes(&self) -> usize {
        let cells_per_face = self
            .n
            .checked_mul(self.n)
            .expect("cube face cell count overflowed usize");
        S::storage_bytes_for_len(cells_per_face)
            .checked_mul(6)
            .expect("cube storage estimate overflowed usize")
    }

    /// Returns the literal move sequence represented by `apply_face_commutator_untracked`.
    pub fn face_commutator_moves(
        &self,
        destination: FaceId,
        helper: FaceId,
        rows: &[usize],
        columns: &[usize],
        slice_angle: MoveAngle,
    ) -> Vec<Move> {
        self.validate_face_commutator(destination, helper, rows, columns);
        face_commutator_moves(self.n, destination, helper, rows, columns, slice_angle)
    }

    /// Returns the literal move sequence represented by
    /// `apply_normalized_face_commutator_untracked`.
    ///
    /// This is the expanded face commutator followed by the inverse of its net
    /// destination outer-face turn.
    pub fn normalized_face_commutator_moves(
        &self,
        destination: FaceId,
        helper: FaceId,
        rows: &[usize],
        columns: &[usize],
        slice_angle: MoveAngle,
    ) -> Vec<Move> {
        self.validate_face_commutator(destination, helper, rows, columns);
        normalized_face_commutator_moves(self.n, destination, helper, rows, columns, slice_angle)
    }

    /// Applies the exact state change of the expanded face commutator while
    /// avoiding the per-column full-slice moves.
    ///
    /// The expanded sequence is `helper columns^-1`, destination face turn,
    /// `helper rows^-1`, destination inverse, `helper columns`, destination face
    /// turn, `helper rows`. Rows and columns must be sorted, disjoint inner
    /// layers.
    pub fn apply_face_commutator_untracked(
        &mut self,
        destination: FaceId,
        helper: FaceId,
        rows: &[usize],
        columns: &[usize],
        slice_angle: MoveAngle,
    ) {
        let commutator = FaceCommutator::new(destination, helper, slice_angle);
        self.apply_face_commutator_plan_untracked(commutator, rows, columns);
    }

    pub fn apply_face_commutator_plan_untracked(
        &mut self,
        commutator: FaceCommutator,
        rows: &[usize],
        columns: &[usize],
    ) {
        self.validate_face_commutator(commutator.destination, commutator.helper, rows, columns);
        self.apply_face_commutator_untracked_direct(commutator, rows, columns);
    }

    /// Applies the expanded face commutator followed by the inverse destination
    /// outer-face turn, but performs only the sparse center delta.
    pub fn apply_normalized_face_commutator_untracked(
        &mut self,
        destination: FaceId,
        helper: FaceId,
        rows: &[usize],
        columns: &[usize],
        slice_angle: MoveAngle,
    ) {
        let commutator = FaceCommutator::new(destination, helper, slice_angle);
        self.apply_normalized_face_commutator_plan_untracked(commutator, rows, columns);
    }

    pub fn apply_normalized_face_commutator_plan_untracked(
        &mut self,
        commutator: FaceCommutator,
        rows: &[usize],
        columns: &[usize],
    ) {
        self.validate_face_commutator(commutator.destination, commutator.helper, rows, columns);
        self.apply_sparse_commutator_template_untracked(
            commutator.normalized_template,
            rows,
            columns,
        );
    }

    pub fn face_commutator_sparse_updates(
        &self,
        commutator: FaceCommutator,
        row: usize,
        column: usize,
    ) -> [FaceletUpdate; 3] {
        self.validate_face_commutator(commutator.destination, commutator.helper, &[row], &[column]);

        commutator
            .expanded_template
            .updates
            .map(|update| FaceletUpdate {
                from: facelet_location(update.from.eval(self.n, row, column)),
                to: facelet_location(update.to.eval(self.n, row, column)),
            })
    }

    pub fn normalized_face_commutator_sparse_updates(
        &self,
        commutator: FaceCommutator,
        row: usize,
        column: usize,
    ) -> [FaceletUpdate; 3] {
        self.validate_face_commutator(commutator.destination, commutator.helper, &[row], &[column]);

        commutator
            .normalized_template
            .updates
            .map(|update| FaceletUpdate {
                from: facelet_location(update.from.eval(self.n, row, column)),
                to: facelet_location(update.to.eval(self.n, row, column)),
            })
    }

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

    /// Applies a precomputed edge three-cycle plan with delayed reads, so the
    /// cyclic overwrite order cannot corrupt source stickers.
    pub fn apply_edge_three_cycle_plan_untracked(&mut self, plan: &EdgeThreeCyclePlan) {
        assert_eq!(
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

    /// Reference implementation for `apply_face_commutator_untracked`.
    ///
    /// This keeps the geometry-derived sparse mapping and delayed writes in one
    /// place so the direct hot path can be tested against it.
    pub fn apply_face_commutator_untracked_reference(
        &mut self,
        destination: FaceId,
        helper: FaceId,
        rows: &[usize],
        columns: &[usize],
        slice_angle: MoveAngle,
    ) {
        self.validate_face_commutator(destination, helper, rows, columns);

        let baseline = face_layer_move(self.n, destination, 0, MoveAngle::Positive);
        self.apply_move_untracked_linear(baseline);

        let mut writes = Vec::with_capacity(rows.len() * columns.len() * 3);
        for row in rows.iter().copied() {
            for column in columns.iter().copied() {
                for (from, to) in expanded_face_commutator_difference_cycle(
                    self.n,
                    destination,
                    helper,
                    row,
                    column,
                    slice_angle,
                ) {
                    writes.push((to, self.position(from)));
                }
            }
        }
        assert_unique_positions(writes.iter().map(|(position, _)| *position));

        for (position, value) in writes {
            self.set_position(position, value);
        }
    }

    fn apply_face_commutator_untracked_direct(
        &mut self,
        commutator: FaceCommutator,
        rows: &[usize],
        columns: &[usize],
    ) {
        let baseline = face_layer_move(self.n, commutator.destination, 0, MoveAngle::Positive);
        self.apply_move_untracked_linear(baseline);

        if rows.is_empty() || columns.is_empty() {
            return;
        }

        self.apply_sparse_commutator_template_untracked(
            commutator.expanded_template,
            rows,
            columns,
        );
    }

    fn apply_sparse_commutator_template_untracked(
        &mut self,
        template: CenterCommutatorTemplate,
        rows: &[usize],
        columns: &[usize],
    ) {
        if rows.is_empty() || columns.is_empty() {
            return;
        }

        let storages = raw_face_storages(&mut self.faces);

        for row in rows.iter().copied() {
            let bound = template.bind(&self.faces, self.n, row);
            unsafe {
                bound.apply_columns::<S>(&storages, columns);
            }
        }
    }

    pub fn net_string(&self) -> String {
        let rows = net_layers(self.n);
        let cols = net_layers(self.n);
        let face_width = net_face_width(&cols);
        let middle_indent = " ".repeat(face_width + NET_FACE_GAP.len());
        let mut out = String::new();

        let _ = writeln!(
            out,
            "Cube(n={}, history={}, storage~{} bytes)",
            self.n,
            self.history.len(),
            self.estimated_storage_bytes(),
        );

        self.push_net_face_block(
            &mut out,
            &rows,
            &cols,
            |out| out.push_str(&middle_indent),
            &[FaceId::U],
        );
        out.push('\n');

        self.push_net_face_block(
            &mut out,
            &rows,
            &cols,
            |_| {},
            &[FaceId::L, FaceId::F, FaceId::R, FaceId::B],
        );
        out.push('\n');

        self.push_net_face_block(
            &mut out,
            &rows,
            &cols,
            |out| out.push_str(&middle_indent),
            &[FaceId::D],
        );

        out
    }

    fn push_net_face_block(
        &self,
        out: &mut String,
        rows: &[NetLayer],
        cols: &[NetLayer],
        mut push_prefix: impl FnMut(&mut String),
        faces: &[FaceId],
    ) {
        for row in rows.iter().copied() {
            push_prefix(out);
            for (face_index, face) in faces.iter().copied().enumerate() {
                if face_index > 0 {
                    out.push_str(NET_FACE_GAP);
                }
                self.push_net_face_row(out, face, row, cols);
            }
            out.push('\n');
        }
    }

    fn push_net_face_row(&self, out: &mut String, face: FaceId, row: NetLayer, cols: &[NetLayer]) {
        for (col_index, col) in cols.iter().copied().enumerate() {
            if col_index > 0 {
                out.push(' ');
            }
            match (row, col) {
                (NetLayer::Index(row), NetLayer::Index(col)) => {
                    out.push(self.face(face).get(row, col).as_char());
                }
                (NetLayer::Separator, _) | (_, NetLayer::Separator) => out.push('-'),
            }
        }
    }

    fn validate_move(&self, mv: Move) {
        assert!(mv.depth < self.n, "move depth out of bounds");
    }

    fn validate_face_commutator(
        &self,
        destination: FaceId,
        helper: FaceId,
        rows: &[usize],
        columns: &[usize],
    ) {
        assert!(
            self.n >= 3,
            "face commutators require side length at least 3"
        );
        assert_ne!(
            destination, helper,
            "destination and helper faces must differ"
        );
        assert_ne!(
            destination,
            opposite_face(helper),
            "destination and helper faces must be perpendicular"
        );
        validate_inner_layer_set(self.n, rows, "commutator rows");
        validate_inner_layer_set(self.n, columns, "commutator columns");
        assert!(
            sorted_layer_sets_are_disjoint(rows, columns),
            "commutator row and column layer sets must be disjoint"
        );
    }

    fn position(&self, position: FacePosition) -> Facelet {
        self.face(position.face).get(position.row, position.col)
    }

    fn set_position(&mut self, position: FacePosition, value: Facelet) {
        self.face_mut(position.face)
            .set(position.row, position.col, value);
    }

    #[inline]
    fn apply_side_cycle(&mut self, specs: [StripSpec; 4], angle: MoveAngle) {
        let traversals = [
            self.faces[specs[0].face.index()].line_traversal(
                specs[0].kind,
                specs[0].index,
                specs[0].reversed,
            ),
            self.faces[specs[1].face.index()].line_traversal(
                specs[1].kind,
                specs[1].index,
                specs[1].reversed,
            ),
            self.faces[specs[2].face.index()].line_traversal(
                specs[2].kind,
                specs[2].index,
                specs[2].reversed,
            ),
            self.faces[specs[3].face.index()].line_traversal(
                specs[3].kind,
                specs[3].index,
                specs[3].reversed,
            ),
        ];
        let face_indices = [
            specs[0].face.index(),
            specs[1].face.index(),
            specs[2].face.index(),
            specs[3].face.index(),
        ];
        let (face0, face1, face2, face3) = faces4_mut(&mut self.faces, face_indices);

        cycle_four_lines(
            face0.matrix_mut().storage_mut(),
            traversals[0],
            face1.matrix_mut().storage_mut(),
            traversals[1],
            face2.matrix_mut().storage_mut(),
            traversals[2],
            face3.matrix_mut().storage_mut(),
            traversals[3],
            self.n,
            angle,
        );
    }

    #[inline]
    fn apply_side_cycle_with_threads(
        &mut self,
        specs: [StripSpec; 4],
        angle: MoveAngle,
        thread_count: usize,
    ) {
        let traversals = [
            self.faces[specs[0].face.index()].line_traversal(
                specs[0].kind,
                specs[0].index,
                specs[0].reversed,
            ),
            self.faces[specs[1].face.index()].line_traversal(
                specs[1].kind,
                specs[1].index,
                specs[1].reversed,
            ),
            self.faces[specs[2].face.index()].line_traversal(
                specs[2].kind,
                specs[2].index,
                specs[2].reversed,
            ),
            self.faces[specs[3].face.index()].line_traversal(
                specs[3].kind,
                specs[3].index,
                specs[3].reversed,
            ),
        ];
        let face_indices = [
            specs[0].face.index(),
            specs[1].face.index(),
            specs[2].face.index(),
            specs[3].face.index(),
        ];
        let (face0, face1, face2, face3) = faces4_mut(&mut self.faces, face_indices);

        cycle_four_lines_with_threads(
            face0.matrix_mut().storage_mut(),
            traversals[0],
            face1.matrix_mut().storage_mut(),
            traversals[1],
            face2.matrix_mut().storage_mut(),
            traversals[2],
            face3.matrix_mut().storage_mut(),
            traversals[3],
            self.n,
            angle,
            thread_count,
        );
    }

    fn apply_move_untracked_with_runner(&mut self, mv: Move, runner: &mut LineCycleRunner<'_, S>) {
        self.validate_move(mv);
        let specs = self.plan_move(mv);
        self.rotate_outer_face_meta(mv);
        self.apply_side_cycle_with_runner(specs, mv.angle, runner);
    }

    #[inline]
    fn apply_side_cycle_with_runner(
        &mut self,
        specs: [StripSpec; 4],
        angle: MoveAngle,
        runner: &mut LineCycleRunner<'_, S>,
    ) {
        let traversals = [
            self.faces[specs[0].face.index()].line_traversal(
                specs[0].kind,
                specs[0].index,
                specs[0].reversed,
            ),
            self.faces[specs[1].face.index()].line_traversal(
                specs[1].kind,
                specs[1].index,
                specs[1].reversed,
            ),
            self.faces[specs[2].face.index()].line_traversal(
                specs[2].kind,
                specs[2].index,
                specs[2].reversed,
            ),
            self.faces[specs[3].face.index()].line_traversal(
                specs[3].kind,
                specs[3].index,
                specs[3].reversed,
            ),
        ];
        let face_indices = [
            specs[0].face.index(),
            specs[1].face.index(),
            specs[2].face.index(),
            specs[3].face.index(),
        ];
        let (face0, face1, face2, face3) = faces4_mut(&mut self.faces, face_indices);

        runner.cycle(
            face0.matrix_mut().storage_mut(),
            traversals[0],
            face1.matrix_mut().storage_mut(),
            traversals[1],
            face2.matrix_mut().storage_mut(),
            traversals[2],
            face3.matrix_mut().storage_mut(),
            traversals[3],
            angle,
        );
    }
}

#[inline]
fn faces4_mut<S: FaceletArray>(
    faces: &mut [Face<S>; 6],
    indices: [usize; 4],
) -> (&mut Face<S>, &mut Face<S>, &mut Face<S>, &mut Face<S>) {
    for i in 0..indices.len() {
        assert!(indices[i] < faces.len(), "face index out of bounds");
        for j in i + 1..indices.len() {
            assert_ne!(
                indices[i], indices[j],
                "move side strips must use distinct faces"
            );
        }
    }

    let ptr = faces.as_mut_ptr();
    unsafe {
        // The index checks above guarantee these mutable references do not alias.
        (
            &mut *ptr.add(indices[0]),
            &mut *ptr.add(indices[1]),
            &mut *ptr.add(indices[2]),
            &mut *ptr.add(indices[3]),
        )
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
struct FacePosition {
    face: FaceId,
    row: usize,
    col: usize,
}

fn facelet_location(position: FacePosition) -> FaceletLocation {
    FaceletLocation {
        face: position.face,
        row: position.row,
        col: position.col,
    }
}

const COMMUTATOR_TEMPLATE_N: usize = 101;
const COMMUTATOR_TEMPLATE_ROW: usize = 17;
const COMMUTATOR_TEMPLATE_COLUMN: usize = 31;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum CoordinateExpr {
    Row,
    Column,
    ReverseRow,
    ReverseColumn,
}

impl CoordinateExpr {
    #[inline(always)]
    fn eval(self, n: usize, row: usize, column: usize) -> usize {
        match self {
            Self::Row => row,
            Self::Column => column,
            Self::ReverseRow => n - 1 - row,
            Self::ReverseColumn => n - 1 - column,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct FacePositionExpr {
    face: FaceId,
    row: CoordinateExpr,
    col: CoordinateExpr,
}

impl FacePositionExpr {
    fn eval(self, n: usize, row: usize, column: usize) -> FacePosition {
        FacePosition {
            face: self.face,
            row: self.row.eval(n, row, column),
            col: self.col.eval(n, row, column),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct PositionUpdateExpr {
    from: FacePositionExpr,
    to: FacePositionExpr,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct CenterCommutatorTemplate {
    updates: [PositionUpdateExpr; 3],
}

impl CenterCommutatorTemplate {
    fn expanded(destination: FaceId, helper: FaceId, slice_angle: MoveAngle) -> Self {
        let updates = expanded_face_commutator_difference_cycle(
            COMMUTATOR_TEMPLATE_N,
            destination,
            helper,
            COMMUTATOR_TEMPLATE_ROW,
            COMMUTATOR_TEMPLATE_COLUMN,
            slice_angle,
        )
        .map(|(from, to)| PositionUpdateExpr {
            from: classify_template_position(from),
            to: classify_template_position(to),
        });

        Self { updates }
    }

    fn normalized(destination: FaceId, helper: FaceId, slice_angle: MoveAngle) -> Self {
        let updates = normalized_face_commutator_difference_cycle(
            COMMUTATOR_TEMPLATE_N,
            destination,
            helper,
            COMMUTATOR_TEMPLATE_ROW,
            COMMUTATOR_TEMPLATE_COLUMN,
            slice_angle,
        )
        .map(|(from, to)| PositionUpdateExpr {
            from: classify_template_position(from),
            to: classify_template_position(to),
        });

        Self { updates }
    }

    fn bind<S: FaceletArray>(
        self,
        faces: &[Face<S>; 6],
        n: usize,
        row: usize,
    ) -> BoundCenterCommutator {
        BoundCenterCommutator {
            updates: self
                .updates
                .map(|update| BoundPositionUpdate::bind(faces, n, row, update)),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct RawCellStream {
    face: FaceId,
    start: usize,
    step: isize,
}

impl RawCellStream {
    fn bind<S: FaceletArray>(
        faces: &[Face<S>; 6],
        n: usize,
        row: usize,
        expr: FacePositionExpr,
    ) -> Self {
        let start = raw_index_for_expr(faces, n, row, 0, expr);
        let next = raw_index_for_expr(faces, n, row, 1, expr);
        let start = isize::try_from(start).expect("raw index overflowed isize");
        let next = isize::try_from(next).expect("raw index overflowed isize");

        Self {
            face: expr.face,
            start: start as usize,
            step: next - start,
        }
    }

    #[inline(always)]
    unsafe fn index_unchecked(self, column: usize) -> usize {
        let column = column as isize;
        (self.start as isize + self.step * column) as usize
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct BoundPositionUpdate {
    from: RawCellStream,
    to: RawCellStream,
}

impl BoundPositionUpdate {
    fn bind<S: FaceletArray>(
        faces: &[Face<S>; 6],
        n: usize,
        row: usize,
        update: PositionUpdateExpr,
    ) -> Self {
        Self {
            from: RawCellStream::bind(faces, n, row, update.from),
            to: RawCellStream::bind(faces, n, row, update.to),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct BoundCenterCommutator {
    updates: [BoundPositionUpdate; 3],
}

impl BoundCenterCommutator {
    unsafe fn apply_columns<S: FaceletArray>(
        self,
        storages: &[S::RawStorage; 6],
        columns: &[usize],
    ) {
        debug_assert!(columns.windows(2).all(|window| window[0] < window[1]));

        let mut index = 0;
        while index < columns.len() {
            let start_column = columns[index];
            let mut run_len = 1;

            while index + run_len < columns.len()
                && columns[index + run_len] == columns[index + run_len - 1] + 1
            {
                run_len += 1;
            }

            if run_len == 1 {
                self.apply_one::<S>(storages, start_column);
            } else {
                self.apply_run::<S>(storages, start_column, run_len);
            }

            index += run_len;
        }
    }

    #[inline(always)]
    unsafe fn apply_one<S: FaceletArray>(self, storages: &[S::RawStorage; 6], column: usize) {
        let from0 = self.updates[0].from.index_unchecked(column);
        let from1 = self.updates[1].from.index_unchecked(column);
        let from2 = self.updates[2].from.index_unchecked(column);
        let to0 = self.updates[0].to.index_unchecked(column);
        let to1 = self.updates[1].to.index_unchecked(column);
        let to2 = self.updates[2].to.index_unchecked(column);

        let v0 = S::get_unchecked_raw_from(storages[self.updates[0].from.face.index()], from0);
        let v1 = S::get_unchecked_raw_from(storages[self.updates[1].from.face.index()], from1);
        let v2 = S::get_unchecked_raw_from(storages[self.updates[2].from.face.index()], from2);

        S::set_unchecked_raw_in(storages[self.updates[0].to.face.index()], to0, v0);
        S::set_unchecked_raw_in(storages[self.updates[1].to.face.index()], to1, v1);
        S::set_unchecked_raw_in(storages[self.updates[2].to.face.index()], to2, v2);
    }

    #[inline(always)]
    unsafe fn apply_run<S: FaceletArray>(
        self,
        storages: &[S::RawStorage; 6],
        start_column: usize,
        len: usize,
    ) {
        let mut from0 = self.updates[0].from.index_unchecked(start_column);
        let mut from1 = self.updates[1].from.index_unchecked(start_column);
        let mut from2 = self.updates[2].from.index_unchecked(start_column);
        let mut to0 = self.updates[0].to.index_unchecked(start_column);
        let mut to1 = self.updates[1].to.index_unchecked(start_column);
        let mut to2 = self.updates[2].to.index_unchecked(start_column);

        for _ in 0..len {
            let v0 = S::get_unchecked_raw_from(storages[self.updates[0].from.face.index()], from0);
            let v1 = S::get_unchecked_raw_from(storages[self.updates[1].from.face.index()], from1);
            let v2 = S::get_unchecked_raw_from(storages[self.updates[2].from.face.index()], from2);

            S::set_unchecked_raw_in(storages[self.updates[0].to.face.index()], to0, v0);
            S::set_unchecked_raw_in(storages[self.updates[1].to.face.index()], to1, v1);
            S::set_unchecked_raw_in(storages[self.updates[2].to.face.index()], to2, v2);

            from0 = add_raw_step(from0, self.updates[0].from.step);
            from1 = add_raw_step(from1, self.updates[1].from.step);
            from2 = add_raw_step(from2, self.updates[2].from.step);
            to0 = add_raw_step(to0, self.updates[0].to.step);
            to1 = add_raw_step(to1, self.updates[1].to.step);
            to2 = add_raw_step(to2, self.updates[2].to.step);
        }
    }
}

#[inline(always)]
fn add_raw_step(index: usize, step: isize) -> usize {
    (index as isize + step) as usize
}

fn raw_index_for_expr<S: FaceletArray>(
    faces: &[Face<S>; 6],
    n: usize,
    row: usize,
    column: usize,
    expr: FacePositionExpr,
) -> usize {
    let position = expr.eval(n, row, column);
    let (physical_row, physical_col) =
        faces[position.face.index()].physical_coords(position.row, position.col);

    physical_row
        .checked_mul(n)
        .and_then(|row_start| row_start.checked_add(physical_col))
        .expect("raw face index overflowed usize")
}

fn classify_template_position(position: FacePosition) -> FacePositionExpr {
    FacePositionExpr {
        face: position.face,
        row: classify_template_coordinate(position.row),
        col: classify_template_coordinate(position.col),
    }
}

fn classify_template_coordinate(value: usize) -> CoordinateExpr {
    match value {
        COMMUTATOR_TEMPLATE_ROW => CoordinateExpr::Row,
        COMMUTATOR_TEMPLATE_COLUMN => CoordinateExpr::Column,
        value if value == COMMUTATOR_TEMPLATE_N - 1 - COMMUTATOR_TEMPLATE_ROW => {
            CoordinateExpr::ReverseRow
        }
        value if value == COMMUTATOR_TEMPLATE_N - 1 - COMMUTATOR_TEMPLATE_COLUMN => {
            CoordinateExpr::ReverseColumn
        }
        _ => panic!("commutator template coordinate does not depend on row or column: {value}"),
    }
}

fn raw_face_storages<S: FaceletArray>(faces: &mut [Face<S>; 6]) -> [S::RawStorage; 6] {
    let ptr = faces.as_mut_ptr();
    unsafe {
        [
            (*ptr.add(FaceId::U.index()))
                .matrix_mut()
                .storage_mut()
                .raw_storage(),
            (*ptr.add(FaceId::D.index()))
                .matrix_mut()
                .storage_mut()
                .raw_storage(),
            (*ptr.add(FaceId::R.index()))
                .matrix_mut()
                .storage_mut()
                .raw_storage(),
            (*ptr.add(FaceId::L.index()))
                .matrix_mut()
                .storage_mut()
                .raw_storage(),
            (*ptr.add(FaceId::F.index()))
                .matrix_mut()
                .storage_mut()
                .raw_storage(),
            (*ptr.add(FaceId::B.index()))
                .matrix_mut()
                .storage_mut()
                .raw_storage(),
        ]
    }
}

fn face_commutator_moves(
    n: usize,
    destination: FaceId,
    helper: FaceId,
    rows: &[usize],
    columns: &[usize],
    slice_angle: MoveAngle,
) -> Vec<Move> {
    let mut moves = Vec::with_capacity((columns.len() + rows.len()) * 2 + 3);
    let reverse = slice_angle.inverse();

    for column in columns.iter().copied() {
        moves.push(face_layer_move(n, helper, column, reverse));
    }
    moves.push(face_layer_move(n, destination, 0, MoveAngle::Positive));
    for row in rows.iter().copied() {
        moves.push(face_layer_move(n, helper, row, reverse));
    }
    moves.push(face_layer_move(n, destination, 0, MoveAngle::Negative));
    for column in columns.iter().copied() {
        moves.push(face_layer_move(n, helper, column, slice_angle));
    }
    moves.push(face_layer_move(n, destination, 0, MoveAngle::Positive));
    for row in rows.iter().copied() {
        moves.push(face_layer_move(n, helper, row, slice_angle));
    }

    moves
}

fn normalized_face_commutator_moves(
    n: usize,
    destination: FaceId,
    helper: FaceId,
    rows: &[usize],
    columns: &[usize],
    slice_angle: MoveAngle,
) -> Vec<Move> {
    let mut moves = face_commutator_moves(n, destination, helper, rows, columns, slice_angle);
    moves.push(face_layer_move(n, destination, 0, MoveAngle::Positive).inverse());
    moves
}

fn validate_inner_layer_set(n: usize, layers: &[usize], name: &str) {
    let mut previous = None;
    for layer in layers.iter().copied() {
        assert!(
            layer > 0 && layer + 1 < n,
            "{name} must contain only inner layers"
        );
        if let Some(previous) = previous {
            assert!(previous < layer, "{name} must be strictly increasing");
        }
        previous = Some(layer);
    }
}

fn sorted_layer_sets_are_disjoint(left: &[usize], right: &[usize]) -> bool {
    let mut left_index = 0;
    let mut right_index = 0;

    while left_index < left.len() && right_index < right.len() {
        match left[left_index].cmp(&right[right_index]) {
            core::cmp::Ordering::Less => left_index += 1,
            core::cmp::Ordering::Greater => right_index += 1,
            core::cmp::Ordering::Equal => return false,
        }
    }

    true
}

fn expanded_face_commutator_difference_cycle(
    n: usize,
    destination: FaceId,
    helper: FaceId,
    row: usize,
    column: usize,
    slice_angle: MoveAngle,
) -> [(FacePosition, FacePosition); 3] {
    try_expanded_face_commutator_difference_cycle(n, destination, helper, row, column, slice_angle)
        .expect("face commutator must differ from the net face turn by exactly one 3-cycle")
}

fn normalized_face_commutator_difference_cycle(
    n: usize,
    destination: FaceId,
    helper: FaceId,
    row: usize,
    column: usize,
    slice_angle: MoveAngle,
) -> [(FacePosition, FacePosition); 3] {
    try_normalized_face_commutator_difference_cycle(
        n,
        destination,
        helper,
        row,
        column,
        slice_angle,
    )
    .expect("normalized face commutator must differ from identity by exactly one 3-cycle")
}

fn try_expanded_face_commutator_difference_cycle(
    n: usize,
    destination: FaceId,
    helper: FaceId,
    row: usize,
    column: usize,
    slice_angle: MoveAngle,
) -> Option<[(FacePosition, FacePosition); 3]> {
    let expanded =
        face_commutator_single_column_moves(n, destination, helper, row, column, slice_angle);
    let baseline = [face_layer_move(n, destination, 0, MoveAngle::Positive)];
    try_difference_cycle(n, row, column, &baseline, &expanded)
}

fn try_normalized_face_commutator_difference_cycle(
    n: usize,
    destination: FaceId,
    helper: FaceId,
    row: usize,
    column: usize,
    slice_angle: MoveAngle,
) -> Option<[(FacePosition, FacePosition); 3]> {
    let expanded =
        face_commutator_single_column_moves(n, destination, helper, row, column, slice_angle);
    let normalizing_inverse = face_layer_move(n, destination, 0, MoveAngle::Positive).inverse();
    let normalized = [
        expanded[0],
        expanded[1],
        expanded[2],
        expanded[3],
        expanded[4],
        expanded[5],
        expanded[6],
        normalizing_inverse,
    ];
    try_difference_cycle(n, row, column, &[], &normalized)
}

fn try_difference_cycle(
    n: usize,
    row: usize,
    column: usize,
    baseline: &[Move],
    expanded: &[Move],
) -> Option<[(FacePosition, FacePosition); 3]> {
    let (coordinates, coordinate_count) = unique_commutator_coordinates(n, row, column);
    let mut changed = [None; 3];
    let mut changed_count = 0;

    for face in FaceId::ALL {
        for row in coordinates.iter().take(coordinate_count).copied() {
            for col in coordinates.iter().take(coordinate_count).copied() {
                let position = FacePosition { face, row, col };
                let baseline_position = trace_position(n, position, baseline.iter().copied());
                let expanded_position = trace_position(n, position, expanded.iter().copied());
                if baseline_position != expanded_position {
                    if changed_count == changed.len() {
                        return None;
                    }
                    changed[changed_count] = Some((baseline_position, expanded_position));
                    changed_count += 1;
                }
            }
        }
    }

    if changed_count != 3 {
        return None;
    }

    let changed = changed.map(|entry| entry.expect("changed entry must be initialized"));
    if !positions_are_unique(changed.iter().map(|(from, _)| *from))
        || !positions_are_unique(changed.iter().map(|(_, to)| *to))
    {
        return None;
    }

    Some(changed)
}

fn face_commutator_single_column_moves(
    n: usize,
    destination: FaceId,
    helper: FaceId,
    row: usize,
    column: usize,
    slice_angle: MoveAngle,
) -> [Move; 7] {
    let reverse = slice_angle.inverse();

    [
        face_layer_move(n, helper, column, reverse),
        face_layer_move(n, destination, 0, MoveAngle::Positive),
        face_layer_move(n, helper, row, reverse),
        face_layer_move(n, destination, 0, MoveAngle::Negative),
        face_layer_move(n, helper, column, slice_angle),
        face_layer_move(n, destination, 0, MoveAngle::Positive),
        face_layer_move(n, helper, row, slice_angle),
    ]
}

fn unique_commutator_coordinates(n: usize, row: usize, column: usize) -> ([usize; 4], usize) {
    let mut coordinates = [0; 4];
    let mut len = 0;

    for coordinate in [row, column, n - 1 - row, n - 1 - column] {
        if !coordinates[..len].contains(&coordinate) {
            coordinates[len] = coordinate;
            len += 1;
        }
    }

    (coordinates, len)
}

fn assert_unique_positions(positions: impl IntoIterator<Item = FacePosition>) {
    assert!(
        positions_are_unique(positions),
        "face commutator generated overlapping sparse writes"
    );
}

fn positions_are_unique(positions: impl IntoIterator<Item = FacePosition>) -> bool {
    let mut seen = std::collections::HashSet::new();
    for position in positions {
        if !seen.insert(position) {
            return false;
        }
    }

    true
}

fn trace_position(
    n: usize,
    position: FacePosition,
    moves: impl IntoIterator<Item = Move>,
) -> FacePosition {
    moves.into_iter().fold(position, |position, mv| {
        trace_position_through_move(n, position, mv)
    })
}

fn trace_position_through_move(n: usize, mut position: FacePosition, mv: Move) -> FacePosition {
    if mv.depth == n - 1 && position.face == geometry::positive_axis_face(mv.axis) {
        position = rotate_face_position(position, n, mv.angle);
    } else if mv.depth == 0 && position.face == geometry::negative_axis_face(mv.axis) {
        position = rotate_face_position(position, n, mv.angle.inverse());
    }

    let specs = geometry::plan_positive_quarter_turn(mv.axis, mv.depth, n);
    for (index, spec) in specs.iter().copied().enumerate() {
        if let Some(offset) = strip_offset(position, spec, n) {
            let destination_index = (index + usize::from(mv.angle.as_u8())) % specs.len();
            return strip_position(specs[destination_index], offset, n);
        }
    }

    position
}

fn rotate_face_position(mut position: FacePosition, n: usize, angle: MoveAngle) -> FacePosition {
    for _ in 0..angle.as_u8() {
        position = FacePosition {
            face: position.face,
            row: position.col,
            col: n - 1 - position.row,
        };
    }
    position
}

fn strip_offset(position: FacePosition, spec: StripSpec, n: usize) -> Option<usize> {
    if position.face != spec.face {
        return None;
    }

    match spec.kind {
        crate::line::LineKind::Row if position.row == spec.index => Some(if spec.reversed {
            n - 1 - position.col
        } else {
            position.col
        }),
        crate::line::LineKind::Col if position.col == spec.index => Some(if spec.reversed {
            n - 1 - position.row
        } else {
            position.row
        }),
        _ => None,
    }
}

fn strip_position(spec: StripSpec, offset: usize, n: usize) -> FacePosition {
    let coordinate = if spec.reversed {
        n - 1 - offset
    } else {
        offset
    };
    match spec.kind {
        crate::line::LineKind::Row => FacePosition {
            face: spec.face,
            row: spec.index,
            col: coordinate,
        },
        crate::line::LineKind::Col => FacePosition {
            face: spec.face,
            row: coordinate,
            col: spec.index,
        },
    }
}

fn face_layer_move(n: usize, face: FaceId, depth_from_face: usize, angle: MoveAngle) -> Move {
    let last = n - 1;
    match face {
        FaceId::U => Move::new(Axis::Y, last - depth_from_face, angle),
        FaceId::D => Move::new(Axis::Y, depth_from_face, angle.inverse()),
        FaceId::R => Move::new(Axis::X, last - depth_from_face, angle),
        FaceId::L => Move::new(Axis::X, depth_from_face, angle.inverse()),
        FaceId::F => Move::new(Axis::Z, last - depth_from_face, angle),
        FaceId::B => Move::new(Axis::Z, depth_from_face, angle.inverse()),
    }
}

fn validate_edge_three_cycle(n: usize, cycle: EdgeThreeCycle) {
    match cycle.kind {
        EdgeThreeCycleKind::FrontRightWing { row } => {
            assert!(
                n >= 4,
                "front-right wing edge three-cycles require side length at least 4"
            );
            assert!(
                row > 0 && row + 1 < n,
                "edge three-cycle row must be an inner layer"
            );
            assert!(
                n % 2 == 0 || row != n / 2,
                "front-right wing edge three-cycle row cannot be the middle layer on odd cubes"
            );
        }
        EdgeThreeCycleKind::FrontRightMiddle { .. } => {
            assert!(
                n >= 3 && n % 2 == 1,
                "front-right middle edge three-cycles require odd side length"
            );
        }
    }
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

fn flip_right_edge_moves(n: usize) -> [Move; 7] {
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

fn unflip_right_edge_moves(n: usize) -> [Move; 7] {
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

fn move_sequence_updates(n: usize, moves: &[Move]) -> Option<Vec<FaceletUpdate>> {
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

fn unique_edge_cubies(
    n: usize,
    locations: impl IntoIterator<Item = FaceletLocation>,
) -> Option<[EdgeCubieLocation; 3]> {
    let mut cubies = [None; 3];
    let mut len = 0;

    for location in locations {
        let cubie = edge_cubie_location(n, location)?;
        if cubies[..len].contains(&Some(cubie)) {
            continue;
        }
        if len == cubies.len() {
            return None;
        }
        cubies[len] = Some(cubie);
        len += 1;
    }

    if len != cubies.len() {
        return None;
    }

    Some(cubies.map(|cubie| cubie.expect("edge cubie entry must be initialized")))
}

fn edge_facelet_orbit_index(n: usize, location: FaceletLocation) -> Option<usize> {
    if n < 3 || location.row >= n || location.col >= n {
        return None;
    }

    let offset = match (location.row, location.col) {
        (0, col) if col > 0 && col + 1 < n => col,
        (last_row, col) if last_row + 1 == n && col > 0 && col + 1 < n => col,
        (row, 0) if row > 0 && row + 1 < n => row,
        (row, last_col) if last_col + 1 == n && row > 0 && row + 1 < n => row,
        _ => return None,
    };

    Some(offset.min(n - 1 - offset))
}

fn edge_cubie_location(n: usize, location: FaceletLocation) -> Option<EdgeCubieLocation> {
    if n < 3 || location.row >= n || location.col >= n {
        return None;
    }

    let coord = geometry::logical_to_coord(location.face, location.row, location.col, n);
    let mut boundary_faces = [None; 3];
    let mut len = 0;

    if coord.x == 0 {
        boundary_faces[len] = Some(FaceId::L);
        len += 1;
    } else if coord.x + 1 == n {
        boundary_faces[len] = Some(FaceId::R);
        len += 1;
    }

    if coord.y == 0 {
        boundary_faces[len] = Some(FaceId::D);
        len += 1;
    } else if coord.y + 1 == n {
        boundary_faces[len] = Some(FaceId::U);
        len += 1;
    }

    if coord.z == 0 {
        boundary_faces[len] = Some(FaceId::B);
        len += 1;
    } else if coord.z + 1 == n {
        boundary_faces[len] = Some(FaceId::F);
        len += 1;
    }

    if len != 2 {
        return None;
    }

    let first_face = boundary_faces[0]?;
    let second_face = boundary_faces[1]?;
    let other_face = if location.face == first_face {
        second_face
    } else if location.face == second_face {
        first_face
    } else {
        return None;
    };
    let (other_row, other_col) = geometry::coord_to_logical(other_face, coord, n);
    let other = FaceletLocation {
        face: other_face,
        row: other_row,
        col: other_col,
    };

    Some(canonical_edge_cubie(location, other))
}

fn canonical_edge_cubie(first: FaceletLocation, second: FaceletLocation) -> EdgeCubieLocation {
    if facelet_location_key(second) < facelet_location_key(first) {
        EdgeCubieLocation {
            stickers: [second, first],
        }
    } else {
        EdgeCubieLocation {
            stickers: [first, second],
        }
    }
}

fn facelet_location_key(location: FaceletLocation) -> (usize, usize, usize) {
    (location.face.index(), location.row, location.col)
}

fn facelet_locations_are_unique(locations: impl IntoIterator<Item = FaceletLocation>) -> bool {
    let mut seen = [None; 6];
    let mut len = 0;

    for location in locations {
        if seen[..len].contains(&Some(location)) {
            return false;
        }
        if len == seen.len() {
            return false;
        }
        seen[len] = Some(location);
        len += 1;
    }

    true
}

fn edge_cubie_sets_match(left: [EdgeCubieLocation; 3], right: [EdgeCubieLocation; 3]) -> bool {
    left.iter().all(|cubie| right.contains(cubie)) && right.iter().all(|cubie| left.contains(cubie))
}

fn edge_cubie_index(cubies: [EdgeCubieLocation; 3], target: EdgeCubieLocation) -> Option<usize> {
    cubies.iter().position(|cubie| *cubie == target)
}

fn random_move_angle<R: RandomSource>(rng: &mut R) -> MoveAngle {
    match (rng.next_u64() % 3) as u8 {
        0 => MoveAngle::Positive,
        1 => MoveAngle::Double,
        _ => MoveAngle::Negative,
    }
}

fn opposite_face(face: FaceId) -> FaceId {
    match face {
        FaceId::U => FaceId::D,
        FaceId::D => FaceId::U,
        FaceId::R => FaceId::L,
        FaceId::L => FaceId::R,
        FaceId::F => FaceId::B,
        FaceId::B => FaceId::F,
    }
}

const NET_FACE_GAP: &str = "   ";

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum NetLayer {
    Index(usize),
    Separator,
}

fn net_layers(n: usize) -> Vec<NetLayer> {
    if n <= 8 {
        return (0..n).map(NetLayer::Index).collect();
    }

    let mut layers = Vec::with_capacity(9);
    layers.extend((0..4).map(NetLayer::Index));
    layers.push(NetLayer::Separator);
    layers.extend((n - 4..n).map(NetLayer::Index));
    layers
}

fn net_face_width(cols: &[NetLayer]) -> usize {
    cols.len().saturating_add(cols.len().saturating_sub(1))
}

impl<S: FaceletArray> fmt::Display for Cube<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Cube(n={}, history={}, storage~{} bytes)",
            self.n,
            self.history.len(),
            self.estimated_storage_bytes()
        )?;
        for id in FaceId::ALL {
            writeln!(f, "  {}", self.face(id))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Byte, Byte3, FaceAngle, Nibble, RandomSource, ThreeBit, XorShift64};

    fn basic_singmaster_turn(side_length: usize, notation: &str) -> Move {
        let last = side_length - 1;

        match notation {
            "U" => Move::new(Axis::Y, last, MoveAngle::Positive),
            "U'" => Move::new(Axis::Y, last, MoveAngle::Negative),
            "U2" => Move::new(Axis::Y, last, MoveAngle::Double),
            "D" => Move::new(Axis::Y, 0, MoveAngle::Negative),
            "D'" => Move::new(Axis::Y, 0, MoveAngle::Positive),
            "D2" => Move::new(Axis::Y, 0, MoveAngle::Double),
            "R" => Move::new(Axis::X, last, MoveAngle::Positive),
            "R'" => Move::new(Axis::X, last, MoveAngle::Negative),
            "R2" => Move::new(Axis::X, last, MoveAngle::Double),
            "L" => Move::new(Axis::X, 0, MoveAngle::Negative),
            "L'" => Move::new(Axis::X, 0, MoveAngle::Positive),
            "L2" => Move::new(Axis::X, 0, MoveAngle::Double),
            "F" => Move::new(Axis::Z, last, MoveAngle::Positive),
            "F'" => Move::new(Axis::Z, last, MoveAngle::Negative),
            "F2" => Move::new(Axis::Z, last, MoveAngle::Double),
            "B" => Move::new(Axis::Z, 0, MoveAngle::Negative),
            "B'" => Move::new(Axis::Z, 0, MoveAngle::Positive),
            "B2" => Move::new(Axis::Z, 0, MoveAngle::Double),
            _ => panic!("unsupported basic Singmaster turn: {notation}"),
        }
    }

    fn random_moves(side_length: usize, count: usize, seed: u64) -> Vec<Move> {
        let mut rng = XorShift64::new(seed);
        let mut moves = Vec::with_capacity(count);

        for _ in 0..count {
            let axis = match rng.next_u64() % 3 {
                0 => Axis::X,
                1 => Axis::Y,
                _ => Axis::Z,
            };
            let depth = (rng.next_u64() as usize) % side_length;
            let angle = match rng.next_u64() % 3 {
                0 => MoveAngle::Positive,
                1 => MoveAngle::Double,
                _ => MoveAngle::Negative,
            };
            moves.push(Move::new(axis, depth, angle));
        }

        moves
    }

    fn patterned_cube<S: FaceletArray>(side_length: usize, seed: usize) -> Cube<S> {
        let mut cube = Cube::<S>::new_solved_with_threads(side_length, 1);

        for face in FaceId::ALL {
            for row in 0..side_length {
                for col in 0..side_length {
                    let raw =
                        (face.index() * 17 + row * 7 + col * 11 + seed * 5) % Facelet::ALL.len();
                    cube.face_mut(face)
                        .set(row, col, Facelet::from_u8(raw as u8));
                }
            }
        }

        cube
    }

    fn disjoint_inner_layer_set_pairs(side_length: usize) -> Vec<(Vec<usize>, Vec<usize>)> {
        let layers = (1..side_length - 1).collect::<Vec<_>>();
        let mut pairs = Vec::new();

        for mask in 0..3usize.pow(layers.len() as u32) {
            let mut rows = Vec::new();
            let mut columns = Vec::new();
            let mut remaining = mask;

            for layer in layers.iter().copied() {
                match remaining % 3 {
                    1 => rows.push(layer),
                    2 => columns.push(layer),
                    _ => {}
                }
                remaining /= 3;
            }

            pairs.push((rows, columns));
        }

        pairs
    }

    fn overlapping_inner_layer_set_pairs(side_length: usize) -> Vec<(Vec<usize>, Vec<usize>)> {
        let layers = (1..side_length - 1).collect::<Vec<_>>();
        let mut pairs = Vec::new();

        for mask in 0..4usize.pow(layers.len() as u32) {
            let mut rows = Vec::new();
            let mut columns = Vec::new();
            let mut remaining = mask;

            for layer in layers.iter().copied() {
                match remaining % 4 {
                    1 => rows.push(layer),
                    2 => columns.push(layer),
                    3 => {
                        rows.push(layer);
                        columns.push(layer);
                    }
                    _ => {}
                }
                remaining /= 4;
            }

            if !sorted_layer_sets_are_disjoint(&rows, &columns) {
                pairs.push((rows, columns));
            }
        }

        pairs
    }

    fn sparse_commutator_mapping_matches_expanded(
        side_length: usize,
        destination: FaceId,
        helper: FaceId,
        rows: &[usize],
        columns: &[usize],
        slice_angle: MoveAngle,
    ) -> bool {
        let expanded = super::face_commutator_moves(
            side_length,
            destination,
            helper,
            rows,
            columns,
            slice_angle,
        );
        let baseline = [super::face_layer_move(
            side_length,
            destination,
            0,
            MoveAngle::Positive,
        )];
        let mut sparse_cycles = Vec::new();

        for row in rows.iter().copied() {
            for column in columns.iter().copied() {
                let Some(cycle) = super::try_expanded_face_commutator_difference_cycle(
                    side_length,
                    destination,
                    helper,
                    row,
                    column,
                    slice_angle,
                ) else {
                    return false;
                };
                sparse_cycles.extend(cycle);
            }
        }

        if !super::positions_are_unique(sparse_cycles.iter().map(|(from, _)| *from))
            || !super::positions_are_unique(sparse_cycles.iter().map(|(_, to)| *to))
        {
            return false;
        }

        for face in FaceId::ALL {
            for row in 0..side_length {
                for col in 0..side_length {
                    let position = super::FacePosition { face, row, col };
                    let baseline_position = super::trace_position(side_length, position, baseline);
                    let sparse_position = sparse_cycles
                        .iter()
                        .find_map(|(from, to)| (*from == baseline_position).then_some(*to))
                        .unwrap_or(baseline_position);
                    let expanded_position =
                        super::trace_position(side_length, position, expanded.iter().copied());

                    if sparse_position != expanded_position {
                        return false;
                    }
                }
            }
        }

        true
    }

    fn sparse_commutator_mapping_matches_normalized(
        side_length: usize,
        destination: FaceId,
        helper: FaceId,
        rows: &[usize],
        columns: &[usize],
        slice_angle: MoveAngle,
    ) -> bool {
        let expanded = super::normalized_face_commutator_moves(
            side_length,
            destination,
            helper,
            rows,
            columns,
            slice_angle,
        );
        let mut sparse_cycles = Vec::new();

        for row in rows.iter().copied() {
            for column in columns.iter().copied() {
                let Some(cycle) = super::try_normalized_face_commutator_difference_cycle(
                    side_length,
                    destination,
                    helper,
                    row,
                    column,
                    slice_angle,
                ) else {
                    return false;
                };
                sparse_cycles.extend(cycle);
            }
        }

        if !super::positions_are_unique(sparse_cycles.iter().map(|(from, _)| *from))
            || !super::positions_are_unique(sparse_cycles.iter().map(|(_, to)| *to))
        {
            return false;
        }

        for face in FaceId::ALL {
            for row in 0..side_length {
                for col in 0..side_length {
                    let position = super::FacePosition { face, row, col };
                    let sparse_position = sparse_cycles
                        .iter()
                        .find_map(|(from, to)| (*from == position).then_some(*to))
                        .unwrap_or(position);
                    let expanded_position =
                        super::trace_position(side_length, position, expanded.iter().copied());

                    if sparse_position != expanded_position {
                        return false;
                    }
                }
            }
        }

        true
    }

    fn edge_three_cycle_specs(side_length: usize) -> Vec<EdgeThreeCycle> {
        let mut specs = Vec::new();

        if side_length % 2 == 1 && side_length >= 3 {
            for direction in EdgeThreeCycleDirection::ALL {
                specs.push(EdgeThreeCycle::front_right_middle(direction));
            }
        }

        if side_length >= 4 {
            for row in 1..side_length - 1 {
                if side_length % 2 == 1 && row == side_length / 2 {
                    continue;
                }
                specs.push(EdgeThreeCycle::front_right_wing(row));
            }
        }

        specs
    }

    fn slice_outer_edge_three_cycle_candidate_moves(
        side_length: usize,
        slice_face: FaceId,
        slice_depth_from_face: usize,
        outer_face: FaceId,
        slice_angle: MoveAngle,
    ) -> Vec<Move> {
        let slice_half = super::face_layer_move(
            side_length,
            slice_face,
            slice_depth_from_face,
            MoveAngle::Double,
        );
        let slice =
            super::face_layer_move(side_length, slice_face, slice_depth_from_face, slice_angle);
        let outer = super::face_layer_move(side_length, outer_face, 0, MoveAngle::Positive);
        let outer_half = super::face_layer_move(side_length, outer_face, 0, MoveAngle::Double);

        vec![
            slice_half,
            outer,
            slice,
            outer_half,
            slice.inverse(),
            outer,
            slice_half,
        ]
    }

    fn move_defined_edge_three_cycle_plans(side_length: usize) -> Vec<EdgeThreeCyclePlan> {
        let probe = Cube::<Byte>::new_solved_with_threads(side_length, 1);
        let mut plans = Vec::new();

        if side_length == 3 {
            for slice_face in FaceId::ALL {
                for outer_face in FaceId::ALL {
                    if outer_face == slice_face || outer_face == super::opposite_face(slice_face) {
                        continue;
                    }

                    for slice_angle in [MoveAngle::Positive, MoveAngle::Negative] {
                        let moves = slice_outer_edge_three_cycle_candidate_moves(
                            side_length,
                            slice_face,
                            1,
                            outer_face,
                            slice_angle,
                        );
                        if let Some(plan) = probe.try_edge_three_cycle_plan_from_moves(moves) {
                            plans.push(plan);
                        }
                    }
                }
            }
        }

        for cycle in edge_three_cycle_specs(side_length) {
            plans.push(probe.edge_three_cycle_plan(cycle));
        }

        plans
    }

    fn assert_cubes_match<A: FaceletArray, B: FaceletArray>(actual: &Cube<A>, expected: &Cube<B>) {
        assert_eq!(actual.side_len(), expected.side_len());

        for face in FaceId::ALL {
            assert_eq!(
                actual.face(face).rotation(),
                expected.face(face).rotation(),
                "face rotation mismatch on {face}"
            );

            for row in 0..actual.side_len() {
                for col in 0..actual.side_len() {
                    assert_eq!(
                        actual.face(face).get(row, col),
                        expected.face(face).get(row, col),
                        "facelet mismatch on {face} at ({row}, {col})"
                    );
                }
            }
        }
    }

    fn threaded_moves_match_linear<S: FaceletArray>() {
        let side_length = 65;
        let moves = random_moves(side_length, 12, 0x7A11_DA7A);
        let mut expected = Cube::<S>::new_solved(side_length);

        expected.apply_moves_untracked(moves.iter().copied());

        for thread_count in [1usize, 2, 4, 16] {
            let mut actual = Cube::<S>::new_solved(side_length);
            actual.apply_moves_untracked_with_threads(moves.iter().copied(), thread_count);

            assert_cubes_match(&actual, &expected);
        }
    }

    fn every_move_inverse_restores<S: FaceletArray>() {
        for n in 1..6 {
            for axis in [Axis::X, Axis::Y, Axis::Z] {
                for depth in 0..n {
                    for angle in MoveAngle::ALL {
                        let mv = Move::new(axis, depth, angle);
                        let mut cube = Cube::<S>::new_solved(n);
                        cube.apply_move_untracked(mv);
                        cube.apply_move_untracked(mv.inverse());
                        assert!(cube.is_solved(), "inverse failed for n={n}, move={mv:?}");
                    }
                }
            }
        }
    }

    fn exact_cube_storage_bytes<S: FaceletArray>(side_length: usize) -> usize {
        side_length
            .checked_mul(side_length)
            .map(S::storage_bytes_for_len)
            .and_then(|bytes_per_face| bytes_per_face.checked_mul(6))
            .expect("test cube storage estimate overflowed usize")
    }

    #[test]
    fn inverse_restores_byte() {
        every_move_inverse_restores::<Byte>();
    }

    #[test]
    fn inverse_restores_byte3() {
        every_move_inverse_restores::<Byte3>();
    }

    #[test]
    fn inverse_restores_nibble() {
        every_move_inverse_restores::<Nibble>();
    }

    #[test]
    fn inverse_restores_three_bit() {
        every_move_inverse_restores::<ThreeBit>();
    }

    #[test]
    fn cube_backends_agree_after_random_moves() {
        let side_length = 6;
        let moves = random_moves(side_length, 1_000, 0xC0DE_CAFE);

        let mut byte = Cube::<Byte>::new_solved(side_length);
        let mut byte3 = Cube::<Byte3>::new_solved(side_length);
        let mut nibble = Cube::<Nibble>::new_solved(side_length);
        let mut three_bit = Cube::<ThreeBit>::new_solved(side_length);

        byte.apply_moves_untracked(moves.iter().copied());
        byte3.apply_moves_untracked(moves.iter().copied());
        nibble.apply_moves_untracked(moves.iter().copied());
        three_bit.apply_moves_untracked(moves.iter().copied());

        assert_cubes_match(&byte3, &byte);
        assert_cubes_match(&nibble, &byte);
        assert_cubes_match(&three_bit, &byte);
    }

    #[test]
    fn optimized_face_commutators_match_expanded_moves_exhaustively() {
        for side_length in 3..=6 {
            for destination in FaceId::ALL {
                for helper in FaceId::ALL {
                    if helper == destination || helper == super::opposite_face(destination) {
                        continue;
                    }

                    for slice_angle in MoveAngle::ALL {
                        for (rows, columns) in disjoint_inner_layer_set_pairs(side_length) {
                            for seed in 0..2 {
                                let mut expected = patterned_cube::<Byte>(side_length, seed);
                                let moves = expected.face_commutator_moves(
                                    destination,
                                    helper,
                                    &rows,
                                    &columns,
                                    slice_angle,
                                );
                                expected
                                    .apply_moves_untracked_with_threads(moves.iter().copied(), 1);

                                let mut reference = patterned_cube::<Byte>(side_length, seed);
                                reference.apply_face_commutator_untracked_reference(
                                    destination,
                                    helper,
                                    &rows,
                                    &columns,
                                    slice_angle,
                                );
                                assert_cubes_match(&reference, &expected);

                                let mut actual = patterned_cube::<Byte>(side_length, seed);
                                actual.apply_face_commutator_untracked(
                                    destination,
                                    helper,
                                    &rows,
                                    &columns,
                                    slice_angle,
                                );

                                assert_cubes_match(&actual, &expected);

                                let mut normalized_expected =
                                    patterned_cube::<Byte>(side_length, seed);
                                let normalized_moves = normalized_expected
                                    .normalized_face_commutator_moves(
                                        destination,
                                        helper,
                                        &rows,
                                        &columns,
                                        slice_angle,
                                    );
                                normalized_expected.apply_moves_untracked_with_threads(
                                    normalized_moves.iter().copied(),
                                    1,
                                );

                                let mut normalized_actual =
                                    patterned_cube::<Byte>(side_length, seed);
                                normalized_actual.apply_normalized_face_commutator_untracked(
                                    destination,
                                    helper,
                                    &rows,
                                    &columns,
                                    slice_angle,
                                );

                                assert_cubes_match(&normalized_actual, &normalized_expected);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn edge_three_cycles_match_expanded_moves_exhaustively() {
        for side_length in 3..=6 {
            let plans = move_defined_edge_three_cycle_plans(side_length);
            assert!(
                !plans.is_empty(),
                "expected edge three-cycle plans for n={side_length}"
            );

            for plan in plans {
                let mut expected = patterned_cube::<Byte>(side_length, 17);
                expected.apply_moves_untracked_with_threads(plan.moves().iter().copied(), 1);

                let mut actual = patterned_cube::<Byte>(side_length, 17);
                assert_eq!(plan.updates().len(), 6);
                assert_eq!(plan.cubies().len(), 3);
                actual.apply_edge_three_cycle_plan_untracked(&plan);

                assert_cubes_match(&actual, &expected);
            }
        }
    }

    #[test]
    fn front_right_middle_edge_three_cycles_match_expanded_moves_for_larger_odd_sizes() {
        for side_length in [7usize, 9] {
            let probe = Cube::<Byte>::new_solved_with_threads(side_length, 1);

            for direction in EdgeThreeCycleDirection::ALL {
                let cycle = EdgeThreeCycle::front_right_middle(direction);
                let plan = probe.edge_three_cycle_plan(cycle);

                let mut expected = patterned_cube::<Byte>(side_length, 31);
                expected.apply_moves_untracked_with_threads(plan.moves().iter().copied(), 1);

                let mut actual = patterned_cube::<Byte>(side_length, 31);
                actual.apply_edge_three_cycle_plan_untracked(&plan);

                assert_cubes_match(&actual, &expected);
            }
        }
    }

    #[test]
    fn edge_three_cycle_direct_updates_only_declared_edge_cubies() {
        for side_length in 3..=6 {
            for plan in move_defined_edge_three_cycle_plans(side_length) {
                let before = patterned_cube::<Byte>(side_length, 23);
                let mut affected = std::collections::HashSet::new();

                for update in plan.updates() {
                    for location in [update.from, update.to] {
                        assert!(
                            super::edge_cubie_location(side_length, location).is_some(),
                            "edge three-cycle touched a non-edge location: {location:?}"
                        );
                        affected.insert(location);
                    }
                }

                assert_eq!(affected.len(), 6);

                let mut after = before.clone();
                after.apply_edge_three_cycle_plan_untracked(&plan);

                for face in FaceId::ALL {
                    assert_eq!(
                        after.face(face).rotation(),
                        before.face(face).rotation(),
                        "edge three-cycle direct apply changed face rotation metadata"
                    );

                    for row in 0..side_length {
                        for col in 0..side_length {
                            let location = FaceletLocation { face, row, col };
                            if !affected.contains(&location) {
                                assert_eq!(
                                    after.face(face).get(row, col),
                                    before.face(face).get(row, col),
                                    "edge three-cycle direct apply changed undeclared location {location:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn edge_three_cycles_work_for_all_storage_backends() {
        for side_length in [5usize, 6] {
            let probe = Cube::<Byte>::new_solved_with_threads(side_length, 1);

            for cycle in edge_three_cycle_specs(side_length) {
                let plan = probe.edge_three_cycle_plan(cycle);
                let mut byte = patterned_cube::<Byte>(side_length, 29);
                let mut byte3 = patterned_cube::<Byte3>(side_length, 29);
                let mut nibble = patterned_cube::<Nibble>(side_length, 29);
                let mut three_bit = patterned_cube::<ThreeBit>(side_length, 29);

                byte.apply_edge_three_cycle_plan_untracked(&plan);
                byte3.apply_edge_three_cycle_plan_untracked(&plan);
                nibble.apply_edge_three_cycle_plan_untracked(&plan);
                three_bit.apply_edge_three_cycle_plan_untracked(&plan);

                assert_cubes_match(&byte3, &byte);
                assert_cubes_match(&nibble, &byte);
                assert_cubes_match(&three_bit, &byte);
            }
        }
    }

    #[test]
    #[should_panic(expected = "edge three-cycle row must be an inner layer")]
    fn edge_three_cycle_rejects_outer_row() {
        let cube = Cube::<Byte>::new_solved_with_threads(4, 1);
        let cycle = EdgeThreeCycle::front_right_wing(0);
        cube.edge_three_cycle_plan(cycle);
    }

    #[test]
    #[should_panic(
        expected = "front-right wing edge three-cycle row cannot be the middle layer on odd cubes"
    )]
    fn edge_three_cycle_rejects_odd_middle_row() {
        let cube = Cube::<Byte>::new_solved_with_threads(5, 1);
        let cycle = EdgeThreeCycle::front_right_wing(2);
        cube.edge_three_cycle_plan(cycle);
    }

    #[test]
    #[should_panic(expected = "front-right middle edge three-cycles require odd side length")]
    fn edge_three_cycle_rejects_even_middle_cycle() {
        let cube = Cube::<Byte>::new_solved_with_threads(6, 1);
        let cycle = EdgeThreeCycle::front_right_middle(EdgeThreeCycleDirection::Positive);
        cube.edge_three_cycle_plan(cycle);
    }

    #[test]
    fn middle_edge_precheck_style_sequence_only_changes_edge_locations() {
        let n = 5;
        let middle = n / 2;
        let mut moves = Vec::new();
        moves.push(face_layer_move(n, FaceId::D, middle, MoveAngle::Positive));
        moves.extend(flip_right_edge_moves(n));
        moves.push(face_layer_move(n, FaceId::D, middle, MoveAngle::Negative));
        moves.extend(unflip_right_edge_moves(n));

        let updates = move_sequence_updates(n, &moves).expect("probe sequence must be valid");
        assert!(!updates.is_empty());
        assert!(
            updates
                .iter()
                .all(|update| edge_cubie_location(n, update.from).is_some()
                    && edge_cubie_location(n, update.to).is_some()),
            "precheck-style sequence must stay edge-only",
        );
    }

    #[test]
    fn parity_fix_style_sequence_only_changes_edge_locations() {
        let n = 6;
        let row = 1usize;
        let moves = vec![
            face_layer_move(n, FaceId::D, row, MoveAngle::Negative),
            face_layer_move(n, FaceId::R, 0, MoveAngle::Double),
            face_layer_move(n, FaceId::U, row, MoveAngle::Positive),
            face_layer_move(n, FaceId::F, 0, MoveAngle::Double),
            face_layer_move(n, FaceId::U, row, MoveAngle::Negative),
            face_layer_move(n, FaceId::F, 0, MoveAngle::Double),
            face_layer_move(n, FaceId::D, row, MoveAngle::Double),
            face_layer_move(n, FaceId::R, 0, MoveAngle::Double),
            face_layer_move(n, FaceId::D, row, MoveAngle::Positive),
            face_layer_move(n, FaceId::R, 0, MoveAngle::Double),
            face_layer_move(n, FaceId::D, row, MoveAngle::Negative),
            face_layer_move(n, FaceId::R, 0, MoveAngle::Double),
            face_layer_move(n, FaceId::F, 0, MoveAngle::Double),
            face_layer_move(n, FaceId::D, row, MoveAngle::Double),
            face_layer_move(n, FaceId::F, 0, MoveAngle::Double),
        ];

        let updates = move_sequence_updates(n, &moves).expect("probe sequence must be valid");
        assert!(!updates.is_empty());
        assert!(
            updates
                .iter()
                .any(|update| edge_cubie_location(n, update.from).is_none()
                    || edge_cubie_location(n, update.to).is_none()),
            "parity-fix sequence is expected to touch non-edge locations and must stay as literal moves",
        );
    }

    #[test]
    fn direct_face_commutators_work_for_all_storage_backends() {
        let side_length = 7;
        let rows = [1usize, 4];
        let columns = [2usize, 3, 5];

        for destination in FaceId::ALL {
            for helper in FaceId::ALL {
                if helper == destination || helper == super::opposite_face(destination) {
                    continue;
                }

                for slice_angle in MoveAngle::ALL {
                    let commutator = FaceCommutator::new(destination, helper, slice_angle);
                    let mut byte = patterned_cube::<Byte>(side_length, 3);
                    let mut byte3 = patterned_cube::<Byte3>(side_length, 3);
                    let mut nibble = patterned_cube::<Nibble>(side_length, 3);
                    let mut three_bit = patterned_cube::<ThreeBit>(side_length, 3);

                    byte.apply_face_commutator_plan_untracked(commutator, &rows, &columns);
                    byte3.apply_face_commutator_plan_untracked(commutator, &rows, &columns);
                    nibble.apply_face_commutator_plan_untracked(commutator, &rows, &columns);
                    three_bit.apply_face_commutator_plan_untracked(commutator, &rows, &columns);

                    assert_cubes_match(&byte3, &byte);
                    assert_cubes_match(&nibble, &byte);
                    assert_cubes_match(&three_bit, &byte);

                    let mut byte = patterned_cube::<Byte>(side_length, 5);
                    let mut byte3 = patterned_cube::<Byte3>(side_length, 5);
                    let mut nibble = patterned_cube::<Nibble>(side_length, 5);
                    let mut three_bit = patterned_cube::<ThreeBit>(side_length, 5);

                    byte.apply_normalized_face_commutator_plan_untracked(
                        commutator, &rows, &columns,
                    );
                    byte3.apply_normalized_face_commutator_plan_untracked(
                        commutator, &rows, &columns,
                    );
                    nibble.apply_normalized_face_commutator_plan_untracked(
                        commutator, &rows, &columns,
                    );
                    three_bit.apply_normalized_face_commutator_plan_untracked(
                        commutator, &rows, &columns,
                    );

                    assert_cubes_match(&byte3, &byte);
                    assert_cubes_match(&nibble, &byte);
                    assert_cubes_match(&three_bit, &byte);
                }
            }
        }
    }

    #[test]
    fn normalized_face_commutator_only_changes_declared_center_positions() {
        let side_length = 9;
        let rows = [1usize, 4, 7];
        let columns = [2usize, 3, 5, 6];

        for destination in FaceId::ALL {
            for helper in FaceId::ALL {
                if helper == destination || helper == super::opposite_face(destination) {
                    continue;
                }

                for slice_angle in MoveAngle::ALL {
                    let commutator = FaceCommutator::new(destination, helper, slice_angle);
                    let before = patterned_cube::<Byte>(side_length, 9);
                    let mut after = before.clone();
                    let mut affected = std::collections::HashSet::new();

                    for row in rows {
                        for column in columns {
                            let updates = before
                                .normalized_face_commutator_sparse_updates(commutator, row, column);
                            for update in updates {
                                for location in [update.from, update.to] {
                                    assert!(
                                        location.row > 0
                                            && location.row + 1 < side_length
                                            && location.col > 0
                                            && location.col + 1 < side_length,
                                        "normalized commutator touched a non-center location: {location:?}"
                                    );
                                    affected.insert(location);
                                }
                            }
                        }
                    }

                    assert_eq!(affected.len(), rows.len() * columns.len() * 3);

                    after.apply_normalized_face_commutator_plan_untracked(
                        commutator, &rows, &columns,
                    );

                    for face in FaceId::ALL {
                        assert_eq!(
                            after.face(face).rotation(),
                            before.face(face).rotation(),
                            "normalized commutator changed face rotation metadata"
                        );

                        for row in 0..side_length {
                            for col in 0..side_length {
                                let location = FaceletLocation { face, row, col };
                                if !affected.contains(&location) {
                                    assert_eq!(
                                        after.face(face).get(row, col),
                                        before.face(face).get(row, col),
                                        "normalized commutator changed undeclared location {location:?}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn overlapping_row_and_column_sets_cannot_extend_sparse_commutator_family() {
        for side_length in 3..=6 {
            for destination in FaceId::ALL {
                for helper in FaceId::ALL {
                    if helper == destination || helper == super::opposite_face(destination) {
                        continue;
                    }

                    for slice_angle in MoveAngle::ALL {
                        for (rows, columns) in overlapping_inner_layer_set_pairs(side_length) {
                            assert!(
                                !sparse_commutator_mapping_matches_expanded(
                                    side_length,
                                    destination,
                                    helper,
                                    &rows,
                                    &columns,
                                    slice_angle,
                                ),
                                "overlapping row/column sets unexpectedly matched for n={side_length}, destination={destination}, helper={helper}, angle={slice_angle}, rows={rows:?}, columns={columns:?}"
                            );
                            assert!(
                                !sparse_commutator_mapping_matches_normalized(
                                    side_length,
                                    destination,
                                    helper,
                                    &rows,
                                    &columns,
                                    slice_angle,
                                ),
                                "overlapping row/column sets unexpectedly matched normalized commutator for n={side_length}, destination={destination}, helper={helper}, angle={slice_angle}, rows={rows:?}, columns={columns:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    #[should_panic(expected = "destination and helper faces must be perpendicular")]
    fn face_commutator_rejects_parallel_helper_face() {
        let mut cube = Cube::<Byte>::new_solved_with_threads(4, 1);
        cube.apply_face_commutator_untracked(FaceId::U, FaceId::D, &[1], &[2], MoveAngle::Positive);
    }

    #[test]
    #[should_panic(expected = "destination and helper faces must be perpendicular")]
    fn normalized_face_commutator_rejects_parallel_helper_face() {
        let mut cube = Cube::<Byte>::new_solved_with_threads(4, 1);
        cube.apply_normalized_face_commutator_untracked(
            FaceId::U,
            FaceId::D,
            &[1],
            &[2],
            MoveAngle::Positive,
        );
    }

    #[test]
    #[should_panic(expected = "commutator row and column layer sets must be disjoint")]
    fn face_commutator_rejects_same_row_and_column_layer() {
        let mut cube = Cube::<Byte>::new_solved_with_threads(4, 1);
        cube.apply_face_commutator_untracked(FaceId::U, FaceId::R, &[1], &[1], MoveAngle::Positive);
    }

    #[test]
    #[should_panic(expected = "commutator row and column layer sets must be disjoint")]
    fn normalized_face_commutator_rejects_same_row_and_column_layer() {
        let mut cube = Cube::<Byte>::new_solved_with_threads(4, 1);
        cube.apply_normalized_face_commutator_untracked(
            FaceId::U,
            FaceId::R,
            &[1],
            &[1],
            MoveAngle::Positive,
        );
    }

    #[test]
    fn sorted_layer_set_disjointness_is_linear_merge_compatible() {
        assert!(sorted_layer_sets_are_disjoint(&[], &[]));
        assert!(sorted_layer_sets_are_disjoint(&[1, 3, 5], &[2, 4, 6]));
        assert!(sorted_layer_sets_are_disjoint(&[1, 2, 3], &[]));
        assert!(sorted_layer_sets_are_disjoint(&[], &[1, 2, 3]));
        assert!(!sorted_layer_sets_are_disjoint(&[1, 3, 5], &[0, 3, 6]));
        assert!(!sorted_layer_sets_are_disjoint(&[1, 2, 3], &[3, 4, 5]));
    }

    #[test]
    fn threaded_byte_moves_match_linear() {
        threaded_moves_match_linear::<Byte>();
    }

    #[test]
    fn threaded_nibble_moves_match_linear() {
        threaded_moves_match_linear::<Nibble>();
    }

    #[test]
    fn threaded_three_bit_moves_match_linear() {
        threaded_moves_match_linear::<ThreeBit>();
    }

    #[test]
    fn threaded_byte3_moves_match_linear() {
        threaded_moves_match_linear::<Byte3>();
    }

    #[test]
    fn cube_storage_estimates_are_exact() {
        for side_length in [1usize, 2, 3, 4, 5, 8, 9, 10, 17] {
            assert_eq!(
                Cube::<Byte>::new_solved(side_length).estimated_storage_bytes(),
                exact_cube_storage_bytes::<Byte>(side_length)
            );
            assert_eq!(
                Cube::<Byte3>::new_solved(side_length).estimated_storage_bytes(),
                exact_cube_storage_bytes::<Byte3>(side_length)
            );
            assert_eq!(
                Cube::<Nibble>::new_solved(side_length).estimated_storage_bytes(),
                exact_cube_storage_bytes::<Nibble>(side_length)
            );
            assert_eq!(
                Cube::<ThreeBit>::new_solved(side_length).estimated_storage_bytes(),
                exact_cube_storage_bytes::<ThreeBit>(side_length)
            );
        }
    }

    #[test]
    fn four_quarter_turns_restore() {
        for n in 1..6 {
            for axis in [Axis::X, Axis::Y, Axis::Z] {
                for depth in 0..n {
                    let mv = Move::new(axis, depth, MoveAngle::Positive);
                    let mut cube = Cube::<Byte>::new_solved(n);
                    for _ in 0..4 {
                        cube.apply_move_untracked(mv);
                    }
                    assert!(cube.is_solved(), "four turns failed for n={n}, move={mv:?}");
                }
            }
        }
    }

    #[test]
    fn tracked_moves_enter_history() {
        let mut cube = Cube::<Byte>::new_solved(3);
        cube.apply_move(Move::new(Axis::Z, 2, MoveAngle::Positive));
        assert_eq!(cube.history().len(), 1);
    }

    #[test]
    fn random_move_stays_within_cube_bounds() {
        let side_length = 11;
        let cube = Cube::<Byte>::new_solved(side_length);
        let mut rng = XorShift64::new(0x5C4A_4B1E);

        for _ in 0..1_000 {
            let mv = cube.random_move(&mut rng);
            assert!(mv.depth < side_length, "random move depth out of bounds");
        }
    }

    #[test]
    fn scramble_applies_six_rounds_of_random_moves_and_outer_face_turns() {
        let side_length = 5;
        let seed = 0x5C4A_2B1E;

        let mut actual = Cube::<Byte>::new_solved(side_length);
        let mut actual_rng = XorShift64::new(seed);
        actual.scramble(&mut actual_rng);

        let mut expected = Cube::<Byte>::new_solved(side_length);
        let mut expected_rng = XorShift64::new(seed);
        for _ in 0..DEFAULT_SCRAMBLE_ROUNDS {
            expected.scramble_random_moves(&mut expected_rng, side_length);

            for face in FaceId::ALL {
                let mv = expected.random_outer_face_move(face, &mut expected_rng);
                expected.apply_move(mv);
            }
        }

        assert_eq!(
            actual.history().len(),
            DEFAULT_SCRAMBLE_ROUNDS * (side_length + FaceId::ALL.len())
        );
        assert_cubes_match(&actual, &expected);
        assert_eq!(actual.history().as_slice(), expected.history().as_slice());
    }

    #[test]
    fn basic_singmaster_turns_match_our_move_notation() {
        let side_length = 5;
        let last = side_length - 1;

        let cases = [
            ("U", Axis::Y, last, MoveAngle::Positive),
            ("U'", Axis::Y, last, MoveAngle::Negative),
            ("U2", Axis::Y, last, MoveAngle::Double),
            ("D", Axis::Y, 0, MoveAngle::Negative),
            ("D'", Axis::Y, 0, MoveAngle::Positive),
            ("D2", Axis::Y, 0, MoveAngle::Double),
            ("R", Axis::X, last, MoveAngle::Positive),
            ("R'", Axis::X, last, MoveAngle::Negative),
            ("R2", Axis::X, last, MoveAngle::Double),
            ("L", Axis::X, 0, MoveAngle::Negative),
            ("L'", Axis::X, 0, MoveAngle::Positive),
            ("L2", Axis::X, 0, MoveAngle::Double),
            ("F", Axis::Z, last, MoveAngle::Positive),
            ("F'", Axis::Z, last, MoveAngle::Negative),
            ("F2", Axis::Z, last, MoveAngle::Double),
            ("B", Axis::Z, 0, MoveAngle::Negative),
            ("B'", Axis::Z, 0, MoveAngle::Positive),
            ("B2", Axis::Z, 0, MoveAngle::Double),
        ];

        for (notation, axis, depth, angle) in cases {
            assert_eq!(
                basic_singmaster_turn(side_length, notation),
                Move::new(axis, depth, angle),
                "unexpected move notation for {notation}"
            );
        }
    }

    #[test]
    fn basic_singmaster_prime_and_double_turns_match_inverse_rules() {
        let side_length = 5;
        let cases = [
            ("U", "U'", "U2"),
            ("D", "D'", "D2"),
            ("R", "R'", "R2"),
            ("L", "L'", "L2"),
            ("F", "F'", "F2"),
            ("B", "B'", "B2"),
        ];

        for (turn, prime, double) in cases {
            let turn_move = basic_singmaster_turn(side_length, turn);
            let prime_move = basic_singmaster_turn(side_length, prime);
            let double_move = basic_singmaster_turn(side_length, double);

            assert_eq!(
                prime_move,
                turn_move.inverse(),
                "{prime} should invert {turn}"
            );
            assert_eq!(
                double_move,
                double_move.inverse(),
                "{double} should be self-inverse"
            );

            let mut cube = Cube::<Byte>::new_solved(side_length);
            cube.apply_move_untracked(turn_move);
            cube.apply_move_untracked(prime_move);
            assert!(
                cube.is_solved(),
                "{turn} followed by {prime} should restore"
            );

            let mut cube = Cube::<Byte>::new_solved(side_length);
            cube.apply_move_untracked(double_move);
            cube.apply_move_untracked(double_move);
            assert!(cube.is_solved(), "{double} twice should restore");
        }
    }

    #[test]
    fn outer_face_rotation_matches_axis_move_direction() {
        let mut cube = Cube::<Byte>::new_solved(3);

        cube.apply_move_untracked(Move::new(Axis::Z, 2, MoveAngle::Positive));
        assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(1));

        cube.apply_move_untracked(Move::new(Axis::Z, 0, MoveAngle::Positive));
        assert_eq!(cube.face(FaceId::B).rotation(), FaceAngle::new(3));

        cube.apply_move_untracked(Move::new(Axis::X, 2, MoveAngle::Negative));
        assert_eq!(cube.face(FaceId::R).rotation(), FaceAngle::new(3));

        cube.apply_move_untracked(Move::new(Axis::X, 0, MoveAngle::Double));
        assert_eq!(cube.face(FaceId::L).rotation(), FaceAngle::new(2));
    }

    #[test]
    fn face_rotation_accumulates_angles_modulo_four() {
        let mut cube = Cube::<Byte>::new_solved(3);

        assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(0));

        cube.apply_move_untracked(Move::new(Axis::Z, 2, MoveAngle::Positive));
        assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(1));

        cube.apply_move_untracked(Move::new(Axis::Z, 2, MoveAngle::Positive));
        assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(2));

        cube.apply_move_untracked(Move::new(Axis::Z, 2, MoveAngle::Positive));
        assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(3));

        cube.apply_move_untracked(Move::new(Axis::Z, 2, MoveAngle::Positive));
        assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(0));
    }

    #[test]
    fn net_uses_traditional_geometry() {
        let cube = Cube::<Byte>::new_solved(2);

        assert_eq!(
            cube.net_string(),
            concat!(
                "Cube(n=2, history=0, storage~24 bytes)\n",
                "      W W\n",
                "      W W\n",
                "\n",
                "O O   G G   R R   B B\n",
                "O O   G G   R R   B B\n",
                "\n",
                "      Y Y\n",
                "      Y Y\n",
            )
        );
    }

    #[test]
    fn net_keeps_unfolded_face_orientations() {
        let mut cube = Cube::<Byte>::new_solved(3);

        for row in 0..3 {
            for col in 0..3 {
                cube.face_mut(FaceId::U)
                    .set(row, col, Facelet::from_u8(row as u8));
                cube.face_mut(FaceId::D)
                    .set(row, col, Facelet::from_u8((2 - row) as u8));
                cube.face_mut(FaceId::F)
                    .set(row, col, Facelet::from_u8(col as u8));
                cube.face_mut(FaceId::B)
                    .set(row, col, Facelet::from_u8((2 - col) as u8));
                cube.face_mut(FaceId::L)
                    .set(row, col, Facelet::from_u8((row + col) as u8));
                cube.face_mut(FaceId::R)
                    .set(row, col, Facelet::from_u8((row + 2 - col) as u8));
            }
        }

        assert_eq!(
            cube.net_string(),
            concat!(
                "Cube(n=3, history=0, storage~54 bytes)\n",
                "        W W W\n",
                "        Y Y Y\n",
                "        R R R\n",
                "\n",
                "W Y R   W Y R   R Y W   R Y W\n",
                "Y R O   W Y R   O R Y   R Y W\n",
                "R O G   W Y R   G O R   R Y W\n",
                "\n",
                "        R R R\n",
                "        Y Y Y\n",
                "        W W W\n",
            )
        );
    }

    #[test]
    fn net_prints_full_small_faces() {
        let cube = Cube::<Byte>::new_solved(4);
        let net = cube.net_string();

        assert!(!net.contains("..."));
        assert!(net.contains("W W W W"));
        assert!(net.contains("O O O O   G G G G   R R R R   B B B B"));
    }

    #[test]
    fn net_prints_full_large_faces_without_ellipsis_markers() {
        let cube = Cube::<Byte>::new_solved(8);
        let net = cube.net_string();

        assert!(!net.contains("..."));
        assert!(!net.contains("-"));
        assert!(net.contains("                  W W W W W W W W\n"));
        assert!(
            net.contains("O O O O O O O O   G G G G G G G G   R R R R R R R R   B B B B B B B B\n")
        );
        assert!(net.contains("                  Y Y Y Y Y Y Y Y\n"));
    }

    #[test]
    fn net_limits_large_faces_to_outer_four_layers_with_separator() {
        let mut cube = Cube::<Byte>::new_solved(10);

        cube.face_mut(FaceId::U).set(0, 0, Facelet::Red);
        cube.face_mut(FaceId::U).set(0, 3, Facelet::Green);
        cube.face_mut(FaceId::U).set(0, 4, Facelet::Orange);
        cube.face_mut(FaceId::U).set(0, 5, Facelet::Yellow);
        cube.face_mut(FaceId::U).set(0, 6, Facelet::Blue);
        cube.face_mut(FaceId::U).set(0, 9, Facelet::Red);

        let net = cube.net_string();

        assert!(!net.contains("..."));
        assert!(net.contains("                    R W W G - B W W R\n"));
        assert!(net.contains("                    - - - - - - - - -\n"));
        assert!(net.contains(
            "O O O O - O O O O   G G G G - G G G G   R R R R - R R R R   B B B B - B B B B\n"
        ));
        assert!(net.contains(
            "- - - - - - - - -   - - - - - - - - -   - - - - - - - - -   - - - - - - - - -\n"
        ));
    }
}

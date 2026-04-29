use core::fmt;

use crate::{
    conventions::{face_layer_move, opposite_face},
    cube::Cube,
    cube::EdgeThreeCycleValidationError,
    face::FaceId,
    moves::{Move, MoveAngle},
    simulation::derived::{facelet_location, trace_position, FacePosition, FaceletUpdate},
    storage::FaceletArray,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FaceCommutator {
    destination: FaceId,
    helper: FaceId,
    slice_angle: MoveAngle,
    expanded_template: CenterCommutatorTemplate,
    normalized_template: CenterCommutatorTemplate,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FaceCommutatorMode {
    Expanded,
    Normalized,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FaceCommutatorLayers<'a> {
    rows: &'a [usize],
    columns: &'a [usize],
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FaceCommutatorPlan<'a> {
    side_length: usize,
    commutator: FaceCommutator,
    mode: FaceCommutatorMode,
    layers: FaceCommutatorLayers<'a>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LayerSetKind {
    Rows,
    Columns,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LayerSetValidationError {
    MustContainOnlyInnerLayers { set: LayerSetKind },
    MustBeStrictlyIncreasing { set: LayerSetKind },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FaceCommutatorValidationError {
    CubeTooSmall,
    DestinationAndHelperMustDiffer,
    DestinationAndHelperMustBePerpendicular,
    InvalidLayerSet(LayerSetValidationError),
    RowAndColumnSetsMustBeDisjoint,
}
impl<'a> FaceCommutatorLayers<'a> {
    pub const fn new(rows: &'a [usize], columns: &'a [usize]) -> Self {
        Self { rows, columns }
    }

    pub const fn rows(self) -> &'a [usize] {
        self.rows
    }

    pub const fn columns(self) -> &'a [usize] {
        self.columns
    }

    pub fn try_validate(
        self,
        side_length: usize,
        commutator: FaceCommutator,
    ) -> Result<(), FaceCommutatorValidationError> {
        try_validate_face_commutator(
            side_length,
            commutator.destination,
            commutator.helper,
            self.rows,
            self.columns,
        )
    }
}

impl<'a> FaceCommutatorPlan<'a> {
    pub fn new(
        side_length: usize,
        commutator: FaceCommutator,
        mode: FaceCommutatorMode,
        rows: &'a [usize],
        columns: &'a [usize],
    ) -> Self {
        Self::try_new(side_length, commutator, mode, rows, columns)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    pub fn try_new(
        side_length: usize,
        commutator: FaceCommutator,
        mode: FaceCommutatorMode,
        rows: &'a [usize],
        columns: &'a [usize],
    ) -> Result<Self, FaceCommutatorValidationError> {
        let layers = FaceCommutatorLayers::new(rows, columns);
        try_validate_face_commutator(
            side_length,
            commutator.destination,
            commutator.helper,
            rows,
            columns,
        )?;

        Ok(Self {
            side_length,
            commutator,
            mode,
            layers,
        })
    }

    pub const fn side_length(self) -> usize {
        self.side_length
    }

    pub const fn commutator(self) -> FaceCommutator {
        self.commutator
    }

    pub const fn mode(self) -> FaceCommutatorMode {
        self.mode
    }

    pub const fn layers(self) -> FaceCommutatorLayers<'a> {
        self.layers
    }

    pub fn try_validate(self) -> Result<(), FaceCommutatorValidationError> {
        self.layers.try_validate(self.side_length, self.commutator)
    }

    pub fn is_valid(self) -> bool {
        self.try_validate().is_ok()
    }

    pub fn literal_move_count(self) -> usize {
        let base = 2 * self.layers.rows.len() + 2 * self.layers.columns.len() + 3;
        match self.mode {
            FaceCommutatorMode::Expanded => base,
            FaceCommutatorMode::Normalized => base + 1,
        }
    }

    pub fn for_each_literal_move(self, mut f: impl FnMut(Move)) {
        for_each_face_commutator_move(
            self.side_length,
            self.commutator.destination,
            self.commutator.helper,
            self.layers.rows,
            self.layers.columns,
            self.commutator.slice_angle,
            &mut f,
        );

        if self.mode == FaceCommutatorMode::Normalized {
            f(face_layer_move(
                self.side_length,
                self.commutator.destination,
                0,
                MoveAngle::Positive,
            )
            .inverse());
        }
    }

    pub fn literal_moves(self) -> Vec<Move> {
        let mut moves = Vec::with_capacity(self.literal_move_count());
        self.for_each_literal_move(|mv| moves.push(mv));
        moves
    }

    pub fn sparse_updates(self, row: usize, column: usize) -> [FaceletUpdate; 3] {
        debug_assert!(self.layers.rows.contains(&row));
        debug_assert!(self.layers.columns.contains(&column));

        let template = match self.mode {
            FaceCommutatorMode::Expanded => self.commutator.expanded_template,
            FaceCommutatorMode::Normalized => self.commutator.normalized_template,
        };

        template.updates.map(|update| FaceletUpdate {
            from: facelet_location(update.from.eval(self.side_length, row, column)),
            to: facelet_location(update.to.eval(self.side_length, row, column)),
        })
    }
}

impl fmt::Display for FaceCommutatorValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CubeTooSmall => f.write_str("face commutators require side length at least 3"),
            Self::DestinationAndHelperMustDiffer => {
                f.write_str("destination and helper faces must differ")
            }
            Self::DestinationAndHelperMustBePerpendicular => {
                f.write_str("destination and helper faces must be perpendicular")
            }
            Self::InvalidLayerSet(error) => error.fmt(f),
            Self::RowAndColumnSetsMustBeDisjoint => {
                f.write_str("commutator row and column layer sets must be disjoint")
            }
        }
    }
}

impl fmt::Display for LayerSetValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MustContainOnlyInnerLayers {
                set: LayerSetKind::Rows,
            } => f.write_str("commutator rows must contain only inner layers"),
            Self::MustContainOnlyInnerLayers {
                set: LayerSetKind::Columns,
            } => f.write_str("commutator columns must contain only inner layers"),
            Self::MustBeStrictlyIncreasing {
                set: LayerSetKind::Rows,
            } => f.write_str("commutator rows must be strictly increasing"),
            Self::MustBeStrictlyIncreasing {
                set: LayerSetKind::Columns,
            } => f.write_str("commutator columns must be strictly increasing"),
        }
    }
}

impl fmt::Display for EdgeThreeCycleValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WingCycleRequiresSideLengthAtLeastFour => {
                f.write_str("front-right wing edge three-cycles require side length at least 4")
            }
            Self::RowMustBeInnerLayer => f.write_str("edge three-cycle row must be an inner layer"),
            Self::WingRowCannotBeOddMiddleLayer => f.write_str(
                "front-right wing edge three-cycle row cannot be the middle layer on odd cubes",
            ),
            Self::MiddleCycleRequiresOddSideLengthAtLeastThree => {
                f.write_str("front-right middle edge three-cycles require odd side length")
            }
        }
    }
}

impl FaceCommutator {
    pub fn try_new(
        destination: FaceId,
        helper: FaceId,
        slice_angle: MoveAngle,
    ) -> Result<Self, FaceCommutatorValidationError> {
        try_validate_face_commutator_faces(destination, helper)?;

        Ok(Self {
            destination,
            helper,
            slice_angle,
            expanded_template: CenterCommutatorTemplate::expanded(destination, helper, slice_angle),
            normalized_template: CenterCommutatorTemplate::normalized(
                destination,
                helper,
                slice_angle,
            ),
        })
    }

    pub fn new(destination: FaceId, helper: FaceId, slice_angle: MoveAngle) -> Self {
        Self::try_new(destination, helper, slice_angle).unwrap_or_else(|error| panic!("{error}"))
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

impl<S: FaceletArray> Cube<S> {
    pub fn face_commutator_plan<'a>(
        &self,
        commutator: FaceCommutator,
        rows: &'a [usize],
        columns: &'a [usize],
    ) -> FaceCommutatorPlan<'a> {
        FaceCommutatorPlan::new(
            self.n,
            commutator,
            FaceCommutatorMode::Expanded,
            rows,
            columns,
        )
    }

    pub fn try_face_commutator_plan<'a>(
        &self,
        commutator: FaceCommutator,
        rows: &'a [usize],
        columns: &'a [usize],
    ) -> Result<FaceCommutatorPlan<'a>, FaceCommutatorValidationError> {
        FaceCommutatorPlan::try_new(
            self.n,
            commutator,
            FaceCommutatorMode::Expanded,
            rows,
            columns,
        )
    }

    pub fn normalized_face_commutator_plan<'a>(
        &self,
        commutator: FaceCommutator,
        rows: &'a [usize],
        columns: &'a [usize],
    ) -> FaceCommutatorPlan<'a> {
        FaceCommutatorPlan::new(
            self.n,
            commutator,
            FaceCommutatorMode::Normalized,
            rows,
            columns,
        )
    }

    pub fn try_normalized_face_commutator_plan<'a>(
        &self,
        commutator: FaceCommutator,
        rows: &'a [usize],
        columns: &'a [usize],
    ) -> Result<FaceCommutatorPlan<'a>, FaceCommutatorValidationError> {
        FaceCommutatorPlan::try_new(
            self.n,
            commutator,
            FaceCommutatorMode::Normalized,
            rows,
            columns,
        )
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
        self.face_commutator_plan(
            FaceCommutator::new(destination, helper, slice_angle),
            rows,
            columns,
        )
        .literal_moves()
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
        self.normalized_face_commutator_plan(
            FaceCommutator::new(destination, helper, slice_angle),
            rows,
            columns,
        )
        .literal_moves()
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
        let plan = self.face_commutator_plan(
            FaceCommutator::new(destination, helper, slice_angle),
            rows,
            columns,
        );
        self.apply_face_commutator_plan_untracked(plan);
    }

    pub fn apply_face_commutator_plan_literal_untracked(&mut self, plan: FaceCommutatorPlan<'_>) {
        self.validate_face_commutator(plan);
        self.apply_moves_untracked(plan.literal_moves());
    }

    pub fn apply_face_commutator_plan_untracked(&mut self, plan: FaceCommutatorPlan<'_>) {
        self.validate_face_commutator(plan);

        match plan.mode {
            FaceCommutatorMode::Expanded => self.apply_face_commutator_untracked_direct(
                plan.commutator,
                plan.layers.rows,
                plan.layers.columns,
            ),
            FaceCommutatorMode::Normalized => self.apply_sparse_commutator_template_untracked(
                plan.commutator.normalized_template,
                plan.layers.rows,
                plan.layers.columns,
            ),
        }
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
        let plan = self.normalized_face_commutator_plan(
            FaceCommutator::new(destination, helper, slice_angle),
            rows,
            columns,
        );
        self.apply_face_commutator_plan_untracked(plan);
    }

    pub fn apply_normalized_face_commutator_plan_untracked(
        &mut self,
        plan: FaceCommutatorPlan<'_>,
    ) {
        debug_assert_eq!(
            plan.mode,
            FaceCommutatorMode::Normalized,
            "normalized commutator executor requires a normalized plan"
        );
        self.apply_face_commutator_plan_untracked(plan);
    }

    pub(crate) fn apply_normalized_face_commutator_prevalidated_untracked(
        &mut self,
        commutator: FaceCommutator,
        rows: &[usize],
        columns: &[usize],
    ) {
        debug_assert!(self
            .try_normalized_face_commutator_plan(commutator, rows, columns)
            .is_ok());
        self.apply_sparse_commutator_template_untracked(
            commutator.normalized_template,
            rows,
            columns,
        );
    }

    pub(crate) fn apply_virtual_normalized_face_commutator_prevalidated_untracked(
        &mut self,
        commutator: FaceCommutator,
        rows: &[usize],
        columns: &[usize],
        virtual_face_rotations: &[u8; 6],
    ) {
        debug_assert!(self
            .try_normalized_face_commutator_plan(commutator, rows, columns)
            .is_ok());
        self.apply_sparse_commutator_template_with_virtual_rotations_untracked(
            commutator.normalized_template,
            rows,
            columns,
            virtual_face_rotations,
        );
    }

    pub fn face_commutator_sparse_updates(
        &self,
        commutator: FaceCommutator,
        row: usize,
        column: usize,
    ) -> [FaceletUpdate; 3] {
        self.face_commutator_plan(commutator, &[row], &[column])
            .sparse_updates(row, column)
    }

    pub fn normalized_face_commutator_sparse_updates(
        &self,
        commutator: FaceCommutator,
        row: usize,
        column: usize,
    ) -> [FaceletUpdate; 3] {
        self.normalized_face_commutator_plan(commutator, &[row], &[column])
            .sparse_updates(row, column)
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
        let plan = self.face_commutator_plan(
            FaceCommutator::new(destination, helper, slice_angle),
            rows,
            columns,
        );
        self.validate_face_commutator(plan);

        let baseline = face_layer_move(self.n, destination, 0, MoveAngle::Positive);
        self.apply_move_untracked(baseline);

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
        self.apply_move_untracked(baseline);

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

        let [first, second, third] = template.updates;

        for row in rows.iter().copied() {
            for column in columns.iter().copied() {
                let first_from = first.from.eval(self.n, row, column);
                let second_from = second.from.eval(self.n, row, column);
                let third_from = third.from.eval(self.n, row, column);
                let first_value = self.position(first_from);
                let second_value = self.position(second_from);
                let third_value = self.position(third_from);

                self.set_position(first.to.eval(self.n, row, column), first_value);
                self.set_position(second.to.eval(self.n, row, column), second_value);
                self.set_position(third.to.eval(self.n, row, column), third_value);
            }
        }
    }

    fn apply_sparse_commutator_template_with_virtual_rotations_untracked(
        &mut self,
        template: CenterCommutatorTemplate,
        rows: &[usize],
        columns: &[usize],
        virtual_face_rotations: &[u8; 6],
    ) {
        if rows.is_empty() || columns.is_empty() {
            return;
        }

        let [first, second, third] = template.updates;

        for row in rows.iter().copied() {
            for column in columns.iter().copied() {
                let first_from = first.from.eval(self.n, row, column);
                let second_from = second.from.eval(self.n, row, column);
                let third_from = third.from.eval(self.n, row, column);
                let first_value =
                    self.raw_position_with_virtual_rotations(first_from, virtual_face_rotations);
                let second_value =
                    self.raw_position_with_virtual_rotations(second_from, virtual_face_rotations);
                let third_value =
                    self.raw_position_with_virtual_rotations(third_from, virtual_face_rotations);

                self.set_raw_position_with_virtual_rotations(
                    first.to.eval(self.n, row, column),
                    first_value,
                    virtual_face_rotations,
                );
                self.set_raw_position_with_virtual_rotations(
                    second.to.eval(self.n, row, column),
                    second_value,
                    virtual_face_rotations,
                );
                self.set_raw_position_with_virtual_rotations(
                    third.to.eval(self.n, row, column),
                    third_value,
                    virtual_face_rotations,
                );
            }
        }
    }

    #[inline(always)]
    fn raw_position_with_virtual_rotations(
        &self,
        position: FacePosition,
        virtual_face_rotations: &[u8; 6],
    ) -> u8 {
        let index = self.raw_index_with_virtual_rotations(position, virtual_face_rotations);
        unsafe {
            self.faces[position.face.index()]
                .matrix()
                .storage()
                .get_unchecked_raw(index)
        }
    }

    #[inline(always)]
    fn set_raw_position_with_virtual_rotations(
        &mut self,
        position: FacePosition,
        value: u8,
        virtual_face_rotations: &[u8; 6],
    ) {
        let index = self.raw_index_with_virtual_rotations(position, virtual_face_rotations);
        unsafe {
            self.faces[position.face.index()]
                .matrix_mut()
                .storage_mut()
                .set_unchecked_raw(index, value);
        }
    }

    #[inline(always)]
    fn raw_index_with_virtual_rotations(
        &self,
        position: FacePosition,
        virtual_face_rotations: &[u8; 6],
    ) -> usize {
        let (row, col) = self.physical_coords_with_virtual_rotations(
            position.face,
            position.row,
            position.col,
            virtual_face_rotations,
        );
        row * self.n + col
    }

    #[inline(always)]
    fn physical_coords_with_virtual_rotations(
        &self,
        face: FaceId,
        row: usize,
        col: usize,
        virtual_face_rotations: &[u8; 6],
    ) -> (usize, usize) {
        debug_assert!(row < self.n, "row out of bounds");
        debug_assert!(col < self.n, "column out of bounds");

        let turns = (self.faces[face.index()].rotation().as_u8()
            + virtual_face_rotations[face.index()])
            & 3;
        match turns {
            0 => (row, col),
            1 => (self.n - 1 - col, row),
            2 => (self.n - 1 - row, self.n - 1 - col),
            3 => (col, self.n - 1 - row),
            _ => unreachable!("face angle is always normalized"),
        }
    }

    fn validate_face_commutator(&self, plan: FaceCommutatorPlan<'_>) {
        debug_assert_eq!(
            self.n, plan.side_length,
            "face commutator plan side length must match the cube"
        );
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
pub(super) struct CenterCommutatorTemplate {
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

#[cfg(test)]
pub(crate) fn face_commutator_moves(
    n: usize,
    destination: FaceId,
    helper: FaceId,
    rows: &[usize],
    columns: &[usize],
    slice_angle: MoveAngle,
) -> Vec<Move> {
    let mut moves = Vec::with_capacity(2 * rows.len() + 2 * columns.len() + 3);
    for_each_face_commutator_move(n, destination, helper, rows, columns, slice_angle, |mv| {
        moves.push(mv)
    });
    moves
}

#[cfg(test)]
pub(crate) fn normalized_face_commutator_moves(
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

fn for_each_face_commutator_move(
    n: usize,
    destination: FaceId,
    helper: FaceId,
    rows: &[usize],
    columns: &[usize],
    slice_angle: MoveAngle,
    mut f: impl FnMut(Move),
) {
    let reverse = slice_angle.inverse();

    for column in columns.iter().copied() {
        f(face_layer_move(n, helper, column, reverse));
    }
    f(face_layer_move(n, destination, 0, MoveAngle::Positive));
    for row in rows.iter().copied() {
        f(face_layer_move(n, helper, row, reverse));
    }
    f(face_layer_move(n, destination, 0, MoveAngle::Negative));
    for column in columns.iter().copied() {
        f(face_layer_move(n, helper, column, slice_angle));
    }
    f(face_layer_move(n, destination, 0, MoveAngle::Positive));
    for row in rows.iter().copied() {
        f(face_layer_move(n, helper, row, slice_angle));
    }
}

fn try_validate_face_commutator_faces(
    destination: FaceId,
    helper: FaceId,
) -> Result<(), FaceCommutatorValidationError> {
    if destination == helper {
        return Err(FaceCommutatorValidationError::DestinationAndHelperMustDiffer);
    }
    if destination == opposite_face(helper) {
        return Err(FaceCommutatorValidationError::DestinationAndHelperMustBePerpendicular);
    }

    Ok(())
}

fn try_validate_inner_layer_set(
    n: usize,
    layers: &[usize],
    set: LayerSetKind,
) -> Result<(), LayerSetValidationError> {
    let mut previous = None;
    for layer in layers.iter().copied() {
        if !(layer > 0 && layer + 1 < n) {
            return Err(LayerSetValidationError::MustContainOnlyInnerLayers { set });
        }
        if let Some(previous) = previous {
            if previous >= layer {
                return Err(LayerSetValidationError::MustBeStrictlyIncreasing { set });
            }
        }
        previous = Some(layer);
    }

    Ok(())
}

fn try_validate_face_commutator(
    n: usize,
    destination: FaceId,
    helper: FaceId,
    rows: &[usize],
    columns: &[usize],
) -> Result<(), FaceCommutatorValidationError> {
    if n < 3 {
        return Err(FaceCommutatorValidationError::CubeTooSmall);
    }
    try_validate_face_commutator_faces(destination, helper)?;
    try_validate_inner_layer_set(n, rows, LayerSetKind::Rows)
        .map_err(FaceCommutatorValidationError::InvalidLayerSet)?;
    try_validate_inner_layer_set(n, columns, LayerSetKind::Columns)
        .map_err(FaceCommutatorValidationError::InvalidLayerSet)?;
    if !sorted_layer_sets_are_disjoint(rows, columns) {
        return Err(FaceCommutatorValidationError::RowAndColumnSetsMustBeDisjoint);
    }

    Ok(())
}

pub(crate) fn sorted_layer_sets_are_disjoint(left: &[usize], right: &[usize]) -> bool {
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

pub(crate) fn try_expanded_face_commutator_difference_cycle(
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

pub(crate) fn try_normalized_face_commutator_difference_cycle(
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

pub(crate) fn positions_are_unique(positions: impl IntoIterator<Item = FacePosition>) -> bool {
    let mut seen = std::collections::HashSet::new();
    for position in positions {
        if !seen.insert(position) {
            return false;
        }
    }

    true
}

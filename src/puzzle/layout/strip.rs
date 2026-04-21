use crate::{face::FaceId, facelet::Facelet, moves::MoveAngle, storage::FaceletArray};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineBuffer {
    data: Vec<Facelet>,
}

impl LineBuffer {
    pub fn with_len(len: usize, fill: Facelet) -> Self {
        Self {
            data: vec![fill; len],
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn as_slice(&self) -> &[Facelet] {
        &self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut [Facelet] {
        &mut self.data
    }

    pub fn reverse(&mut self) {
        self.data.reverse();
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum LineKind {
    Row,
    Col,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct StripSpec {
    pub face: FaceId,
    pub kind: LineKind,
    pub index: usize,
    pub reversed: bool,
}

impl StripSpec {
    pub const fn row(face: FaceId, index: usize, reversed: bool) -> Self {
        Self {
            face,
            kind: LineKind::Row,
            index,
            reversed,
        }
    }

    pub const fn col(face: FaceId, index: usize, reversed: bool) -> Self {
        Self {
            face,
            kind: LineKind::Col,
            index,
            reversed,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct LineTraversal {
    pub start: isize,
    pub step: isize,
}

impl LineTraversal {
    #[inline(always)]
    pub fn new(start: usize, step: isize) -> Self {
        Self {
            start: isize::try_from(start).expect("line start index overflowed isize"),
            step,
        }
    }
}

pub fn cycle_four_line_arrays<S: FaceletArray>(
    line0: &mut S,
    line1: &mut S,
    line2: &mut S,
    line3: &mut S,
    angle: MoveAngle,
) {
    let len = line0.len();
    assert_eq!(line1.len(), len, "line lengths must match");
    assert_eq!(line2.len(), len, "line lengths must match");
    assert_eq!(line3.len(), len, "line lengths must match");

    cycle_four_lines(
        line0,
        LineTraversal::new(0, 1),
        line1,
        LineTraversal::new(0, 1),
        line2,
        LineTraversal::new(0, 1),
        line3,
        LineTraversal::new(0, 1),
        len,
        angle,
    );
}

pub fn cycle_four_line_arrays_with_threads<S: FaceletArray>(
    line0: &mut S,
    line1: &mut S,
    line2: &mut S,
    line3: &mut S,
    angle: MoveAngle,
    thread_count: usize,
) {
    assert!(thread_count > 0, "thread count must be greater than zero");

    if thread_count == 1 {
        cycle_four_line_arrays(line0, line1, line2, line3, angle);
        return;
    }

    let len = line0.len();
    assert_eq!(line1.len(), len, "line lengths must match");
    assert_eq!(line2.len(), len, "line lengths must match");
    assert_eq!(line3.len(), len, "line lengths must match");

    cycle_four_lines_with_threads(
        line0,
        LineTraversal::new(0, 1),
        line1,
        LineTraversal::new(0, 1),
        line2,
        LineTraversal::new(0, 1),
        line3,
        LineTraversal::new(0, 1),
        len,
        angle,
        thread_count,
    );
}

pub fn cycle_four_line_arrays_many_with_threads<S, I>(
    line0: &mut S,
    line1: &mut S,
    line2: &mut S,
    line3: &mut S,
    angles: I,
    thread_count: usize,
) where
    S: FaceletArray,
    I: IntoIterator<Item = MoveAngle>,
{
    assert!(thread_count > 0, "thread count must be greater than zero");

    if thread_count == 1 {
        for angle in angles {
            cycle_four_line_arrays(line0, line1, line2, line3, angle);
        }
        return;
    }

    let len = line0.len();
    assert_eq!(line1.len(), len, "line lengths must match");
    assert_eq!(line2.len(), len, "line lengths must match");
    assert_eq!(line3.len(), len, "line lengths must match");

    with_line_cycle_runner::<S, _, _>(len, thread_count, |runner| {
        for angle in angles {
            runner.cycle(
                line0,
                LineTraversal::new(0, 1),
                line1,
                LineTraversal::new(0, 1),
                line2,
                LineTraversal::new(0, 1),
                line3,
                LineTraversal::new(0, 1),
                angle,
            );
        }
    });
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn cycle_four_lines<S: FaceletArray>(
    storage0: &mut S,
    traversal0: LineTraversal,
    storage1: &mut S,
    traversal1: LineTraversal,
    storage2: &mut S,
    traversal2: LineTraversal,
    storage3: &mut S,
    traversal3: LineTraversal,
    len: usize,
    angle: MoveAngle,
) {
    match angle {
        MoveAngle::Positive => cycle_four_lines_mapped(
            storage0,
            traversal0,
            storage1,
            traversal1,
            storage2,
            traversal2,
            storage3,
            traversal3,
            len,
            |v0, v1, v2, v3| (v3, v0, v1, v2),
        ),
        MoveAngle::Double => cycle_four_lines_mapped(
            storage0,
            traversal0,
            storage1,
            traversal1,
            storage2,
            traversal2,
            storage3,
            traversal3,
            len,
            |v0, v1, v2, v3| (v2, v3, v0, v1),
        ),
        MoveAngle::Negative => cycle_four_lines_mapped(
            storage0,
            traversal0,
            storage1,
            traversal1,
            storage2,
            traversal2,
            storage3,
            traversal3,
            len,
            |v0, v1, v2, v3| (v1, v2, v3, v0),
        ),
    }
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn cycle_four_lines_mapped<S, F>(
    storage0: &mut S,
    traversal0: LineTraversal,
    storage1: &mut S,
    traversal1: LineTraversal,
    storage2: &mut S,
    traversal2: LineTraversal,
    storage3: &mut S,
    traversal3: LineTraversal,
    len: usize,
    mut rotate: F,
) where
    S: FaceletArray,
    F: FnMut(u8, u8, u8, u8) -> (u8, u8, u8, u8),
{
    let mut p0 = traversal0.start;
    let mut p1 = traversal1.start;
    let mut p2 = traversal2.start;
    let mut p3 = traversal3.start;

    for _ in 0..len {
        let i0 = p0 as usize;
        let i1 = p1 as usize;
        let i2 = p2 as usize;
        let i3 = p3 as usize;

        unsafe {
            // Traversals come from validated strips; raw values are only moved between storages.
            let v0 = storage0.get_unchecked_raw(i0);
            let v1 = storage1.get_unchecked_raw(i1);
            let v2 = storage2.get_unchecked_raw(i2);
            let v3 = storage3.get_unchecked_raw(i3);
            let (n0, n1, n2, n3) = rotate(v0, v1, v2, v3);

            storage0.set_unchecked_raw(i0, n0);
            storage1.set_unchecked_raw(i1, n1);
            storage2.set_unchecked_raw(i2, n2);
            storage3.set_unchecked_raw(i3, n3);
        }

        p0 += traversal0.step;
        p1 += traversal1.step;
        p2 += traversal2.step;
        p3 += traversal3.step;
    }
}

#[inline]
#[allow(clippy::too_many_arguments)]
pub(crate) fn cycle_four_lines_with_threads<S: FaceletArray>(
    storage0: &mut S,
    traversal0: LineTraversal,
    storage1: &mut S,
    traversal1: LineTraversal,
    storage2: &mut S,
    traversal2: LineTraversal,
    storage3: &mut S,
    traversal3: LineTraversal,
    len: usize,
    angle: MoveAngle,
    thread_count: usize,
) {
    assert!(thread_count > 0, "thread count must be greater than zero");

    if thread_count == 1 {
        cycle_four_lines(
            storage0, traversal0, storage1, traversal1, storage2, traversal2, storage3, traversal3,
            len, angle,
        );
        return;
    }

    with_line_cycle_runner::<S, _, _>(len, thread_count, |runner| {
        runner.cycle(
            storage0, traversal0, storage1, traversal1, storage2, traversal2, storage3, traversal3,
            angle,
        );
    });
}

pub(crate) fn with_line_cycle_runner<S, F, R>(len: usize, thread_count: usize, f: F) -> R
where
    S: FaceletArray,
    F: FnOnce(&mut LineCycleRunner<'_, S>) -> R,
{
    assert!(thread_count > 0, "thread count must be greater than zero");

    if thread_count == 1 || len == 0 {
        let mut runner = LineCycleRunner::Linear { len };
        return f(&mut runner);
    }

    let worker_count = thread_count - 1;
    let shared = LineCycleShared::new(worker_count);

    std::thread::scope(|scope| {
        for worker_index in 0..worker_count {
            let shared = &shared;
            scope.spawn(move || line_cycle_worker_loop::<S>(shared, worker_index));
        }

        let mut runner = LineCycleRunner::Parallel {
            len,
            worker_count,
            shared: &shared,
        };
        f(&mut runner)
    })
}

pub(crate) enum LineCycleRunner<'a, S: FaceletArray> {
    Linear {
        len: usize,
    },
    Parallel {
        len: usize,
        worker_count: usize,
        shared: &'a LineCycleShared<S>,
    },
}

impl<S: FaceletArray> LineCycleRunner<'_, S> {
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn cycle(
        &mut self,
        storage0: &mut S,
        traversal0: LineTraversal,
        storage1: &mut S,
        traversal1: LineTraversal,
        storage2: &mut S,
        traversal2: LineTraversal,
        storage3: &mut S,
        traversal3: LineTraversal,
        angle: MoveAngle,
    ) {
        match self {
            Self::Linear { len } => cycle_four_lines(
                storage0, traversal0, storage1, traversal1, storage2, traversal2, storage3,
                traversal3, *len, angle,
            ),
            Self::Parallel {
                len,
                worker_count,
                shared,
            } => {
                let chunks = contiguous_line_chunks::<S>(
                    *len,
                    *worker_count + 1,
                    [traversal0, traversal1, traversal2, traversal3],
                );

                if chunks.iter().filter(|chunk| !chunk.is_empty()).count() <= 1 {
                    cycle_four_lines(
                        storage0, traversal0, storage1, traversal1, storage2, traversal2, storage3,
                        traversal3, *len, angle,
                    );
                    return;
                }

                let job = LineCycleJob {
                    storage0: storage0.raw_storage(),
                    traversal0,
                    storage1: storage1.raw_storage(),
                    traversal1,
                    storage2: storage2.raw_storage(),
                    traversal2,
                    storage3: storage3.raw_storage(),
                    traversal3,
                    angle,
                    chunks: chunks.into(),
                };
                let current_thread_chunk = job.chunks[*worker_count];

                shared.start_job(job.clone());
                unsafe {
                    cycle_four_lines_raw_chunk::<S>(
                        job.storage0,
                        job.traversal0,
                        job.storage1,
                        job.traversal1,
                        job.storage2,
                        job.traversal2,
                        job.storage3,
                        job.traversal3,
                        current_thread_chunk.start,
                        current_thread_chunk.end,
                        job.angle,
                    );
                }
                shared.wait_for_workers();
            }
        }
    }
}

impl<S: FaceletArray> Drop for LineCycleRunner<'_, S> {
    fn drop(&mut self) {
        if let Self::Parallel { shared, .. } = self {
            shared.stop_workers();
        }
    }
}

pub(crate) struct LineCycleShared<S: FaceletArray> {
    state: std::sync::Mutex<LineCycleState<S>>,
    job_available: std::sync::Condvar,
    job_done: std::sync::Condvar,
}

impl<S: FaceletArray> LineCycleShared<S> {
    fn new(worker_count: usize) -> Self {
        Self {
            state: std::sync::Mutex::new(LineCycleState {
                generation: 0,
                pending_workers: 0,
                worker_count,
                stop: false,
                job: None,
            }),
            job_available: std::sync::Condvar::new(),
            job_done: std::sync::Condvar::new(),
        }
    }

    fn start_job(&self, job: LineCycleJob<S>) {
        let mut state = self.state.lock().expect("line cycle worker mutex poisoned");
        debug_assert_eq!(state.pending_workers, 0);
        state.job = Some(job);
        state.pending_workers = state.worker_count;
        state.generation = state.generation.wrapping_add(1);
        self.job_available.notify_all();
    }

    fn wait_for_workers(&self) {
        let mut state = self.state.lock().expect("line cycle worker mutex poisoned");
        while state.pending_workers > 0 {
            state = self
                .job_done
                .wait(state)
                .expect("line cycle worker mutex poisoned");
        }
        state.job = None;
    }

    fn stop_workers(&self) {
        let mut state = self.state.lock().expect("line cycle worker mutex poisoned");
        state.stop = true;
        state.generation = state.generation.wrapping_add(1);
        self.job_available.notify_all();
    }
}

struct LineCycleState<S: FaceletArray> {
    generation: u64,
    pending_workers: usize,
    worker_count: usize,
    stop: bool,
    job: Option<LineCycleJob<S>>,
}

struct LineCycleJob<S: FaceletArray> {
    storage0: S::RawStorage,
    traversal0: LineTraversal,
    storage1: S::RawStorage,
    traversal1: LineTraversal,
    storage2: S::RawStorage,
    traversal2: LineTraversal,
    storage3: S::RawStorage,
    traversal3: LineTraversal,
    angle: MoveAngle,
    chunks: std::sync::Arc<[LineChunk]>,
}

impl<S: FaceletArray> Clone for LineCycleJob<S> {
    fn clone(&self) -> Self {
        Self {
            storage0: self.storage0,
            traversal0: self.traversal0,
            storage1: self.storage1,
            traversal1: self.traversal1,
            storage2: self.storage2,
            traversal2: self.traversal2,
            storage3: self.storage3,
            traversal3: self.traversal3,
            angle: self.angle,
            chunks: self.chunks.clone(),
        }
    }
}

fn line_cycle_worker_loop<S: FaceletArray>(shared: &LineCycleShared<S>, worker_index: usize) {
    let mut last_seen_generation = 0;

    loop {
        let job = {
            let mut state = shared
                .state
                .lock()
                .expect("line cycle worker mutex poisoned");

            while state.generation == last_seen_generation && !state.stop {
                state = shared
                    .job_available
                    .wait(state)
                    .expect("line cycle worker mutex poisoned");
            }

            if state.stop {
                return;
            }

            last_seen_generation = state.generation;
            state
                .job
                .as_ref()
                .cloned()
                .expect("line cycle job must be set before notifying")
        };
        let chunk = job.chunks[worker_index];

        unsafe {
            cycle_four_lines_raw_chunk::<S>(
                job.storage0,
                job.traversal0,
                job.storage1,
                job.traversal1,
                job.storage2,
                job.traversal2,
                job.storage3,
                job.traversal3,
                chunk.start,
                chunk.end,
                job.angle,
            );
        }

        let mut state = shared
            .state
            .lock()
            .expect("line cycle worker mutex poisoned");
        state.pending_workers -= 1;
        if state.pending_workers == 0 {
            shared.job_done.notify_one();
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct LineChunk {
    start: usize,
    end: usize,
}

impl LineChunk {
    fn is_empty(self) -> bool {
        self.start == self.end
    }
}

fn contiguous_line_chunks<S: FaceletArray>(
    len: usize,
    thread_count: usize,
    traversals: [LineTraversal; 4],
) -> Vec<LineChunk> {
    let mut boundaries = Vec::with_capacity(thread_count + 1);
    boundaries.push(0);

    for worker in 1..thread_count {
        let previous = *boundaries
            .last()
            .expect("line chunk boundaries must start with zero");
        let ideal = worker * len / thread_count;
        let boundary =
            nearest_safe_chunk_boundary::<S>(ideal, previous, len, traversals).unwrap_or(previous);
        boundaries.push(boundary);
    }

    boundaries.push(len);

    let mut chunks = Vec::with_capacity(thread_count);
    for pair in boundaries.windows(2) {
        chunks.push(LineChunk {
            start: pair[0],
            end: pair[1],
        });
    }

    chunks
}

fn nearest_safe_chunk_boundary<S: FaceletArray>(
    ideal: usize,
    previous: usize,
    len: usize,
    traversals: [LineTraversal; 4],
) -> Option<usize> {
    if previous + 1 >= len {
        return None;
    }

    let ideal = ideal.clamp(previous + 1, len - 1);
    let max_distance = (ideal - previous).max(len - ideal);

    for distance in 0..=max_distance {
        if let Some(candidate) = ideal.checked_sub(distance) {
            if candidate > previous && is_safe_chunk_boundary::<S>(candidate, len, traversals) {
                return Some(candidate);
            }
        }

        if let Some(candidate) = ideal.checked_add(distance) {
            if distance != 0
                && candidate < len
                && is_safe_chunk_boundary::<S>(candidate, len, traversals)
            {
                return Some(candidate);
            }
        }
    }

    None
}

fn is_safe_chunk_boundary<S: FaceletArray>(
    boundary: usize,
    len: usize,
    traversals: [LineTraversal; 4],
) -> bool {
    if boundary == 0 || boundary >= len {
        return true;
    }

    traversals.iter().copied().all(|traversal| {
        let previous = traversal_storage_unit_range::<S>(traversal, boundary - 1);
        let next = traversal_storage_unit_range::<S>(traversal, boundary);
        storage_unit_ranges_are_disjoint(previous, next)
    })
}

fn traversal_storage_unit_range<S: FaceletArray>(
    traversal: LineTraversal,
    offset: usize,
) -> (usize, usize) {
    let index = traversal_position(traversal, offset) as usize;
    S::storage_unit_range(index)
}

fn storage_unit_ranges_are_disjoint(a: (usize, usize), b: (usize, usize)) -> bool {
    a.1 < b.0 || b.1 < a.0
}

#[inline(always)]
fn traversal_position(traversal: LineTraversal, offset: usize) -> isize {
    let offset = isize::try_from(offset).expect("line offset overflowed isize");
    let scaled = traversal
        .step
        .checked_mul(offset)
        .expect("line traversal offset overflowed isize");
    traversal
        .start
        .checked_add(scaled)
        .expect("line traversal position overflowed isize")
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
unsafe fn cycle_four_lines_raw_chunk<S: FaceletArray>(
    storage0: S::RawStorage,
    traversal0: LineTraversal,
    storage1: S::RawStorage,
    traversal1: LineTraversal,
    storage2: S::RawStorage,
    traversal2: LineTraversal,
    storage3: S::RawStorage,
    traversal3: LineTraversal,
    start: usize,
    end: usize,
    angle: MoveAngle,
) {
    match angle {
        MoveAngle::Positive => cycle_four_lines_raw_chunk_mapped::<S, _>(
            storage0,
            traversal0,
            storage1,
            traversal1,
            storage2,
            traversal2,
            storage3,
            traversal3,
            start,
            end,
            |v0, v1, v2, v3| (v3, v0, v1, v2),
        ),
        MoveAngle::Double => cycle_four_lines_raw_chunk_mapped::<S, _>(
            storage0,
            traversal0,
            storage1,
            traversal1,
            storage2,
            traversal2,
            storage3,
            traversal3,
            start,
            end,
            |v0, v1, v2, v3| (v2, v3, v0, v1),
        ),
        MoveAngle::Negative => cycle_four_lines_raw_chunk_mapped::<S, _>(
            storage0,
            traversal0,
            storage1,
            traversal1,
            storage2,
            traversal2,
            storage3,
            traversal3,
            start,
            end,
            |v0, v1, v2, v3| (v1, v2, v3, v0),
        ),
    }
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
unsafe fn cycle_four_lines_raw_chunk_mapped<S, F>(
    storage0: S::RawStorage,
    traversal0: LineTraversal,
    storage1: S::RawStorage,
    traversal1: LineTraversal,
    storage2: S::RawStorage,
    traversal2: LineTraversal,
    storage3: S::RawStorage,
    traversal3: LineTraversal,
    start: usize,
    end: usize,
    mut rotate: F,
) where
    S: FaceletArray,
    F: FnMut(u8, u8, u8, u8) -> (u8, u8, u8, u8),
{
    let mut p0 = traversal_position(traversal0, start);
    let mut p1 = traversal_position(traversal1, start);
    let mut p2 = traversal_position(traversal2, start);
    let mut p3 = traversal_position(traversal3, start);

    for _ in start..end {
        let i0 = p0 as usize;
        let i1 = p1 as usize;
        let i2 = p2 as usize;
        let i3 = p3 as usize;

        let v0 = S::get_unchecked_raw_from(storage0, i0);
        let v1 = S::get_unchecked_raw_from(storage1, i1);
        let v2 = S::get_unchecked_raw_from(storage2, i2);
        let v3 = S::get_unchecked_raw_from(storage3, i3);
        let (n0, n1, n2, n3) = rotate(v0, v1, v2, v3);

        S::set_unchecked_raw_in(storage0, i0, n0);
        S::set_unchecked_raw_in(storage1, i1, n1);
        S::set_unchecked_raw_in(storage2, i2, n2);
        S::set_unchecked_raw_in(storage3, i3, n3);

        p0 += traversal0.step;
        p1 += traversal1.step;
        p2 += traversal2.step;
        p3 += traversal3.step;
    }
}

#[derive(Clone, Debug)]
pub struct MoveScratch {
    pub a: LineBuffer,
    pub b: LineBuffer,
    pub c: LineBuffer,
    pub d: LineBuffer,
}

impl MoveScratch {
    pub fn new(line_len: usize) -> Self {
        let blank = Facelet::White;
        Self {
            a: LineBuffer::with_len(line_len, blank),
            b: LineBuffer::with_len(line_len, blank),
            c: LineBuffer::with_len(line_len, blank),
            d: LineBuffer::with_len(line_len, blank),
        }
    }

    pub fn line_len(&self) -> usize {
        self.a.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        cycle_four_line_arrays, cycle_four_line_arrays_many_with_threads,
        cycle_four_line_arrays_with_threads,
    };
    use crate::{Byte, Byte3, Facelet, FaceletArray, MoveAngle, Nibble, ThreeBit};

    fn storage(values: &[Facelet]) -> Byte {
        let mut storage = Byte::with_len(values.len(), Facelet::White);
        for (index, value) in values.iter().copied().enumerate() {
            storage.set(index, value);
        }
        storage
    }

    fn assert_storage_eq(storage: &Byte, expected: &[Facelet]) {
        assert_eq!(storage.len(), expected.len());
        for (index, value) in expected.iter().copied().enumerate() {
            assert_eq!(storage.get(index), value, "mismatch at index {index}");
        }
    }

    fn patterned_storage<S: FaceletArray>(len: usize, offset: usize) -> S {
        let mut storage = S::with_len(len, Facelet::White);

        for index in 0..len {
            let value = Facelet::from_u8(((index * 5 + offset) % Facelet::ALL.len()) as u8);
            storage.set(index, value);
        }

        storage
    }

    fn assert_matching_storage<S: FaceletArray>(actual: &S, expected: &S) {
        assert_eq!(actual.len(), expected.len());

        for index in 0..actual.len() {
            assert_eq!(
                actual.get(index),
                expected.get(index),
                "mismatch at index {index}"
            );
        }
    }

    fn threaded_line_cycles_match_linear<S: FaceletArray>() {
        for len in [0usize, 1, 2, 3, 5, 63, 64, 65, 127] {
            for angle in MoveAngle::ALL {
                let mut expected0 = patterned_storage::<S>(len, 0);
                let mut expected1 = patterned_storage::<S>(len, 1);
                let mut expected2 = patterned_storage::<S>(len, 2);
                let mut expected3 = patterned_storage::<S>(len, 3);

                cycle_four_line_arrays(
                    &mut expected0,
                    &mut expected1,
                    &mut expected2,
                    &mut expected3,
                    angle,
                );

                for thread_count in [1usize, 2, 4, 16] {
                    let mut actual0 = patterned_storage::<S>(len, 0);
                    let mut actual1 = patterned_storage::<S>(len, 1);
                    let mut actual2 = patterned_storage::<S>(len, 2);
                    let mut actual3 = patterned_storage::<S>(len, 3);

                    cycle_four_line_arrays_with_threads(
                        &mut actual0,
                        &mut actual1,
                        &mut actual2,
                        &mut actual3,
                        angle,
                        thread_count,
                    );

                    assert_matching_storage(&actual0, &expected0);
                    assert_matching_storage(&actual1, &expected1);
                    assert_matching_storage(&actual2, &expected2);
                    assert_matching_storage(&actual3, &expected3);
                }
            }
        }
    }

    fn threaded_many_line_cycles_match_linear<S: FaceletArray>() {
        let len = 65;
        let angles = [
            MoveAngle::Positive,
            MoveAngle::Double,
            MoveAngle::Negative,
            MoveAngle::Positive,
        ];
        let mut expected0 = patterned_storage::<S>(len, 0);
        let mut expected1 = patterned_storage::<S>(len, 1);
        let mut expected2 = patterned_storage::<S>(len, 2);
        let mut expected3 = patterned_storage::<S>(len, 3);

        for angle in angles {
            cycle_four_line_arrays(
                &mut expected0,
                &mut expected1,
                &mut expected2,
                &mut expected3,
                angle,
            );
        }

        for thread_count in [1usize, 2, 4, 16] {
            let mut actual0 = patterned_storage::<S>(len, 0);
            let mut actual1 = patterned_storage::<S>(len, 1);
            let mut actual2 = patterned_storage::<S>(len, 2);
            let mut actual3 = patterned_storage::<S>(len, 3);

            cycle_four_line_arrays_many_with_threads(
                &mut actual0,
                &mut actual1,
                &mut actual2,
                &mut actual3,
                angles,
                thread_count,
            );

            assert_matching_storage(&actual0, &expected0);
            assert_matching_storage(&actual1, &expected1);
            assert_matching_storage(&actual2, &expected2);
            assert_matching_storage(&actual3, &expected3);
        }
    }

    #[test]
    fn cycle_four_line_arrays_applies_positive_turn() {
        let mut a = storage(&[Facelet::White, Facelet::Yellow]);
        let mut b = storage(&[Facelet::Red, Facelet::Orange]);
        let mut c = storage(&[Facelet::Green, Facelet::Blue]);
        let mut d = storage(&[Facelet::Yellow, Facelet::White]);

        cycle_four_line_arrays(&mut a, &mut b, &mut c, &mut d, MoveAngle::Positive);

        assert_storage_eq(&a, &[Facelet::Yellow, Facelet::White]);
        assert_storage_eq(&b, &[Facelet::White, Facelet::Yellow]);
        assert_storage_eq(&c, &[Facelet::Red, Facelet::Orange]);
        assert_storage_eq(&d, &[Facelet::Green, Facelet::Blue]);
    }

    #[test]
    fn cycle_four_line_arrays_applies_double_turn() {
        let mut a = storage(&[Facelet::White, Facelet::Yellow]);
        let mut b = storage(&[Facelet::Red, Facelet::Orange]);
        let mut c = storage(&[Facelet::Green, Facelet::Blue]);
        let mut d = storage(&[Facelet::Yellow, Facelet::White]);

        cycle_four_line_arrays(&mut a, &mut b, &mut c, &mut d, MoveAngle::Double);

        assert_storage_eq(&a, &[Facelet::Green, Facelet::Blue]);
        assert_storage_eq(&b, &[Facelet::Yellow, Facelet::White]);
        assert_storage_eq(&c, &[Facelet::White, Facelet::Yellow]);
        assert_storage_eq(&d, &[Facelet::Red, Facelet::Orange]);
    }

    #[test]
    fn cycle_four_line_arrays_applies_negative_turn() {
        let mut a = storage(&[Facelet::White, Facelet::Yellow]);
        let mut b = storage(&[Facelet::Red, Facelet::Orange]);
        let mut c = storage(&[Facelet::Green, Facelet::Blue]);
        let mut d = storage(&[Facelet::Yellow, Facelet::White]);

        cycle_four_line_arrays(&mut a, &mut b, &mut c, &mut d, MoveAngle::Negative);

        assert_storage_eq(&a, &[Facelet::Red, Facelet::Orange]);
        assert_storage_eq(&b, &[Facelet::Green, Facelet::Blue]);
        assert_storage_eq(&c, &[Facelet::Yellow, Facelet::White]);
        assert_storage_eq(&d, &[Facelet::White, Facelet::Yellow]);
    }

    #[test]
    fn threaded_byte_cycles_match_linear() {
        threaded_line_cycles_match_linear::<Byte>();
        threaded_many_line_cycles_match_linear::<Byte>();
    }

    #[test]
    fn threaded_nibble_cycles_match_linear() {
        threaded_line_cycles_match_linear::<Nibble>();
        threaded_many_line_cycles_match_linear::<Nibble>();
    }

    #[test]
    fn threaded_three_bit_cycles_match_linear() {
        threaded_line_cycles_match_linear::<ThreeBit>();
        threaded_many_line_cycles_match_linear::<ThreeBit>();
    }

    #[test]
    fn threaded_byte3_cycles_match_linear() {
        threaded_line_cycles_match_linear::<Byte3>();
        threaded_many_line_cycles_match_linear::<Byte3>();
    }
}

use core::mem::MaybeUninit;

const MIN_PARALLEL_INIT_UNITS: usize = 16 * 1024;

pub(crate) fn filled_vec<T>(len: usize, value: T, thread_count: usize) -> Vec<T>
where
    T: Copy + Send,
{
    let thread_count = effective_thread_count(len, thread_count);

    if thread_count == 1 {
        return vec![value; len];
    }

    let mut data = uninit_vec(len);
    let chunk_len = len.div_ceil(thread_count);

    std::thread::scope(|scope| {
        for chunk in data.chunks_mut(chunk_len) {
            scope.spawn(move || {
                for slot in chunk {
                    slot.write(value);
                }
            });
        }
    });

    unsafe { assume_init_vec(data) }
}

pub(crate) fn initialized_vec<T, F>(len: usize, thread_count: usize, init: F) -> Vec<T>
where
    T: Copy + Send,
    F: Fn(usize) -> T + Sync,
{
    let thread_count = effective_thread_count(len, thread_count);

    if thread_count == 1 {
        return (0..len).map(init).collect();
    }

    let mut data = uninit_vec(len);
    let chunk_len = len.div_ceil(thread_count);

    std::thread::scope(|scope| {
        for (chunk_index, chunk) in data.chunks_mut(chunk_len).enumerate() {
            let init = &init;
            let start = chunk_index * chunk_len;
            scope.spawn(move || {
                for (offset, slot) in chunk.iter_mut().enumerate() {
                    slot.write(init(start + offset));
                }
            });
        }
    });

    unsafe { assume_init_vec(data) }
}

pub(crate) fn initialize_slice<T, F>(slice: &mut [T], thread_count: usize, init: F)
where
    T: Copy + Send,
    F: Fn(usize) -> T + Sync,
{
    let thread_count = effective_thread_count(slice.len(), thread_count);

    if thread_count == 1 {
        for (index, slot) in slice.iter_mut().enumerate() {
            *slot = init(index);
        }
        return;
    }

    let chunk_len = slice.len().div_ceil(thread_count);

    std::thread::scope(|scope| {
        for (chunk_index, chunk) in slice.chunks_mut(chunk_len).enumerate() {
            let init = &init;
            let start = chunk_index * chunk_len;
            scope.spawn(move || {
                for (offset, slot) in chunk.iter_mut().enumerate() {
                    *slot = init(start + offset);
                }
            });
        }
    });
}

pub(crate) fn fill_slice<T>(slice: &mut [T], value: T, thread_count: usize)
where
    T: Copy + Send,
{
    let thread_count = effective_thread_count(slice.len(), thread_count);

    if thread_count == 1 {
        slice.fill(value);
        return;
    }

    let chunk_len = slice.len().div_ceil(thread_count);

    std::thread::scope(|scope| {
        for chunk in slice.chunks_mut(chunk_len) {
            scope.spawn(move || chunk.fill(value));
        }
    });
}

fn effective_thread_count(len: usize, thread_count: usize) -> usize {
    assert!(thread_count > 0, "thread count must be greater than zero");

    if thread_count == 1 || len < MIN_PARALLEL_INIT_UNITS {
        return 1;
    }

    thread_count.min(len)
}

fn uninit_vec<T>(len: usize) -> Vec<MaybeUninit<T>> {
    let mut data = Vec::with_capacity(len);
    unsafe {
        // MaybeUninit<T> can hold uninitialized bytes; callers write every slot
        // before converting the vector to Vec<T>.
        data.set_len(len);
    }
    data
}

unsafe fn assume_init_vec<T>(mut data: Vec<MaybeUninit<T>>) -> Vec<T> {
    // The allocation layout is identical. Callers guarantee every element was
    // initialized before this conversion.
    let ptr = data.as_mut_ptr().cast::<T>();
    let len = data.len();
    let capacity = data.capacity();
    core::mem::forget(data);
    Vec::from_raw_parts(ptr, len, capacity)
}

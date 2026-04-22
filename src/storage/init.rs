pub(crate) fn filled_vec<T>(len: usize, value: T) -> Vec<T>
where
    T: Copy,
{
    vec![value; len]
}

pub(crate) fn initialized_vec<T, F>(len: usize, init: F) -> Vec<T>
where
    F: Fn(usize) -> T,
{
    (0..len).map(init).collect()
}

pub(crate) fn initialize_slice<T, F>(slice: &mut [T], init: F)
where
    F: Fn(usize) -> T,
{
    for (index, slot) in slice.iter_mut().enumerate() {
        *slot = init(index);
    }
}

pub(crate) fn fill_slice<T>(slice: &mut [T], value: T)
where
    T: Copy,
{
    slice.fill(value);
}

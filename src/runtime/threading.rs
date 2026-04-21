pub fn default_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|threads| threads.get())
        .unwrap_or(1)
}

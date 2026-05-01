use std::sync::OnceLock;

use rayon::ThreadPool;

pub fn default_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|threads| threads.get())
        .unwrap_or(1)
}

pub fn optimized_thread_count() -> usize {
    const DEFAULT_MAX_THREADS: usize = 4;

    if let Ok(value) = std::env::var("RUBIK_OPT_THREADS") {
        if let Ok(parsed) = value.parse::<usize>() {
            return parsed.max(1);
        }
    }

    default_thread_count().clamp(1, DEFAULT_MAX_THREADS)
}

pub(crate) fn optimized_thread_pool() -> &'static ThreadPool {
    static POOL: OnceLock<ThreadPool> = OnceLock::new();

    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(optimized_thread_count())
            .thread_name(|index| format!("rubik-opt-{index}"))
            .build()
            .expect("failed to build optimized thread pool")
    })
}

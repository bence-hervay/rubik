use std::sync::OnceLock;

use rayon::ThreadPool;

static CONFIGURED_OPTIMIZED_THREAD_COUNT: OnceLock<usize> = OnceLock::new();
static OPTIMIZED_THREAD_POOL: OnceLock<ThreadPool> = OnceLock::new();

pub fn default_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|threads| threads.get())
        .unwrap_or(1)
}

pub fn optimized_thread_count() -> usize {
    if let Some(thread_count) = CONFIGURED_OPTIMIZED_THREAD_COUNT.get() {
        return *thread_count;
    }

    if let Ok(value) = std::env::var("RUBIK_OPT_THREADS") {
        if let Ok(parsed) = value.parse::<usize>() {
            return parsed.max(1);
        }
    }

    default_thread_count()
}

pub fn configure_optimized_thread_count(thread_count: usize) -> Result<(), String> {
    let thread_count = thread_count.max(1);

    if let Some(pool) = OPTIMIZED_THREAD_POOL.get() {
        let current = pool.current_num_threads();
        if current == thread_count {
            return Ok(());
        }

        return Err(format!(
            "optimized thread pool has already been initialized with {current} threads"
        ));
    }

    if let Some(configured) = CONFIGURED_OPTIMIZED_THREAD_COUNT.get() {
        if *configured == thread_count {
            return Ok(());
        }

        return Err(format!(
            "optimized thread count is already configured as {configured}"
        ));
    }

    CONFIGURED_OPTIMIZED_THREAD_COUNT
        .set(thread_count)
        .map_err(|_| "optimized thread count is already configured".to_owned())
}

pub(crate) fn optimized_thread_pool() -> &'static ThreadPool {
    OPTIMIZED_THREAD_POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(optimized_thread_count())
            .thread_name(|index| format!("rubik-opt-{index}"))
            .build()
            .expect("failed to build optimized thread pool")
    })
}

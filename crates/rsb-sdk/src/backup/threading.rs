use tracing::debug;
use crate::config::Config;

pub fn determine_optimal_threads(_config: &Config, max_threads: Option<usize>) -> usize {
     if let Some(threads) = max_threads {
        if threads > 0 {
            return threads.min(256); // maximum limit for parallel I/O
        }
    }

    // 2. Automatic logic based on cores
    let cores = num_cpus::get();
    
    let optimal = (cores * 2).min(256);

    debug!(
        "📊 System has {} cores, using {} threads for optimal backup parallelism",
        cores, optimal
    );
    
    optimal
}

/// Configures the global Rayon thread pool
pub fn setup_rayon_thread_pool(num_threads: usize) -> Result<(), rayon::ThreadPoolBuildError> {
    let _pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build_global();
    
    Ok(())
}

use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
pub struct Stats {
    processed: AtomicUsize,
    skipped: AtomicUsize,
    errors: AtomicUsize,
}

#[derive(Clone, Copy, Debug)]
pub struct StatsSummary {
    pub processed: usize,
    pub skipped: usize,
    pub errors: usize,
}

impl Stats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inc_processed(&self) {
        self.processed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_skipped(&self) {
        self.skipped.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_processed(&self) -> usize {
        self.processed.load(Ordering::Relaxed)
    }

    pub fn get_skipped(&self) -> usize {
        self.skipped.load(Ordering::Relaxed)
    }

    pub fn get_errors(&self) -> usize {
        self.errors.load(Ordering::Relaxed)
    }

    pub fn finalize(&self) -> StatsSummary {
        StatsSummary {
            processed: self.processed.load(Ordering::Relaxed),
            skipped: self.skipped.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
        }
    }
}

impl std::fmt::Display for StatsSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Processed: {} | Skipped: {} | Errors: {}",
            self.processed, self.skipped, self.errors
        )
    }
}

use std::time::{Duration, Instant};

/// Helper para gerenciar timing de operações no UI (compatível com Dioxus signals)
#[derive(Clone, Copy, Debug)]
pub struct OperationTimer {
    start_time: Instant,
    last_progress_time: Instant,
    last_progress_value: f64,
}

impl OperationTimer {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            start_time: now,
            last_progress_time: now,
            last_progress_value: 0.0,
        }
    }

    /// Get elapsed time formatted as string
    /// Returns a formatted string like "2m 30s" or "45s"
    pub fn elapsed_string(&self) -> String {
        let elapsed = self.start_time.elapsed();
        format_duration(elapsed)
    }

    /// Estimate time remaining based on current progress (0.0 to 1.0)
    /// Returns a formatted string like "1m 15s" or None if estimation isn't reliable yet
    pub fn estimate_remaining(&mut self, current_progress: f64) -> Option<String> {
        if current_progress <= 0.0 || current_progress >= 1.0 {
            return None;
        }

        let elapsed_since_last = self.last_progress_time.elapsed();
        let progress_since_last = current_progress - self.last_progress_value;

        // Need at least some progress and time to estimate
        if progress_since_last <= 0.001 || elapsed_since_last.as_secs_f64() < 0.1 {
            return None;
        }

        let rate = progress_since_last / elapsed_since_last.as_secs_f64();
        let remaining_progress = 1.0 - current_progress;
        let estimated_remaining_secs = remaining_progress / rate;

        self.last_progress_time = Instant::now();
        self.last_progress_value = current_progress;

        Some(format_duration(Duration::from_secs_f64(
            estimated_remaining_secs,
        )))
    }

    /// Reset progress tracking for new estimation cycle
    pub fn reset_progress_tracking(&mut self) {
        self.last_progress_time = Instant::now();
        self.last_progress_value = 0.0;
    }
}

impl Default for OperationTimer {
    fn default() -> Self {
        Self::new()
    }
}

/// Format duration as human-readable string
/// Examples: "30s", "1m 30s", "1h 15m"
pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();

    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        let minutes = secs / 60;
        let seconds = secs % 60;
        if seconds > 0 {
            format!("{}m {}s", minutes, seconds)
        } else {
            format!("{}m", minutes)
        }
    } else {
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        if minutes > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}h", hours)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_secs(30)), "30s");
        assert_eq!(format_duration(Duration::from_secs(90)), "1m 30s");
        assert_eq!(format_duration(Duration::from_secs(3600)), "1h");
        assert_eq!(format_duration(Duration::from_secs(3661)), "1h 1m");
    }

    #[test]
    fn test_operation_timer() {
        let timer = OperationTimer::new();
        let elapsed = timer.elapsed_string();
        assert!(elapsed.ends_with("s"));
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct IntegrityReport {
    pub verified_files: usize,
    pub errors: usize,
    pub warnings: usize,
    pub duration_ms: u128,
}

//! Health transport DTOs.

use serde::{Deserialize, Serialize};

/// Health check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Status string.
    pub status: String,
}

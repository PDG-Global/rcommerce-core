//! Job retry logic with exponential backoff

use crate::jobs::{JobError, JobProcessingResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// Helper module for Duration serialization
#[allow(dead_code)]
type SerializedDuration = std::time::Duration;

/// Type alias for custom retry function to reduce complexity
type CustomRetryFn =
    Box<dyn Fn(u32, &JobError) -> JobProcessingResult<Option<Duration>> + Send + Sync>;

/// Retry policy for failed jobs
pub enum RetryPolicy {
    /// No retries
    None,

    /// Fixed delay between retries
    Fixed {
        /// Delay between retries
        delay: Duration,

        /// Max retry attempts
        max_attempts: u32,
    },

    /// Exponential backoff
    Exponential(ExponentialBackoff),

    /// Custom retry logic
    Custom(CustomRetryFn),
}

impl std::fmt::Debug for RetryPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RetryPolicy::None => f.debug_struct("None").finish(),
            RetryPolicy::Fixed {
                delay,
                max_attempts,
            } => f
                .debug_struct("Fixed")
                .field("delay", delay)
                .field("max_attempts", max_attempts)
                .finish(),
            RetryPolicy::Exponential(backoff) => {
                f.debug_tuple("Exponential").field(backoff).finish()
            }
            RetryPolicy::Custom(_) => f.debug_struct("Custom").finish_non_exhaustive(),
        }
    }
}

impl Clone for RetryPolicy {
    fn clone(&self) -> Self {
        match self {
            RetryPolicy::None => RetryPolicy::None,
            RetryPolicy::Fixed {
                delay,
                max_attempts,
            } => RetryPolicy::Fixed {
                delay: *delay,
                max_attempts: *max_attempts,
            },
            RetryPolicy::Exponential(backoff) => RetryPolicy::Exponential(backoff.clone()),
            RetryPolicy::Custom(_) => RetryPolicy::None, // Custom cannot be cloned, fallback to None
        }
    }
}

impl RetryPolicy {
    /// Calculate retry delay for given attempt and error
    pub fn calculate_delay(
        &self,
        attempt: u32,
        error: &JobError,
    ) -> JobProcessingResult<Option<Duration>> {
        match self {
            RetryPolicy::None => Ok(None),

            RetryPolicy::Fixed {
                delay,
                max_attempts,
            } => {
                if attempt >= *max_attempts {
                    Ok(None)
                } else {
                    Ok(Some(*delay))
                }
            }

            RetryPolicy::Exponential(backoff) => Ok(backoff.calculate_delay(attempt)),

            RetryPolicy::Custom(func) => func(attempt, error),
        }
    }

    /// Check if should retry based on error
    pub fn should_retry(&self, error: &JobError) -> bool {
        match error {
            JobError::Cancelled => false,
            JobError::TimeoutMillis(_) => !matches!(self, RetryPolicy::None),
            _ => !matches!(self, RetryPolicy::None),
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy::Exponential(ExponentialBackoff::default())
    }
}

/// Exponential backoff configuration
#[derive(Debug, Clone)]
pub struct ExponentialBackoff {
    /// Initial delay
    pub initial_delay: Duration,

    /// Maximum delay
    pub max_delay: Duration,

    /// Multiplier (usually 2.0)
    pub multiplier: f64,

    /// Jitter factor (0.0 - 1.0) to randomize delays
    pub jitter: f64,
}

impl ExponentialBackoff {
    /// Create new exponential backoff
    pub fn new(initial_delay: Duration, max_delay: Duration, multiplier: f64) -> Self {
        Self {
            initial_delay,
            max_delay,
            multiplier,
            jitter: 0.1, // 10% jitter by default
        }
    }

    /// With jitter factor
    pub fn with_jitter(mut self, jitter: f64) -> Self {
        self.jitter = jitter.clamp(0.0, 1.0);
        self
    }

    /// Calculate delay for attempt
    pub fn calculate_delay(&self, attempt: u32) -> Option<Duration> {
        if attempt == 0 {
            return Some(self.initial_delay);
        }

        // Calculate exponential delay
        let exponent = attempt.saturating_sub(1) as f64;
        let delay_secs = self.initial_delay.as_secs_f64() * self.multiplier.powf(exponent);

        // Cap at max delay
        let delay_secs = delay_secs.min(self.max_delay.as_secs_f64());

        // Apply jitter
        let jitter_range = delay_secs * self.jitter;
        let jitter = if self.jitter > 0.0 {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            rng.gen_range(-jitter_range..=jitter_range)
        } else {
            0.0
        };

        let final_delay = (delay_secs + jitter).max(0.0);

        Some(Duration::from_secs_f64(final_delay))
    }
}

impl Default for ExponentialBackoff {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(3600), // 1 hour
            multiplier: 2.0,
            jitter: 0.1,
        }
    }
}

/// Retry attempt information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryAttempt {
    /// Attempt number (1-indexed)
    pub attempt: u32,

    /// Previous error
    pub error: JobError,

    /// Delay before this attempt (in milliseconds)
    #[serde(with = "duration_millis")]
    pub delay: Duration,

    /// Timestamp of attempt
    pub attempted_at: i64,
}

/// Helper module for Duration serialization in milliseconds
mod duration_millis {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

impl RetryAttempt {
    /// Create new retry attempt
    pub fn new(attempt: u32, error: JobError, delay: Duration) -> Self {
        Self {
            attempt,
            error,
            delay,
            attempted_at: chrono::Utc::now().timestamp(),
        }
    }
}

/// Retry history for a job
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RetryHistory {
    /// All retry attempts
    pub attempts: Vec<RetryAttempt>,
}

impl RetryHistory {
    /// Create empty history
    pub fn new() -> Self {
        Self {
            attempts: Vec::new(),
        }
    }

    /// Add attempt to history
    pub fn add_attempt(&mut self, attempt: RetryAttempt) {
        self.attempts.push(attempt);
    }

    /// Get attempt count
    pub fn attempt_count(&self) -> u32 {
        self.attempts.len() as u32
    }

    /// Get total delay time
    pub fn total_delay(&self) -> Duration {
        self.attempts.iter().map(|a| a.delay).sum()
    }

    /// Get last error
    pub fn last_error(&self) -> Option<&JobError> {
        self.attempts.last().map(|a| &a.error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::jobs::JobError;

    #[test]
    fn test_exponential_backoff() {
        // Use no jitter for predictable test results
        let backoff = ExponentialBackoff {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(3600),
            multiplier: 2.0,
            jitter: 0.0, // No jitter for predictable tests
        };

        // attempt 0 uses initial_delay directly
        let delay0 = backoff.calculate_delay(0).unwrap();
        assert_eq!(delay0, Duration::from_secs(1));

        // attempt 1: 1s * 2^0 = 1s (exponent = attempt-1 = 0)
        let delay1 = backoff.calculate_delay(1).unwrap();
        assert_eq!(delay1, Duration::from_secs(1));

        // attempt 2: 1s * 2^1 = 2s (exponent = attempt-1 = 1)
        let delay2 = backoff.calculate_delay(2).unwrap();
        assert_eq!(delay2, Duration::from_secs(2));

        // attempt 3: 1s * 2^2 = 4s
        let delay3 = backoff.calculate_delay(3).unwrap();
        assert_eq!(delay3, Duration::from_secs(4));
    }

    #[test]
    fn test_fixed_retry_policy() {
        let policy = RetryPolicy::Fixed {
            delay: Duration::from_secs(10),
            max_attempts: 3,
        };

        let error = JobError::Execution("test".to_string());

        // First attempt
        let delay1 = policy.calculate_delay(0, &error).unwrap();
        assert_eq!(delay1, Some(Duration::from_secs(10)));

        // Last attempt
        let delay3 = policy.calculate_delay(2, &error).unwrap();
        assert_eq!(delay3, Some(Duration::from_secs(10)));

        // After max attempts
        let delay4 = policy.calculate_delay(3, &error).unwrap();
        assert_eq!(delay4, None);
    }

    #[test]
    fn test_no_retry_policy() {
        let policy = RetryPolicy::None;
        let error = JobError::Execution("test".to_string());

        let delay = policy.calculate_delay(0, &error).unwrap();
        assert_eq!(delay, None);
    }

    #[test]
    fn test_should_retry() {
        let policy = RetryPolicy::default();

        assert!(policy.should_retry(&JobError::TimeoutMillis(30000)));
        assert!(policy.should_retry(&JobError::Execution("error".to_string())));
        assert!(!policy.should_retry(&JobError::Cancelled));
    }

    #[test]
    fn test_retry_history() {
        let mut history = RetryHistory::new();
        assert_eq!(history.attempt_count(), 0);

        let error = JobError::Execution("test".to_string());
        let attempt = RetryAttempt::new(1, error, Duration::from_secs(1));
        history.add_attempt(attempt);

        assert_eq!(history.attempt_count(), 1);
        assert!(history.last_error().is_some());
    }
}

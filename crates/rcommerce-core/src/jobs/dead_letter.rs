//! Dead letter queue for failed jobs that exhausted all retry attempts

use crate::jobs::job::Job;
use crate::jobs::{JobError, RetryHistory};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetter {
    pub id: Uuid,
    pub job: Job,
    pub final_error: JobError,
    pub retry_history: RetryHistory,
    pub created_at: DateTime<Utc>,
}

impl DeadLetter {
    pub fn new(job: Job, final_error: JobError, retry_history: RetryHistory) -> Self {
        Self {
            id: Uuid::new_v4(),
            job,
            final_error,
            retry_history,
            created_at: Utc::now(),
        }
    }
}

pub struct DeadLetterQueue {
    queue: VecDeque<DeadLetter>,
    max_size: usize,
}

impl DeadLetterQueue {
    pub fn new(max_size: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            max_size,
        }
    }

    pub fn push(&mut self, dead_letter: DeadLetter) {
        if self.queue.len() >= self.max_size {
            self.queue.pop_front();
        }
        self.queue.push_back(dead_letter);
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &DeadLetter> {
        self.queue.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dead_letter_creation() {
        let job = Job::new("test_job", serde_json::json!({}), "default");
        let error = JobError::Execution("test error".to_string());
        let history = RetryHistory::new();

        let dead_letter = DeadLetter::new(job.clone(), error, history);
        assert_eq!(dead_letter.job.id, job.id);
    }

    #[test]
    fn test_dead_letter_queue() {
        let mut queue = DeadLetterQueue::new(2);
        assert!(queue.is_empty());

        let job1 = Job::new("test1", serde_json::json!({}), "default");
        let job2 = Job::new("test2", serde_json::json!({}), "default");
        let job3 = Job::new("test3", serde_json::json!({}), "default");

        let error = JobError::Execution("error".to_string());
        let history = RetryHistory::new();

        queue.push(DeadLetter::new(job1, error.clone(), history.clone()));
        queue.push(DeadLetter::new(job2, error.clone(), history.clone()));
        assert_eq!(queue.len(), 2);

        // This should push out job1
        queue.push(DeadLetter::new(job3, error, history));
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.iter().count(), 2);
    }
}

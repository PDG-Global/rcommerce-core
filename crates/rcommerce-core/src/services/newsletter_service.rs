use crate::{repository::PostgresNewsletterRepository, Error, Result};

/// Newsletter subscription service
///
/// Subscriptions are idempotent: subscribing an existing active subscriber is
/// a no-op, and a previously unsubscribed email is re-subscribed.
#[derive(Clone)]
pub struct NewsletterService {
    repository: PostgresNewsletterRepository,
}

impl NewsletterService {
    pub fn new(repository: PostgresNewsletterRepository) -> Self {
        Self { repository }
    }

    /// Subscribe an email address (trims + lowercases, idempotent)
    pub async fn subscribe(&self, email: &str) -> Result<()> {
        let email = email.trim().to_lowercase();

        if email.is_empty() || !email.contains('@') || email.len() > 255 {
            return Err(Error::validation("A valid email address is required"));
        }

        if let Some(existing) = self.repository.find_by_email(&email).await? {
            if existing.status != "subscribed" {
                self.repository
                    .set_status(existing.id, "subscribed")
                    .await?;
            }
            return Ok(());
        }

        self.repository.create(&email).await?;
        Ok(())
    }
}

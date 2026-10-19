//! PostgreSQL repository for newsletter subscribers

use uuid::Uuid;

use super::PostgresDb;
use crate::{models::NewsletterSubscriber, Result};

/// PostgreSQL newsletter subscriber repository
#[derive(Clone)]
pub struct PostgresNewsletterRepository {
    db: PostgresDb,
}

impl PostgresNewsletterRepository {
    pub fn new(db: PostgresDb) -> Self {
        Self { db }
    }

    /// Find a subscriber by email (case-insensitive). The caller passes the
    /// already-normalized (trimmed + lowercased) email.
    pub async fn find_by_email(&self, email: &str) -> Result<Option<NewsletterSubscriber>> {
        let subscriber = sqlx::query_as::<_, NewsletterSubscriber>(
            "SELECT * FROM newsletter_subscribers WHERE lower(email) = lower($1)",
        )
        .bind(email)
        .fetch_optional(self.db.pool())
        .await?;

        Ok(subscriber)
    }

    /// Insert a new subscriber (email must already be normalized)
    pub async fn create(&self, email: &str) -> Result<NewsletterSubscriber> {
        let subscriber = sqlx::query_as::<_, NewsletterSubscriber>(
            r#"
            INSERT INTO newsletter_subscribers (id, email, status, created_at)
            VALUES ($1, $2, 'subscribed', NOW())
            RETURNING *
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(email)
        .fetch_one(self.db.pool())
        .await?;

        Ok(subscriber)
    }

    /// Update the subscription status (e.g. re-subscribe after unsubscribe)
    pub async fn set_status(&self, id: Uuid, status: &str) -> Result<()> {
        sqlx::query("UPDATE newsletter_subscribers SET status = $2 WHERE id = $1")
            .bind(id)
            .bind(status)
            .execute(self.db.pool())
            .await?;

        Ok(())
    }
}

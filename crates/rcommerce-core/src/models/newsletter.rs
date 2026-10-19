use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

/// Newsletter subscriber entity
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NewsletterSubscriber {
    pub id: Uuid,
    pub email: String,
    /// 'subscribed' | 'unsubscribed'
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// Newsletter subscribe request
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubscribeNewsletterRequest {
    #[validate(email)]
    pub email: String,
}

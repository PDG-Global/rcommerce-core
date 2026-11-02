//! Database access utilities

pub mod migrate;

use once_cell::sync::Lazy;
use sqlx::PgPool;
use std::sync::Arc;

static POOL: Lazy<Arc<PgPool>> = Lazy::new(|| {
    // In production, this would be initialized from config
    Arc::new(PgPool::connect_lazy("postgres://localhost/rcommerce").expect("Failed to create pool"))
});

/// Get the database pool
pub async fn get_pool() -> Result<Arc<PgPool>, crate::Error> {
    Ok(POOL.clone())
}

/// Initialize database pool with connection string
pub fn init_pool(_connection_string: &str) -> Result<(), crate::Error> {
    // This would reinitialize the pool if needed
    Ok(())
}

/// Names of the jobs guarded by [`run_exclusive`]
pub mod job_names {
    /// Abandoned-order sweep (background loop)
    pub const ABANDONED_ORDER_SWEEP: &str = "abandoned_order_sweep";
    /// Subscription billing (background loop and admin trigger)
    pub const SUBSCRIPTION_BILLING: &str = "subscription_billing";
    /// Dunning payment retries (admin trigger)
    pub const DUNNING_RETRIES: &str = "dunning_retries";
}

/// Run `job` unless another database session is running the job of the same
/// `name`; returns `None` when it was skipped.
///
/// The guard is a transaction-level advisory lock held by one pooled
/// connection for the run, released when the run ends, fails or is cancelled.
/// Instances sharing a database, or an admin trigger racing the background
/// loop, thus never run the same job twice at once.
pub async fn run_exclusive<T, F>(pool: &PgPool, name: &str, job: F) -> crate::Result<Option<T>>
where
    F: std::future::Future<Output = crate::Result<T>>,
{
    let mut lock = pool.begin().await?;
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtext($1))")
        .bind(format!("rcommerce:{}", name))
        .fetch_one(&mut *lock)
        .await?;
    if !acquired {
        return Ok(None);
    }
    let result = job.await;
    // Nothing was written through this transaction; ending it frees the lock
    lock.rollback().await?;
    result.map(Some)
}

#[cfg(test)]
mod run_exclusive_tests {
    use super::run_exclusive;
    use tokio::sync::oneshot;

    /// DB-backed: requires PostgreSQL via TEST_DATABASE_URL; skipped without it
    async fn test_pool() -> Option<sqlx::PgPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .ok()
    }

    #[tokio::test]
    async fn a_job_does_not_run_twice_at_once() {
        let Some(pool) = test_pool().await else {
            eprintln!("Skipping: TEST_DATABASE_URL not set");
            return;
        };
        let name = format!("test-{}", uuid::Uuid::new_v4());
        let (started_tx, started_rx) = oneshot::channel();
        let (finish_tx, finish_rx) = oneshot::channel::<()>();
        let first = {
            let (pool, name) = (pool.clone(), name.clone());
            tokio::spawn(async move {
                run_exclusive(&pool, &name, async move {
                    started_tx.send(()).ok();
                    finish_rx.await.ok();
                    Ok(1)
                })
                .await
            })
        };
        started_rx.await.unwrap();

        // Same name: skipped while the first run holds the lock
        assert_eq!(
            run_exclusive(&pool, &name, async { Ok(2) }).await.unwrap(),
            None
        );
        // Another name is independent
        let other = format!("{name}-other");
        assert_eq!(
            run_exclusive(&pool, &other, async { Ok(3) }).await.unwrap(),
            Some(3)
        );

        finish_tx.send(()).unwrap();
        assert_eq!(first.await.unwrap().unwrap(), Some(1));
        // Free again once the first run ended
        assert_eq!(
            run_exclusive(&pool, &name, async { Ok(4) }).await.unwrap(),
            Some(4)
        );
    }

    #[tokio::test]
    async fn a_failed_or_cancelled_run_frees_the_lock() {
        let Some(pool) = test_pool().await else {
            eprintln!("Skipping: TEST_DATABASE_URL not set");
            return;
        };
        let name = format!("test-{}", uuid::Uuid::new_v4());
        let failed = run_exclusive(&pool, &name, async {
            Err::<(), _>(crate::Error::validation("boom"))
        })
        .await;
        assert!(failed.is_err());
        assert_eq!(
            run_exclusive(&pool, &name, async { Ok(1) }).await.unwrap(),
            Some(1)
        );

        // A run dropped mid-way (e.g. its task cancelled) releases the lock too,
        // once its connection's pending rollback has gone through
        let pending = run_exclusive(&pool, &name, std::future::pending::<crate::Result<()>>());
        let _ = tokio::time::timeout(std::time::Duration::from_millis(200), pending).await;
        let mut freed = None;
        for _ in 0..20 {
            freed = run_exclusive(&pool, &name, async { Ok(2) }).await.unwrap();
            if freed.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(freed, Some(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_pool() {
        let pool = get_pool().await;
        assert!(pool.is_ok());
    }
}

//! Store locations (branches, pickup points): storefront details, weekly opening
//! hours, special days and translations, stored on `inventory_locations` and
//! the tables of migration 023.

use std::collections::HashMap;

use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::location::{
    ClosureInput, LocationClosure, LocationInput, LocationTranslationInput, OpeningRange,
    StoreLocation,
};
use crate::{Error, Result};

/// Location columns plus its translation in the requested locale (`$1`)
const LOCATION_SELECT: &str = "SELECT l.id, l.name, l.code, l.slug, l.is_active, l.is_public,
       l.is_pickup_point, l.phone, l.email, l.address_line1, l.address_line2, l.postal_code,
       l.city, l.country_code, l.latitude, l.longitude, l.timezone, l.pickup_lead_minutes,
       l.services, l.manager_name, l.sort_order, t.description, t.pickup_instructions,
       l.created_at, l.updated_at
FROM inventory_locations l
LEFT JOIN location_translations t ON t.location_id = l.id AND t.locale = $1";

/// Duplicate code or slug becomes a client error
fn map_constraint(e: sqlx::Error) -> Error {
    match &e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => {
            Error::validation("Another location already uses this code or slug")
        }
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23503") => {
            Error::not_found("Location not found")
        }
        _ => Error::Database(e),
    }
}

/// Store locations, their opening hours, special days and translations
pub struct LocationRepository {
    pool: PgPool,
}

impl LocationRepository {
    /// A repository on the given connection pool
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Locations in display order; public ones only (active and listed) unless
    /// `public_only` is false (admin)
    pub async fn list(
        &self,
        public_only: bool,
        locale: Option<&str>,
    ) -> Result<Vec<StoreLocation>> {
        Ok(sqlx::query_as::<_, StoreLocation>(&format!(
            "{LOCATION_SELECT}
             WHERE NOT $2 OR (l.is_public AND l.is_active)
             ORDER BY l.sort_order, l.name"
        ))
        .bind(locale)
        .bind(public_only)
        .fetch_all(&self.pool)
        .await?)
    }

    /// One location by id or slug
    pub async fn get(
        &self,
        id_or_slug: &str,
        public_only: bool,
        locale: Option<&str>,
    ) -> Result<Option<StoreLocation>> {
        Ok(sqlx::query_as::<_, StoreLocation>(&format!(
            "{LOCATION_SELECT}
             WHERE (l.id = $3 OR l.slug = $4) AND (NOT $2 OR (l.is_public AND l.is_active))"
        ))
        .bind(locale)
        .bind(public_only)
        .bind(Uuid::parse_str(id_or_slug).ok())
        .bind(id_or_slug)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Weekly hours of several locations, in one query
    pub async fn opening_hours(&self, ids: &[Uuid]) -> Result<HashMap<Uuid, Vec<OpeningRange>>> {
        #[derive(sqlx::FromRow)]
        struct Row {
            location_id: Uuid,
            #[sqlx(flatten)]
            range: OpeningRange,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT location_id, weekday, opens_at, closes_at FROM location_opening_hours
             WHERE location_id = ANY($1) ORDER BY weekday, opens_at",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;
        let mut by_location: HashMap<Uuid, Vec<OpeningRange>> = HashMap::new();
        for row in rows {
            by_location
                .entry(row.location_id)
                .or_default()
                .push(row.range);
        }
        Ok(by_location)
    }

    /// Special days of several locations between two dates (inclusive), in one query
    pub async fn closures(
        &self,
        ids: &[Uuid],
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<HashMap<Uuid, Vec<LocationClosure>>> {
        let rows = sqlx::query_as::<_, LocationClosure>(
            "SELECT id, location_id, date, opens_at, closes_at, reason FROM location_closures
             WHERE location_id = ANY($1) AND date BETWEEN $2 AND $3
             ORDER BY date, opens_at NULLS FIRST",
        )
        .bind(ids)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        let mut by_location: HashMap<Uuid, Vec<LocationClosure>> = HashMap::new();
        for row in rows {
            by_location.entry(row.location_id).or_default().push(row);
        }
        Ok(by_location)
    }

    /// Create a location
    pub async fn create(&self, input: &LocationInput) -> Result<StoreLocation> {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO inventory_locations (name, code, slug, is_active, is_public, is_pickup_point,
                 phone, email, address_line1, address_line2, postal_code, city, country_code,
                 latitude, longitude, timezone, pickup_lead_minutes, services, manager_name, sort_order)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
             RETURNING id",
        )
        .bind(&input.name)
        .bind(&input.code)
        .bind(&input.slug)
        .bind(input.is_active)
        .bind(input.is_public)
        .bind(input.is_pickup_point)
        .bind(&input.phone)
        .bind(&input.email)
        .bind(&input.address_line1)
        .bind(&input.address_line2)
        .bind(&input.postal_code)
        .bind(&input.city)
        .bind(&input.country_code)
        .bind(input.latitude)
        .bind(input.longitude)
        .bind(&input.timezone)
        .bind(input.pickup_lead_minutes)
        .bind(&input.services)
        .bind(&input.manager_name)
        .bind(input.sort_order)
        .fetch_one(&self.pool)
        .await
        .map_err(map_constraint)?;
        self.get(&id.to_string(), false, None)
            .await?
            .ok_or_else(|| Error::not_found("Location not found"))
    }

    /// Replace a location's details; None when it does not exist
    pub async fn update(&self, id: Uuid, input: &LocationInput) -> Result<Option<StoreLocation>> {
        let updated = sqlx::query(
            "UPDATE inventory_locations SET name = $2, code = $3, slug = $4, is_active = $5,
                 is_public = $6, is_pickup_point = $7, phone = $8, email = $9, address_line1 = $10,
                 address_line2 = $11, postal_code = $12, city = $13, country_code = $14,
                 latitude = $15, longitude = $16, timezone = $17, pickup_lead_minutes = $18,
                 services = $19, manager_name = $20, sort_order = $21, updated_at = NOW()
             WHERE id = $1",
        )
        .bind(id)
        .bind(&input.name)
        .bind(&input.code)
        .bind(&input.slug)
        .bind(input.is_active)
        .bind(input.is_public)
        .bind(input.is_pickup_point)
        .bind(&input.phone)
        .bind(&input.email)
        .bind(&input.address_line1)
        .bind(&input.address_line2)
        .bind(&input.postal_code)
        .bind(&input.city)
        .bind(&input.country_code)
        .bind(input.latitude)
        .bind(input.longitude)
        .bind(&input.timezone)
        .bind(input.pickup_lead_minutes)
        .bind(&input.services)
        .bind(&input.manager_name)
        .bind(input.sort_order)
        .execute(&self.pool)
        .await
        .map_err(map_constraint)?;
        if updated.rows_affected() == 0 {
            return Ok(None);
        }
        self.get(&id.to_string(), false, None).await
    }

    /// Replace the weekly hours of a location (one transaction)
    pub async fn replace_opening_hours(&self, id: Uuid, ranges: &[OpeningRange]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM location_opening_hours WHERE location_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for range in ranges {
            sqlx::query(
                "INSERT INTO location_opening_hours (location_id, weekday, opens_at, closes_at)
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(id)
            .bind(range.weekday)
            .bind(range.opens_at)
            .bind(range.closes_at)
            .execute(&mut *tx)
            .await
            .map_err(map_constraint)?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Add a special day
    pub async fn add_closure(&self, id: Uuid, input: &ClosureInput) -> Result<LocationClosure> {
        sqlx::query_as::<_, LocationClosure>(
            "INSERT INTO location_closures (location_id, date, opens_at, closes_at, reason)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, location_id, date, opens_at, closes_at, reason",
        )
        .bind(id)
        .bind(input.date)
        .bind(input.opens_at)
        .bind(input.closes_at)
        .bind(&input.reason)
        .fetch_one(&self.pool)
        .await
        .map_err(map_constraint)
    }

    /// Remove a special day; false when it does not exist
    pub async fn delete_closure(&self, location_id: Uuid, closure_id: Uuid) -> Result<bool> {
        let result =
            sqlx::query("DELETE FROM location_closures WHERE id = $1 AND location_id = $2")
                .bind(closure_id)
                .bind(location_id)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Create or replace a location's text in one locale
    pub async fn upsert_translation(
        &self,
        id: Uuid,
        locale: &str,
        input: &LocationTranslationInput,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO location_translations (location_id, locale, description, pickup_instructions)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (location_id, locale)
             DO UPDATE SET description = EXCLUDED.description,
                           pickup_instructions = EXCLUDED.pickup_instructions",
        )
        .bind(id)
        .bind(locale)
        .bind(&input.description)
        .bind(&input.pickup_instructions)
        .execute(&self.pool)
        .await
        .map_err(map_constraint)?;
        Ok(())
    }

    /// Delete a location's text in one locale; false when there was none
    pub async fn delete_translation(&self, id: Uuid, locale: &str) -> Result<bool> {
        let result =
            sqlx::query("DELETE FROM location_translations WHERE location_id = $1 AND locale = $2")
                .bind(id)
                .bind(locale)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    /// A pool on TEST_DATABASE_URL with migrations applied; None skips the test
    async fn test_pool() -> Option<PgPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .ok()?;
        crate::Migrator::new(pool.clone()).migrate().await.ok()?;
        Some(pool)
    }

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn input(code: &str, slug: &str, public: bool) -> LocationInput {
        serde_json::from_value(serde_json::json!({
            "name": format!("Branch {code}"),
            "code": code,
            "slug": slug,
            "is_public": public,
            "is_pickup_point": public,
            "timezone": "Europe/Brussels",
            "country_code": "BE",
            "services": ["pickup", "showroom"]
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn locations_hours_closures_and_translations_against_postgres() {
        let Some(pool) = test_pool().await else {
            eprintln!("Skipping location repository test: set TEST_DATABASE_URL");
            return;
        };
        let repo = LocationRepository::new(pool.clone());
        let tag = Uuid::new_v4().simple().to_string()[..8].to_string();

        let public = repo
            .create(&input(&format!("P{tag}"), &format!("public-{tag}"), true))
            .await
            .unwrap();
        let private = repo
            .create(&input(&format!("X{tag}"), &format!("private-{tag}"), false))
            .await
            .unwrap();
        assert_eq!(public.services, vec!["pickup", "showroom"]);
        assert_eq!(public.timezone, "Europe/Brussels");

        // Only public, active locations are listed and fetchable on the storefront
        let listed = repo.list(true, None).await.unwrap();
        assert!(listed.iter().any(|l| l.id == public.id));
        assert!(!listed.iter().any(|l| l.id == private.id));
        assert!(repo
            .get(&format!("private-{tag}"), true, None)
            .await
            .unwrap()
            .is_none());
        assert!(repo
            .get(&private.id.to_string(), false, None)
            .await
            .unwrap()
            .is_some());

        // A duplicate slug or code is a validation error
        let duplicate = repo
            .create(&input(&format!("Q{tag}"), &format!("public-{tag}"), true))
            .await;
        assert!(matches!(duplicate, Err(Error::Validation(_))));

        // Weekly hours are replaced as a whole; special days are per date
        let ranges = vec![
            OpeningRange {
                weekday: 1,
                opens_at: t(7, 0),
                closes_at: t(12, 0),
            },
            OpeningRange {
                weekday: 1,
                opens_at: t(13, 0),
                closes_at: t(18, 0),
            },
        ];
        repo.replace_opening_hours(public.id, &ranges)
            .await
            .unwrap();
        repo.replace_opening_hours(public.id, &ranges[..1])
            .await
            .unwrap();
        let hours = repo.opening_hours(&[public.id, private.id]).await.unwrap();
        assert_eq!(hours.get(&public.id).map(Vec::len), Some(1));
        assert!(!hours.contains_key(&private.id));

        let christmas = NaiveDate::from_ymd_opt(2026, 12, 25).unwrap();
        let closure = repo
            .add_closure(
                public.id,
                &ClosureInput {
                    date: christmas,
                    opens_at: None,
                    closes_at: None,
                    reason: Some("Noël".to_string()),
                },
            )
            .await
            .unwrap();
        let closures = repo
            .closures(&[public.id], christmas, christmas)
            .await
            .unwrap();
        assert_eq!(closures[&public.id], vec![closure.clone()]);
        assert!(repo.delete_closure(public.id, closure.id).await.unwrap());
        assert!(!repo.delete_closure(public.id, closure.id).await.unwrap());

        // Translations follow the requested locale and fall back to none
        repo.upsert_translation(
            public.id,
            "nl",
            &LocationTranslationInput {
                description: Some("Bouwmaterialen".to_string()),
                pickup_instructions: Some("Aan de toonbank".to_string()),
            },
        )
        .await
        .unwrap();
        let nl = repo
            .get(&public.id.to_string(), true, Some("nl"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(nl.description.as_deref(), Some("Bouwmaterialen"));
        let fr = repo
            .get(&public.id.to_string(), true, Some("fr"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fr.description, None);

        // Updating keeps the id; an unknown id is None
        let mut changed = input(&format!("P{tag}"), &format!("public-{tag}"), true);
        changed.pickup_lead_minutes = 30;
        let updated = repo.update(public.id, &changed).await.unwrap().unwrap();
        assert_eq!(updated.pickup_lead_minutes, 30);
        assert!(repo
            .update(Uuid::new_v4(), &changed)
            .await
            .unwrap()
            .is_none());

        sqlx::query("DELETE FROM inventory_locations WHERE id = ANY($1)")
            .bind(vec![public.id, private.id])
            .execute(&pool)
            .await
            .unwrap();
    }
}

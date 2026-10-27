//! Store locations: the branches and pickup points customers see (address,
//! hours, open now, when an order would be ready), and their management.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Extension, Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use rcommerce_core::common::locale::normalize_locale;
use rcommerce_core::models::location::{
    ClosureInput, LocationClosure, LocationInput, LocationTranslationInput, OpeningHoursInput,
    OpeningRange, StoreLocation,
};
use rcommerce_core::repository::LocationRepository;
use rcommerce_core::services::location_service::{distance_km, OpeningSchedule, OpeningStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::JwtAuth;
use crate::state::AppState;

/// Special days listed with a location, and considered for its status
const SPECIAL_DAYS_AHEAD: i64 = 31;

/// `?lang=` for translated text, `?near=lat,lng` to sort by distance
#[derive(Debug, Default, Deserialize)]
pub struct LocationQuery {
    lang: Option<String>,
    near: Option<String>,
}

/// A location as the storefront shows it
#[derive(Debug, Serialize)]
pub struct LocationView {
    #[serde(flatten)]
    pub location: StoreLocation,
    pub opening_hours: Vec<OpeningRange>,
    /// Closed days and special hours in the coming weeks
    pub special_days: Vec<LocationClosure>,
    pub status: OpeningStatus,
    /// When an order placed now could be collected (pickup points only)
    pub pickup_ready_at: Option<DateTime<Utc>>,
    /// From `?near=` (kilometres)
    pub distance_km: Option<f64>,
    /// Locale of the translated text, when one was requested and found
    pub locale: Option<String>,
}

/// "50.41,4.52" -> (50.41, 4.52); anything else is ignored
fn parse_near(near: Option<&str>) -> Option<(f64, f64)> {
    let (lat, lng) = near?.split_once(',')?;
    let point = (lat.trim().parse().ok()?, lng.trim().parse().ok()?);
    ((-90.0..=90.0).contains(&point.0) && (-180.0..=180.0).contains(&point.1)).then_some(point)
}

/// Hours, special days, status and pickup time for each location, with two
/// queries for the whole list
async fn views(
    repo: &LocationRepository,
    locations: Vec<StoreLocation>,
    locale: Option<&str>,
    near: Option<(f64, f64)>,
    now: DateTime<Utc>,
) -> Result<Vec<LocationView>, rcommerce_core::Error> {
    let ids: Vec<Uuid> = locations.iter().map(|l| l.id).collect();
    // From yesterday (UTC) so a location's local "today" is always covered
    let from = (now - Duration::days(1)).date_naive();
    let to = (now + Duration::days(SPECIAL_DAYS_AHEAD)).date_naive();
    let (mut hours, mut closures) =
        tokio::try_join!(repo.opening_hours(&ids), repo.closures(&ids, from, to))?;

    let mut views: Vec<LocationView> = locations
        .into_iter()
        .map(|location| {
            let opening_hours = hours.remove(&location.id).unwrap_or_default();
            let special_days = closures.remove(&location.id).unwrap_or_default();
            location_view(location, opening_hours, special_days, now, near, locale)
        })
        .collect();
    if near.is_some() {
        views.sort_by(|a, b| {
            a.distance_km
                .unwrap_or(f64::MAX)
                .total_cmp(&b.distance_km.unwrap_or(f64::MAX))
        });
    }
    Ok(views)
}

/// One location at `now`: status, upcoming special days, pickup time and the
/// distance from `near`
fn location_view(
    location: StoreLocation,
    opening_hours: Vec<OpeningRange>,
    special_days: Vec<LocationClosure>,
    now: DateTime<Utc>,
    near: Option<(f64, f64)>,
    locale: Option<&str>,
) -> LocationView {
    let schedule = OpeningSchedule::new(
        &location.timezone,
        opening_hours.clone(),
        special_days.clone(),
    );
    let status = schedule.status_at(now);
    let pickup_ready_at = location
        .is_pickup_point
        .then(|| schedule.pickup_ready_at(now, location.pickup_lead_minutes))
        .flatten();
    let distance_km = near
        .zip(location.latitude.zip(location.longitude))
        .map(|(from, to)| (distance_km(from, to) * 10.0).round() / 10.0);
    let translated = location.description.is_some() || location.pickup_instructions.is_some();
    LocationView {
        locale: locale.filter(|_| translated).map(str::to_string),
        location,
        opening_hours,
        special_days: special_days
            .into_iter()
            .filter(|d| d.date >= now.date_naive())
            .collect(),
        status,
        pickup_ready_at,
        distance_km,
    }
}

/// Public locations, in display order or nearest first with `?near=`
pub async fn list_locations(
    State(state): State<AppState>,
    Query(query): Query<LocationQuery>,
) -> Result<Json<Vec<LocationView>>, rcommerce_core::Error> {
    let locale = query.lang.as_deref().and_then(normalize_locale);
    let repo = LocationRepository::new(state.db.pool().clone());
    let locations = repo.list(true, locale.as_deref()).await?;
    Ok(Json(
        views(
            &repo,
            locations,
            locale.as_deref(),
            parse_near(query.near.as_deref()),
            Utc::now(),
        )
        .await?,
    ))
}

/// One public location, by id or slug
pub async fn get_location(
    State(state): State<AppState>,
    Path(id_or_slug): Path<String>,
    Query(query): Query<LocationQuery>,
) -> Result<Json<LocationView>, rcommerce_core::Error> {
    let locale = query.lang.as_deref().and_then(normalize_locale);
    let repo = LocationRepository::new(state.db.pool().clone());
    let location = repo
        .get(&id_or_slug, true, locale.as_deref())
        .await?
        .ok_or_else(|| rcommerce_core::Error::not_found("Location not found"))?;
    let mut list = views(
        &repo,
        vec![location],
        locale.as_deref(),
        parse_near(query.near.as_deref()),
        Utc::now(),
    )
    .await?;
    Ok(Json(list.remove(0)))
}

fn require_admin(auth: &JwtAuth) -> Result<(), rcommerce_core::Error> {
    if auth.is_admin() {
        Ok(())
    } else {
        Err(rcommerce_core::Error::Forbidden(
            "Admin access required".to_string(),
        ))
    }
}

fn path_locale(locale: &str) -> Result<String, rcommerce_core::Error> {
    normalize_locale(locale).ok_or_else(|| {
        rcommerce_core::Error::validation("Locale must be a 2-3 letter language code")
    })
}

/// Every location, private ones included (admin)
pub async fn admin_list_locations(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
) -> Result<Json<Vec<LocationView>>, rcommerce_core::Error> {
    require_admin(&auth)?;
    let repo = LocationRepository::new(state.db.pool().clone());
    let locations = repo.list(false, None).await?;
    Ok(Json(views(&repo, locations, None, None, Utc::now()).await?))
}

/// Create a location (admin)
pub async fn create_location(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Json(input): Json<LocationInput>,
) -> Result<(StatusCode, Json<LocationView>), rcommerce_core::Error> {
    require_admin(&auth)?;
    input.check()?;
    let repo = LocationRepository::new(state.db.pool().clone());
    let location = repo.create(&input).await?;
    let mut list = views(&repo, vec![location], None, None, Utc::now()).await?;
    Ok((StatusCode::CREATED, Json(list.remove(0))))
}

/// Replace a location's details (admin)
pub async fn update_location(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
    Json(input): Json<LocationInput>,
) -> Result<Json<LocationView>, rcommerce_core::Error> {
    require_admin(&auth)?;
    input.check()?;
    let repo = LocationRepository::new(state.db.pool().clone());
    let location = repo
        .update(id, &input)
        .await?
        .ok_or_else(|| rcommerce_core::Error::not_found("Location not found"))?;
    let mut list = views(&repo, vec![location], None, None, Utc::now()).await?;
    Ok(Json(list.remove(0)))
}

/// Replace a location's weekly opening hours (admin)
pub async fn put_opening_hours(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
    Json(input): Json<OpeningHoursInput>,
) -> Result<StatusCode, rcommerce_core::Error> {
    require_admin(&auth)?;
    input.check()?;
    let repo = LocationRepository::new(state.db.pool().clone());
    if repo.get(&id.to_string(), false, None).await?.is_none() {
        return Err(rcommerce_core::Error::not_found("Location not found"));
    }
    repo.replace_opening_hours(id, &input.ranges).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Add a closed day or special hours (admin)
pub async fn add_closure(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
    Json(input): Json<ClosureInput>,
) -> Result<(StatusCode, Json<LocationClosure>), rcommerce_core::Error> {
    require_admin(&auth)?;
    input.check()?;
    let closure = LocationRepository::new(state.db.pool().clone())
        .add_closure(id, &input)
        .await?;
    Ok((StatusCode::CREATED, Json(closure)))
}

/// Remove a closed day or special hours (admin)
pub async fn delete_closure(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path((id, closure_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, rcommerce_core::Error> {
    require_admin(&auth)?;
    if LocationRepository::new(state.db.pool().clone())
        .delete_closure(id, closure_id)
        .await?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(rcommerce_core::Error::not_found("Special day not found"))
    }
}

/// Create or replace a location's text in one language (admin)
pub async fn put_location_translation(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path((id, locale)): Path<(Uuid, String)>,
    Json(input): Json<LocationTranslationInput>,
) -> Result<StatusCode, rcommerce_core::Error> {
    require_admin(&auth)?;
    input.check()?;
    LocationRepository::new(state.db.pool().clone())
        .upsert_translation(id, &path_locale(&locale)?, &input)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Delete a location's text in one language (admin)
pub async fn delete_location_translation(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path((id, locale)): Path<(Uuid, String)>,
) -> Result<StatusCode, rcommerce_core::Error> {
    require_admin(&auth)?;
    if LocationRepository::new(state.db.pool().clone())
        .delete_translation(id, &path_locale(&locale)?)
        .await?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(rcommerce_core::Error::not_found("Translation not found"))
    }
}

/// Public location routes
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/locations", get(list_locations))
        .route("/locations/:id_or_slug", get(get_location))
}

/// Location management routes (mounted behind the admin middleware)
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/locations",
            get(admin_list_locations).post(create_location),
        )
        .route("/admin/locations/:id", put(update_location))
        .route("/admin/locations/:id/opening-hours", put(put_opening_hours))
        .route("/admin/locations/:id/closures", post(add_closure))
        .route(
            "/admin/locations/:id/closures/:closure_id",
            delete(delete_closure),
        )
        .route(
            "/admin/locations/:id/translations/:locale",
            put(put_location_translation).delete(delete_location_translation),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_parameter_accepts_coordinates_only() {
        assert_eq!(parse_near(Some("50.41,4.52")), Some((50.41, 4.52)));
        assert_eq!(parse_near(Some(" 50.41 , 4.52 ")), Some((50.41, 4.52)));
        assert_eq!(parse_near(Some("91,4")), None);
        assert_eq!(parse_near(Some("Downtown")), None);
        assert_eq!(parse_near(None), None);
    }
}

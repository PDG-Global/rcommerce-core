//! Catalogue sync administration: run a sync, see its state, choose the
//! assortment (`[catalogue] source_url` must be set).

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    routing::{get, post, put},
    Json, Router,
};
use rcommerce_core::catalogue::{Assortment, CatalogueStatus, CatalogueSync, SyncReport};
use rcommerce_core::Error;
use serde::Deserialize;

use crate::state::AppState;

/// `?full=true` copies the whole assortment again
#[derive(Debug, Default, Deserialize)]
pub struct SyncQuery {
    #[serde(default)]
    full: bool,
}

fn configured(state: &AppState) -> Result<Arc<CatalogueSync>, Error> {
    state.catalogue_sync.clone().ok_or_else(|| {
        Error::not_found("Catalogue sync is not configured ([catalogue] source_url)")
    })
}

/// Run a sync now
pub async fn run_sync(
    State(state): State<AppState>,
    Query(query): Query<SyncQuery>,
) -> Result<Json<SyncReport>, Error> {
    Ok(Json(configured(&state)?.run(query.full).await?))
}

/// Sync state and assortment
pub async fn get_status(State(state): State<AppState>) -> Result<Json<CatalogueStatus>, Error> {
    Ok(Json(configured(&state)?.status().await?))
}

/// Replace the assortment; the next sync copies it whole
pub async fn set_assortment(
    State(state): State<AppState>,
    Json(assortment): Json<Assortment>,
) -> Result<Json<CatalogueStatus>, Error> {
    let sync = configured(&state)?;
    sync.set_assortment(&assortment).await?;
    Ok(Json(sync.status().await?))
}

/// Admin catalogue routes (mounted under admin_middleware)
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/admin/catalogue", get(get_status))
        .route("/admin/catalogue/sync", post(run_sync))
        .route("/admin/catalogue/assortment", put(set_assortment))
}

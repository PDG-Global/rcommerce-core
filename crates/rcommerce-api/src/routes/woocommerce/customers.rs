//! WooCommerce-compatible customer endpoints.
//!
//! GET /wp-json/wc/v3/customers
//! GET /wp-json/wc/v3/customers/:id

use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Deserialize;

use crate::middleware::api_key_auth::ApiKeyAuth;
use crate::state::AppState;

use super::pagination;
use super::types::*;

#[derive(Debug, Deserialize, Default)]
pub struct CustomerListParams {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub search: Option<String>,
    pub email: Option<String>,
}

fn map_customer(
    customer: &rcommerce_core::models::Customer,
    addresses: &[rcommerce_core::models::Address],
) -> WcCustomer {
    let default_billing = addresses
        .iter()
        .find(|a| a.is_default_billing)
        .or(addresses.first());

    let default_shipping = addresses
        .iter()
        .find(|a| a.is_default_shipping)
        .or(addresses.first());

    let make_address = |addr: Option<&rcommerce_core::models::Address>| -> WcAddress {
        match addr {
            Some(a) => WcAddress {
                first_name: a.first_name.clone(),
                last_name: a.last_name.clone(),
                address_1: a.address1.clone(),
                address_2: a.address2.clone().unwrap_or_default(),
                city: a.city.clone(),
                state: a.state.clone().unwrap_or_default(),
                postcode: a.zip.clone(),
                country: a.country.clone(),
                email: customer.email.clone(),
                phone: a.phone.clone().unwrap_or_default(),
            },
            None => WcAddress {
                first_name: customer.first_name.clone(),
                last_name: customer.last_name.clone(),
                address_1: String::new(),
                address_2: String::new(),
                city: String::new(),
                state: String::new(),
                postcode: String::new(),
                country: String::new(),
                email: customer.email.clone(),
                phone: customer.phone.clone().unwrap_or_default(),
            },
        }
    };

    WcCustomer {
        id: customer.id.to_string(),
        email: customer.email.clone(),
        first_name: customer.first_name.clone(),
        last_name: customer.last_name.clone(),
        billing: make_address(default_billing),
        shipping: make_address(default_shipping),
        date_created: customer.created_at.to_rfc3339(),
        date_modified: customer.updated_at.to_rfc3339(),
    }
}

/// GET /wp-json/wc/v3/customers
pub async fn list_customers(
    State(state): State<AppState>,
    Extension(_auth): Extension<ApiKeyAuth>,
    Query(params): Query<CustomerListParams>,
) -> Response {
    let (page, per_page) = pagination::parse_pagination(params.page, params.per_page);

    let customers = sqlx::query_as::<_, rcommerce_core::models::Customer>(
        "SELECT * FROM customers ORDER BY created_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(per_page)
    .bind(pagination::offset(page, per_page))
    .fetch_all(state.db.pool())
    .await;

    let customers = match customers {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("WC customers list: {}", e);
            return super::super::woocommerce_error(
                500,
                "internal_server_error",
                "Internal server error.",
            );
        }
    };

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM customers")
        .fetch_one(state.db.pool())
        .await
        .unwrap_or(0);

    let mut wc_customers = Vec::new();
    for customer in &customers {
        let addresses = sqlx::query_as::<_, rcommerce_core::models::Address>(
            "SELECT * FROM addresses WHERE customer_id = $1",
        )
        .bind(customer.id)
        .fetch_all(state.db.pool())
        .await
        .unwrap_or_default();

        wc_customers.push(map_customer(customer, &addresses));
    }

    let mut response =
        Json(serde_json::to_value(&wc_customers).unwrap_or_default()).into_response();
    let headers = pagination::pagination_headers(
        total,
        page,
        per_page,
        "https://example.com/wp-json/wc/v3/customers",
    );
    response.headers_mut().extend(headers);
    response
}

/// GET /wp-json/wc/v3/customers/:id
pub async fn get_customer(
    State(state): State<AppState>,
    Extension(_auth): Extension<ApiKeyAuth>,
    Path(id): Path<String>,
) -> Response {
    let customer_id = match uuid::Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return super::super::woocommerce_error(
                400,
                "woocommerce_rest_invalid_id",
                "Invalid customer ID.",
            );
        }
    };

    let customer = sqlx::query_as::<_, rcommerce_core::models::Customer>(
        "SELECT * FROM customers WHERE id = $1",
    )
    .bind(customer_id)
    .fetch_optional(state.db.pool())
    .await;

    let customer = match customer {
        Ok(Some(c)) => c,
        Ok(None) => {
            return super::super::woocommerce_error(
                404,
                "woocommerce_rest_invalid_id",
                "Invalid ID.",
            );
        }
        Err(e) => {
            tracing::error!("WC customer get: {}", e);
            return super::super::woocommerce_error(
                500,
                "internal_server_error",
                "Internal server error.",
            );
        }
    };

    let addresses = sqlx::query_as::<_, rcommerce_core::models::Address>(
        "SELECT * FROM addresses WHERE customer_id = $1",
    )
    .bind(customer.id)
    .fetch_all(state.db.pool())
    .await
    .unwrap_or_default();

    let wc = map_customer(&customer, &addresses);
    Json(serde_json::to_value(&wc).unwrap_or_default()).into_response()
}

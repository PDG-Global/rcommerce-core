//! WooCommerce REST API v3 compatibility layer.
//!
//! Provides a WooCommerce-compatible API at `/wp-json/wc/v3/` so that
//! existing integrations (Dianxiaomi ERP, WooCommerce libraries, etc.)
//! can connect to rCommerce without modification.
//!
//! Authentication uses WooCommerce-style Basic Auth with consumer_key/
//! consumer_secret (ck_xxx / cs_xxx).

pub mod customers;
pub mod orders;
pub mod pagination;
pub mod products;
pub mod system_status;
pub mod tracking;
pub mod types;

use axum::{
    middleware,
    routing::{get, post, put},
    Router,
};
use std::sync::Arc;

use crate::middleware::wc_auth::wc_auth_middleware;
use crate::state::AppState;
use rcommerce_core::repository::PostgresApiKeyRepository;

/// WooCommerce API v3 routes, all requiring WC Basic Auth.
pub fn wc_v3_routes() -> Router<AppState> {
    // Dianxiaomi (and other ERPs) append trailing slashes to resource paths,
    // and axum treats "/orders/1/" as a different, unmatched path (answered
    // with an empty 404 that ERPs report as "response content is empty").
    // Router-level middleware cannot help (axum skips layers for unmatched
    // requests), so every route is registered twice, with and without the
    // trailing slash.
    fn dual(
        app: Router<AppState>,
        path: &str,
        methods: axum::routing::MethodRouter<AppState>,
    ) -> Router<AppState> {
        let slash = format!("{path}/");
        app.route(path, methods.clone()).route(&slash, methods)
    }

    let app = Router::new();
    // Products
    let app = dual(app, "/products", get(products::list_products));
    let app = dual(app, "/products/categories", get(products::list_categories));
    let app = dual(app, "/products/:id", get(products::get_product));
    // Orders
    let app = dual(app, "/orders", get(orders::list_orders));
    // Dianxiaomi updates orders via POST as well as PUT
    let app = dual(
        app,
        "/orders/:id",
        get(orders::get_order)
            .put(orders::update_order)
            .post(orders::update_order),
    );
    // Shipment tracking (WooCommerce Shipment Tracking plugin compatible)
    // Dianxiaomi posts a shipment note when marking an order shipped; it uses
    // an uppercase "NOTES" path segment, and axum matching is case-sensitive
    let app = dual(app, "/orders/:id/notes", post(orders::create_order_note));
    let app = dual(app, "/orders/:id/NOTES", post(orders::create_order_note));
    let app = dual(
        app,
        "/orders/:order_id/shipment-trackings",
        get(tracking::list_trackings).post(tracking::create_tracking),
    );
    let app = dual(
        app,
        "/orders/:order_id/shipment-trackings/:id",
        put(tracking::update_tracking),
    );
    // Customers
    let app = dual(app, "/customers", get(customers::list_customers));
    let app = dual(app, "/customers/:id", get(customers::get_customer));
    // System status
    let app = dual(app, "/system_status", get(system_status::system_status));
    // Unknown paths inside the WC namespace must still return a JSON body
    // (an empty 404/405 is reported by ERPs as "response content is empty")
    app.fallback(|| async {
        super::woocommerce_error(
            404,
            "woocommerce_rest_invalid_endpoint",
            "No route found for the WC API path.",
        )
    })
}

/// Mount the WooCommerce compatibility layer onto the main router.
///
/// Adds routes under `/wp-json/wc/v3/` with the WooCommerce auth middleware.
pub fn mount_wc_compat(
    mut app: Router<AppState>,
    api_key_repo: Arc<PostgresApiKeyRepository>,
) -> Router<AppState> {
    let wc_routes = wc_v3_routes().layer(middleware::from_fn_with_state(
        api_key_repo,
        wc_auth_middleware,
    ));

    app = app.nest("/wp-json/wc/v3", wc_routes);

    // WooCommerce API discovery endpoint (with and without trailing slash)
    let wc_index = get(|| async {
        axum::Json(serde_json::json!({
            "namespace": "wc/v3",
            "routes": {
                "/wc/v3/products": { "methods": ["GET"] },
                "/wc/v3/products/categories": { "methods": ["GET"] },
                "/wc/v3/products/{id}": { "methods": ["GET"] },
                "/wc/v3/orders": { "methods": ["GET", "POST"] },
                "/wc/v3/orders/{id}": { "methods": ["GET", "PUT", "POST"] },
                "/wc/v3/customers": { "methods": ["GET", "POST"] },
                "/wc/v3/customers/{id}": { "methods": ["GET", "PUT", "DELETE"] },
                "/wc/v3/system_status": { "methods": ["GET"] },
            },
            "authentication": {
                "oauth1": { "version": "1.0" },
                "basic": true
            },
            "settings": {
                "api_enabled": true,
                "force_ssl": true,
                "currency": "USD",
            }
        }))
    });
    app = app
        .route("/wp-json/wc/v3", wc_index.clone())
        .route("/wp-json/wc/v3/", wc_index);

    app
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_strip_wc_trailing_slash_uri_parse() {
        let pq = "/wp-json/wc/v3/orders/8391860615509621128/shipment-trackings";
        let uri: Result<axum::http::Uri, _> = pq.parse();
        assert!(uri.is_ok(), "uri parse failed: {:?}", uri.err());
        let with_query = format!("{}?a=1", pq);
        assert!(with_query.parse::<axum::http::Uri>().is_ok());
    }
}

#[cfg(test)]
mod layer_probe_tests {
    use axum::{routing::get, Router};

    // Documents the axum 0.7 behavior that forces the dual route
    // registration in `wc_v3_routes`: Router-level middleware never runs for
    // requests that match no route (the trailing-slash 404 case), regardless
    // of layer/nest ordering.
    async fn strip(
        mut req: axum::extract::Request,
        next: axum::middleware::Next,
    ) -> axum::response::Response {
        let path = req.uri().path().to_string();
        if path.len() > 1 && path.ends_with('/') {
            let trimmed = path.trim_end_matches('/').to_string();
            if let Ok(u) = trimmed.parse::<axum::http::Uri>() {
                *req.uri_mut() = u;
            }
        }
        next.run(req).await
    }

    async fn probe(app: Router, uri: &str) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app.into_make_service()).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("http://{addr}{uri}"))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    #[tokio::test]
    async fn router_layer_cannot_fix_trailing_slash() {
        let inner = Router::new().route("/orders/:id", get(|| async { "ok" }));
        // nest-then-layer: axum still skips the layer for unmatched requests
        let app = Router::new()
            .nest("/wc", inner.clone())
            .layer(axum::middleware::from_fn(strip));
        assert_eq!(probe(app, "/wc/orders/5/").await, 404);

        // layer-then-nest: same result
        let app = Router::new()
            .layer(axum::middleware::from_fn(strip))
            .nest("/wc", inner);
        assert_eq!(probe(app, "/wc/orders/5/").await, 404);
    }

    #[tokio::test]
    async fn dual_route_registration_handles_trailing_slash() {
        let handler = get(|| async { "ok" });
        let inner = Router::new()
            .route("/orders/:id", handler.clone())
            .route("/orders/:id/", handler);
        let build = || Router::new().nest("/wc", inner.clone());
        assert_eq!(probe(build(), "/wc/orders/5").await, 200);
        assert_eq!(probe(build(), "/wc/orders/5/").await, 200);
    }
}

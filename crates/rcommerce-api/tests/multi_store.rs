//! Several stores in one process. Each store has its own database, signing
//! secret and token audience, and requests reach a store by host name.
//!
//! Needs TEST_DATABASE_URL: the test creates two databases on that server and
//! drops them at the end. Skipped without it.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use rcommerce_core::Config;
use sqlx::postgres::PgPoolOptions;
use tower::Service;

/// One request through the application, as the server would route it
async fn call(
    app: &mut Router,
    host: &str,
    method: &str,
    path: &str,
    body: Option<serde_json::Value>,
    token: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, host)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = request
        .body(match body {
            Some(json) => Body::from(json.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    std::future::poll_fn(|cx| Service::<Request<Body>>::poll_ready(app, cx))
        .await
        .unwrap();
    let response = app.call(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

/// A fresh, migrated database on the test server
async fn fresh_database(admin: &sqlx::PgPool, server: &url::Url, name: &str) -> url::Url {
    sqlx::query(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
        .execute(admin)
        .await
        .unwrap();
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(admin)
        .await
        .unwrap();
    let mut url = server.clone();
    url.set_path(&format!("/{name}"));
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(url.as_str())
        .await
        .unwrap();
    rcommerce_core::Migrator::new(pool.clone())
        .migrate()
        .await
        .unwrap();
    pool.close().await;
    url
}

#[tokio::test]
async fn stores_are_isolated_by_host() {
    let Ok(admin_url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("Skipping multi-store test: set TEST_DATABASE_URL");
        return;
    };
    let server = url::Url::parse(&admin_url).expect("TEST_DATABASE_URL is a URL");
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&admin_url)
        .await
        .unwrap();
    let store_a_url = fresh_database(&admin, &server, "rc_multistore_a").await;
    fresh_database(&admin, &server, "rc_multistore_b").await;

    let config: Config = toml::from_str(&format!(
        r#"
[database]
db_type = "Postgres"
host = "{host}"
port = {port}
database = "unused"
username = "{user}"
password = "{password}"
pool_size = 3
ssl_mode = "Disable"

[security.jwt]
secret = "base-secret-0123456789abcdefghijklmnopqrstuvwxyz"

[rate_limiting]
enabled = false

[payment]
test_mode = true

[notifications]
enabled = false

[stores.a]
hostnames = ["shop-a.test"]
[stores.a.database]
database = "rc_multistore_a"
[stores.a.security.jwt]
secret = "store-a-secret-0123456789abcdefghijklmnopqrstu"

[stores.b]
hostnames = ["shop-b.test"]
[stores.b.database]
database = "rc_multistore_b"
[stores.b.security.jwt]
secret = "store-b-secret-0123456789abcdefghijklmnopqrstu"
"#,
        host = server.host_str().unwrap(),
        port = server.port().unwrap_or(5432),
        user = server.username(),
        password = server.password().unwrap_or_default(),
    ))
    .unwrap();
    let mut app = rcommerce_api::server::build_app(&config).await.unwrap();

    let email = "same.person@example.com";
    let account = serde_json::json!({
        "email": email, "password": "Correct-Horse-9",
        "first_name": "Jean", "last_name": "Martin"
    });
    let login = serde_json::json!({ "email": email, "password": "Correct-Horse-9" });

    // Registered on store A only, then verified there
    let (status, body) = call(
        &mut app,
        "shop-a.test",
        "POST",
        "/api/v1/auth/register",
        Some(account.clone()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let store_a = PgPoolOptions::new()
        .max_connections(1)
        .connect(store_a_url.as_str())
        .await
        .unwrap();
    sqlx::query("UPDATE customers SET email_verified = true WHERE email = $1")
        .bind(email)
        .execute(&store_a)
        .await
        .unwrap();
    store_a.close().await;

    let (status, body) = call(
        &mut app,
        "shop-a.test",
        "POST",
        "/api/v1/auth/login",
        Some(login.clone()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let token_a = body["access_token"].as_str().unwrap().to_string();

    // Store B has no such customer
    let (status, _) = call(
        &mut app,
        "shop-b.test",
        "POST",
        "/api/v1/auth/login",
        Some(login),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // A's token works on A only
    let (status, _) = call(
        &mut app,
        "shop-a.test",
        "GET",
        "/api/v1/customers/me",
        None,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(
        &mut app,
        "shop-b.test",
        "GET",
        "/api/v1/customers/me",
        None,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // The same e-mail can open a separate account on B
    let (status, body) = call(
        &mut app,
        "shop-b.test",
        "POST",
        "/api/v1/auth/register",
        Some(account),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // A host no store serves
    let (status, _) = call(
        &mut app,
        "unknown.test",
        "GET",
        "/api/v1/products",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    drop(app);
    for name in ["rc_multistore_a", "rc_multistore_b"] {
        sqlx::query(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
            .execute(&admin)
            .await
            .ok();
    }
}

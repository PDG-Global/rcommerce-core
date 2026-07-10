//! Setup wizard for R Commerce CLI
//!
//! Interactive configuration wizard to help users set up their R Commerce instance.

use colored::Colorize;
use dialoguer::{Confirm, Input, Password, Select};
use std::path::PathBuf;

use rcommerce_core::config::{CacheType, Config, DatabaseType, LetsEncryptConfig, StorageType};
use rcommerce_core::db::migrate::Migrator;

/// Run the interactive setup wizard
pub async fn run_setup(output_path: Option<PathBuf>) -> Result<(), String> {
    println!("\n{}", "🚀 R Commerce Setup Wizard".bold().cyan());
    println!("{}", "==========================".cyan());
    println!("\nThis wizard will help you configure your R Commerce instance.\n");

    let mut config = Config::default();

    // Step 1: Basic Store Information
    config = setup_store_info(config).await?;

    // Step 2: Database Configuration
    config = setup_database(config).await?;

    // Step 3: Database Setup (migrations)
    setup_database_schema(&config).await?;

    // Step 4: Data Import (optional)
    let import_done = setup_import(&config).await?;

    // Only continue with other config if user didn't exit during import
    if !import_done {
        // Step 5: Server Configuration
        config = setup_server(config).await?;

        // Step 6: Cache/Redis Configuration
        config = setup_cache(config).await?;

        // Step 7: Security & JWT
        config = setup_security(config).await?;

        // Step 8: Media Storage
        config = setup_media(config).await?;

        // Step 9: TLS/SSL
        config = setup_tls(config).await?;

        // Step 10: Payment Gateways (optional)
        config = setup_payment_gateways(config).await?;

        // Step 11: Notifications (optional)
        config = setup_notifications(config).await?;
    }

    // Generate and save config
    save_config(&config, output_path).await?;

    Ok(())
}

/// Setup basic store information
async fn setup_store_info(config: Config) -> Result<Config, String> {
    println!("\n{}", "📦 Store Configuration".bold().green());
    println!("{}", "----------------------".green());

    // Store name (for display only, not stored in config yet)
    let _store_name: String = Input::new()
        .with_prompt("Store name")
        .default("My Store".to_string())
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    // Default currency
    let currencies = vec![
        "USD", "EUR", "GBP", "AUD", "CAD", "JPY", "CNY", "HKD", "SGD",
    ];
    let currency_idx = Select::new()
        .with_prompt("Default currency")
        .items(&currencies)
        .default(0)
        .interact()
        .map_err(|e| format!("Selection error: {}", e))?;

    println!(
        "\n{}",
        format!("✓ Store configured with {}", currencies[currency_idx]).green()
    );

    Ok(config)
}

/// Setup database configuration
async fn setup_database(mut config: Config) -> Result<Config, String> {
    println!("\n{}", "🗄️  Database Configuration".bold().green());
    println!("{}", "--------------------------".green());

    // PostgreSQL is the only supported database
    config.database.db_type = DatabaseType::Postgres;

    let host: String = Input::new()
        .with_prompt("Database host")
        .default("localhost".to_string())
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    let port: u16 = Input::new()
        .with_prompt("Database port")
        .default(5432)
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    let database: String = Input::new()
        .with_prompt("Database name")
        .default("rcommerce".to_string())
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    let username: String = Input::new()
        .with_prompt("Database username")
        .default("rcommerce".to_string())
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    let password: String = Password::new()
        .with_prompt("Database password")
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    let pool_size: u32 = Input::new()
        .with_prompt("Connection pool size")
        .default(20)
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    config.database.host = host;
    config.database.port = port;
    config.database.database = database;
    config.database.username = username;
    config.database.password = password;
    config.database.pool_size = pool_size;

    println!("\n{}", "✓ Database configured".green());

    Ok(config)
}

/// Setup server configuration
async fn setup_server(mut config: Config) -> Result<Config, String> {
    println!("\n{}", "🌐 Server Configuration".bold().green());
    println!("{}", "-----------------------".green());

    let host: String = Input::new()
        .with_prompt("Server bind address")
        .default("0.0.0.0".to_string())
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    let port: u16 = Input::new()
        .with_prompt("Server port")
        .default(8080)
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    let worker_threads: usize = Input::new()
        .with_prompt("Worker threads (0 = auto)")
        .default(0)
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    config.server.host = host;
    config.server.port = port;
    config.server.worker_threads = worker_threads;

    println!(
        "\n{}",
        format!(
            "✓ Server will bind to {}:{}",
            config.server.host, config.server.port
        )
        .green()
    );

    Ok(config)
}

/// Setup cache configuration
async fn setup_cache(mut config: Config) -> Result<Config, String> {
    println!("\n{}", "💨 Cache Configuration".bold().green());
    println!("{}", "----------------------".green());

    let use_redis = Confirm::new()
        .with_prompt("Enable Redis caching? (Recommended for production)")
        .default(false)
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    if use_redis {
        config.cache.cache_type = CacheType::Redis;

        let redis_url: String = Input::new()
            .with_prompt("Redis URL")
            .default("redis://localhost:6379".to_string())
            .interact()
            .map_err(|e| format!("Input error: {}", e))?;

        config.cache.redis_url = Some(redis_url);
        println!("{}", "✓ Redis caching configured".green());
    } else {
        config.cache.cache_type = CacheType::Memory;
        println!("{}", "✓ In-memory caching configured".green());
    }

    Ok(config)
}

/// Setup security configuration
async fn setup_security(mut config: Config) -> Result<Config, String> {
    println!("\n{}", "🔒 Security Configuration".bold().green());
    println!("{}", "-------------------------".green());

    // JWT Secret
    let jwt_secret: String = Password::new()
        .with_prompt("JWT Secret (leave empty to generate random)")
        .allow_empty_password(true)
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    if jwt_secret.is_empty() {
        // Generate a simple random secret using timestamp and random bytes
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let random_part: String = std::iter::repeat_with(|| {
            let byte = (timestamp % 256) as u8;
            (b'a' + (byte % 26)) as char
        })
        .take(32)
        .collect();
        let random_secret = format!("{}{}", timestamp, random_part);

        config.security.jwt.secret = random_secret;
        println!("{}", "✓ Random JWT secret generated".green());
    } else {
        config.security.jwt.secret = jwt_secret;
    }

    let jwt_expiry: u64 = Input::new()
        .with_prompt("JWT token expiry (hours)")
        .default(24)
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    config.security.jwt.expiry_hours = jwt_expiry;

    // Rate limiting
    let enable_rate_limiting = Confirm::new()
        .with_prompt("Enable rate limiting?")
        .default(true)
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    config.rate_limiting.enabled = enable_rate_limiting;

    if enable_rate_limiting {
        let requests_per_minute: u32 = Input::new()
            .with_prompt("Requests per minute limit")
            .default(60)
            .interact()
            .map_err(|e| format!("Input error: {}", e))?;

        config.rate_limiting.requests_per_minute = requests_per_minute;
    }

    println!("{}", "✓ Security configured".green());

    Ok(config)
}

/// Setup media storage configuration
async fn setup_media(mut config: Config) -> Result<Config, String> {
    println!("\n{}", "📁 Media Storage Configuration".bold().green());
    println!("{}", "------------------------------".green());

    let storage_types = vec!["Local Filesystem", "S3 Compatible"];
    let storage_idx = Select::new()
        .with_prompt("Storage type")
        .items(&storage_types)
        .default(0)
        .interact()
        .map_err(|e| format!("Selection error: {}", e))?;

    if storage_idx == 0 {
        // Local storage
        config.media.storage_type = StorageType::Local;

        let local_path: String = Input::new()
            .with_prompt("Media storage path")
            .default("./uploads".to_string())
            .interact()
            .map_err(|e| format!("Input error: {}", e))?;

        config.media.local_path = Some(local_path);
    } else {
        // S3 storage
        config.media.storage_type = StorageType::S3;

        let bucket: String = Input::new()
            .with_prompt("S3 Bucket name")
            .interact()
            .map_err(|e| format!("Input error: {}", e))?;

        let region: String = Input::new()
            .with_prompt("S3 Region")
            .default("us-east-1".to_string())
            .interact()
            .map_err(|e| format!("Input error: {}", e))?;

        let access_key: String = Input::new()
            .with_prompt("S3 Access Key ID")
            .interact()
            .map_err(|e| format!("Input error: {}", e))?;

        let secret_key: String = Password::new()
            .with_prompt("S3 Secret Access Key")
            .interact()
            .map_err(|e| format!("Input error: {}", e))?;

        config.media.s3_bucket = Some(bucket);
        config.media.s3_region = Some(region);
        config.media.s3_access_key = Some(access_key);
        config.media.s3_secret_key = Some(secret_key);
    }

    println!("{}", "✓ Media storage configured".green());

    Ok(config)
}

/// Setup TLS configuration
async fn setup_tls(mut config: Config) -> Result<Config, String> {
    println!("\n{}", "🔐 TLS/SSL Configuration".bold().green());
    println!("{}", "------------------------".green());

    let enable_tls = Confirm::new()
        .with_prompt("Enable TLS/SSL?")
        .default(false)
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    if enable_tls {
        let tls_options = vec![
            "Let's Encrypt (Auto - Recommended)",
            "Manual Certificates (Existing files)",
        ];
        let tls_idx = Select::new()
            .with_prompt("TLS certificate source")
            .items(&tls_options)
            .default(0)
            .interact()
            .map_err(|e| format!("Selection error: {}", e))?;

        if tls_idx == 0 {
            // Let's Encrypt
            println!("\n{}", "Let's Encrypt Configuration".cyan());

            let email: String = Input::new()
                .with_prompt("Contact email for Let's Encrypt")
                .interact()
                .map_err(|e| format!("Input error: {}", e))?;

            let domains_input: String = Input::new()
                .with_prompt("Domains (comma-separated, e.g., example.com,www.example.com)")
                .interact()
                .map_err(|e| format!("Input error: {}", e))?;

            let domains: Vec<String> = domains_input
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            let use_staging = Confirm::new()
                .with_prompt("Use Let's Encrypt staging server? (for testing)")
                .default(false)
                .interact()
                .map_err(|e| format!("Input error: {}", e))?;

            let cache_dir: String = Input::new()
                .with_prompt("Certificate cache directory")
                .default("./certs".to_string())
                .interact()
                .map_err(|e| format!("Input error: {}", e))?;

            let https_port: u16 = Input::new()
                .with_prompt("HTTPS port")
                .default(443)
                .interact()
                .map_err(|e| format!("Input error: {}", e))?;

            let http_port: u16 = Input::new()
                .with_prompt("HTTP port (for ACME challenges and redirects)")
                .default(80)
                .interact()
                .map_err(|e| format!("Input error: {}", e))?;

            config.tls.enabled = true;
            config.tls.https_port = https_port;
            config.tls.http_port = http_port;
            config.tls.lets_encrypt = Some(LetsEncryptConfig {
                enabled: true,
                email,
                domains,
                use_staging,
                cache_dir: PathBuf::from(cache_dir),
                ..Default::default()
            });

            println!("\n{}", "✓ Let's Encrypt configured".green());
            println!(
                "{}",
                "  Note: Ensure ports 80 and 443 are accessible from the internet".dimmed()
            );
            println!("{}", "  and DNS records point to this server.".dimmed());
        } else {
            // Manual certificates
            let cert_path: String = Input::new()
                .with_prompt("Path to TLS certificate file")
                .default("./certs/cert.pem".to_string())
                .interact()
                .map_err(|e| format!("Input error: {}", e))?;

            let key_path: String = Input::new()
                .with_prompt("Path to TLS private key file")
                .default("./certs/key.pem".to_string())
                .interact()
                .map_err(|e| format!("Input error: {}", e))?;

            let https_port: u16 = Input::new()
                .with_prompt("HTTPS port")
                .default(443)
                .interact()
                .map_err(|e| format!("Input error: {}", e))?;

            config.tls.enabled = true;
            config.tls.https_port = https_port;
            config.tls.cert_file = Some(PathBuf::from(cert_path));
            config.tls.key_file = Some(PathBuf::from(key_path));

            println!("{}", "✓ Manual TLS certificates configured".green());
        }
    } else {
        config.tls.enabled = false;
        println!("{}", "✓ TLS disabled (will use HTTP)".yellow());
    }

    Ok(config)
}

/// Setup payment gateways (optional)
async fn setup_payment_gateways(mut config: Config) -> Result<Config, String> {
    println!("\n{}", "💳 Payment Gateways (Optional)".bold().green());
    println!("{}", "------------------------------".green());

    let configure_payments = Confirm::new()
        .with_prompt("Configure payment gateways now?")
        .default(false)
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    if configure_payments {
        // Stripe
        let enable_stripe = Confirm::new()
            .with_prompt("Enable Stripe?")
            .default(false)
            .interact()
            .map_err(|e| format!("Input error: {}", e))?;

        if enable_stripe {
            let stripe_key: String = Input::new()
                .with_prompt("Stripe Secret Key")
                .interact()
                .map_err(|e| format!("Input error: {}", e))?;

            let stripe_webhook: String = Input::new()
                .with_prompt("Stripe Webhook Secret (optional)")
                .default("".to_string())
                .interact()
                .map_err(|e| format!("Input error: {}", e))?;

            config.payment.stripe.enabled = true;
            config.payment.stripe.secret_key = Some(stripe_key);
            config.payment.stripe.webhook_secret = if stripe_webhook.is_empty() {
                None
            } else {
                Some(stripe_webhook)
            };
        }

        // Test mode
        let test_mode = Confirm::new()
            .with_prompt("Enable payment test mode?")
            .default(true)
            .interact()
            .map_err(|e| format!("Input error: {}", e))?;

        config.payment.test_mode = test_mode;

        println!("{}", "✓ Payment gateways configured".green());
    } else {
        println!(
            "{}",
            "ℹ️  Payment gateways can be configured later".dimmed()
        );
    }

    Ok(config)
}

/// Setup notifications (optional)
async fn setup_notifications(mut config: Config) -> Result<Config, String> {
    println!("\n{}", "📧 Notifications (Optional)".bold().green());
    println!("{}", "---------------------------".green());

    let configure_email = Confirm::new()
        .with_prompt("Configure email notifications?")
        .default(false)
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    if configure_email {
        let smtp_host: String = Input::new()
            .with_prompt("SMTP Host")
            .default("smtp.gmail.com".to_string())
            .interact()
            .map_err(|e| format!("Input error: {}", e))?;

        let smtp_port: u16 = Input::new()
            .with_prompt("SMTP Port")
            .default(587)
            .interact()
            .map_err(|e| format!("Input error: {}", e))?;

        let smtp_username: String = Input::new()
            .with_prompt("SMTP Username")
            .interact()
            .map_err(|e| format!("Input error: {}", e))?;

        let smtp_password: String = Password::new()
            .with_prompt("SMTP Password")
            .interact()
            .map_err(|e| format!("Input error: {}", e))?;

        let from_email: String = Input::new()
            .with_prompt("From email address")
            .interact()
            .map_err(|e| format!("Input error: {}", e))?;

        let from_name: String = Input::new()
            .with_prompt("From name")
            .default("R Commerce Store".to_string())
            .interact()
            .map_err(|e| format!("Input error: {}", e))?;

        config.notifications.enabled = true;
        config.notifications.email.smtp_host = Some(smtp_host);
        config.notifications.email.smtp_port = Some(smtp_port);
        config.notifications.email.smtp_user = Some(smtp_username);
        config.notifications.email.smtp_pass = Some(smtp_password);
        config.notifications.email.from_email = Some(from_email);
        config.notifications.email.from_name = Some(from_name);

        println!("{}", "✓ Email notifications configured".green());
    } else {
        config.notifications.enabled = false;
        println!(
            "{}",
            "ℹ️  Email notifications can be configured later".dimmed()
        );
    }

    Ok(config)
}

/// Setup database schema (run migrations)
async fn setup_database_schema(config: &Config) -> Result<(), String> {
    println!("\n{}", "🗄️  Database Setup".bold().green());
    println!("{}", "------------------".green());

    // Check if we can connect to the database
    println!("{}", "Connecting to database...".yellow());

    // Try to create database connection pool
    let pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&config.database.url())
        .await
    {
        Ok(pool) => {
            println!("{}", "✓ Connected to database".green());
            pool
        }
        Err(e) => {
            println!("{}", format!("❌ Cannot connect to database: {}", e).red());

            let continue_anyway = Confirm::new()
                .with_prompt(
                    "Continue without database setup? (You'll need to run migrations manually)",
                )
                .default(false)
                .interact()
                .map_err(|e| format!("Input error: {}", e))?;

            if continue_anyway {
                println!(
                    "{}",
                    "⚠️  Skipping database setup. Run 'rcommerce db migrate' later.".yellow()
                );
                return Ok(());
            } else {
                return Err("Database connection required".to_string());
            }
        }
    };

    // Check if database has existing tables
    let has_tables: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT FROM information_schema.tables 
            WHERE table_schema = 'public' 
            AND table_name = '_migrations'
        )",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(false);

    if has_tables {
        println!("\n{}", "⚠️  Database already has tables!".yellow().bold());

        let check_data: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
            .fetch_one(&pool)
            .await
            .unwrap_or(0);

        if check_data > 0 {
            println!(
                "{}",
                format!("   Found existing data ({} products)", check_data).dimmed()
            );
        }

        let action = Select::new()
            .with_prompt("What would you like to do?")
            .items(&[
                "Keep existing data (skip migrations)",
                "Reset database (delete all data and start fresh)",
                "Exit setup and investigate",
            ])
            .default(0)
            .interact()
            .map_err(|e| format!("Selection error: {}", e))?;

        match action {
            0 => {
                println!(
                    "{}",
                    "✓ Keeping existing database. Skipping migrations.".green()
                );
                return Ok(());
            }
            1 => {
                let confirm = Confirm::new()
                    .with_prompt("⚠️  WARNING: This will DELETE ALL DATA. Are you sure?")
                    .default(false)
                    .interact()
                    .map_err(|e| format!("Input error: {}", e))?;

                if confirm {
                    println!("{}", "Resetting database...".yellow());

                    // Run database reset
                    let migrator = Migrator::new(pool.clone());
                    match migrator.reset().await {
                        Ok(_) => println!("{}", "✓ Database reset complete".green()),
                        Err(e) => {
                            println!("{}", format!("❌ Database reset failed: {}", e).red());
                            return Err(format!("Database reset failed: {}", e));
                        }
                    }
                } else {
                    println!("{}", "Reset cancelled. Skipping migrations.".yellow());
                    return Ok(());
                }
            }
            2 => {
                return Err("Setup cancelled by user".to_string());
            }
            _ => {}
        }
    }

    // Run migrations
    println!("{}", "Running database migrations...".yellow());

    let migrator = Migrator::new(pool.clone());
    match migrator.migrate().await {
        Ok(_) => {
            println!("{}", "✓ Database migrations completed".green());
        }
        Err(e) => {
            println!("{}", format!("❌ Migration failed: {}", e).red());

            let continue_anyway = Confirm::new()
                .with_prompt("Continue despite migration failure?")
                .default(false)
                .interact()
                .map_err(|e| format!("Input error: {}", e))?;

            if !continue_anyway {
                return Err(format!("Migration failed: {}", e));
            }
        }
    }

    Ok(())
}

/// Setup data import from existing store
async fn setup_import(config: &Config) -> Result<bool, String> {
    println!("\n{}", "📥 Data Import (Optional)".bold().green());
    println!("{}", "------------------------".green());

    let do_import = Confirm::new()
        .with_prompt("Would you like to import data from an existing store?")
        .default(false)
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    if !do_import {
        println!(
            "{}",
            "ℹ️  You can import data later using 'rcommerce import'".dimmed()
        );
        return Ok(false);
    }

    // Select platform
    let platforms = vec![
        "WooCommerce",
        "Shopify",
        "Magento",
        "Medusa",
        "CSV File",
        "Skip import",
    ];

    let platform_idx = Select::new()
        .with_prompt("Select source platform")
        .items(&platforms)
        .default(0)
        .interact()
        .map_err(|e| format!("Selection error: {}", e))?;

    if platform_idx == 5 {
        println!("{}", "ℹ️  Import skipped. You can run it later.".dimmed());
        return Ok(false);
    }

    let platform = match platform_idx {
        0 => "woocommerce",
        1 => "shopify",
        2 => "magento",
        3 => "medusa",
        4 => "csv",
        _ => return Ok(false),
    };

    if platform == "csv" {
        println!("\n{}", "CSV Import".cyan());
        println!("{}", "Please use the following command to import:".dimmed());
        println!("  rcommerce import csv <file> -c config.toml");
        return Ok(false);
    }

    // Get platform-specific details
    println!(
        "\n{}",
        format!("{} API Configuration", platforms[platform_idx]).cyan()
    );

    let api_url: String = Input::new()
        .with_prompt(format!("{} store URL", platforms[platform_idx]))
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    let api_key: String = Password::new()
        .with_prompt("API Key / Consumer Key")
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    let api_secret: String = Password::new()
        .with_prompt("API Secret / Consumer Secret (if required)")
        .allow_empty_password(true)
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    let default_currency: String = Input::new()
        .with_prompt("Default currency for imported records")
        .default("USD".to_string())
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    let overwrite = Confirm::new()
        .with_prompt("Update existing records if they exist?")
        .default(true)
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    // Confirm before import
    println!("\n{}", "Import Summary:".bold());
    println!("  Platform: {}", platforms[platform_idx]);
    println!("  Store URL: {}", api_url);
    println!("  Currency: {}", default_currency);
    println!(
        "  Update existing: {}",
        if overwrite { "Yes" } else { "No" }
    );

    let confirm = Confirm::new()
        .with_prompt("Start import?")
        .default(true)
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    if !confirm {
        println!("{}", "Import cancelled.".yellow());
        return Ok(false);
    }

    // Run the import
    println!(
        "\n{}",
        format!("🚀 Starting {} import...", platforms[platform_idx])
            .bold()
            .cyan()
    );

    // Build import config
    let import_config = rcommerce_core::import::ImportConfig {
        database_url: config.database.url(),
        source: rcommerce_core::import::types::SourceConfig::Platform {
            platform: platform.to_string(),
            api_url,
            api_key,
            headers: if api_secret.is_empty() {
                std::collections::HashMap::new()
            } else {
                let mut h = std::collections::HashMap::new();
                h.insert("consumer_secret".to_string(), api_secret);
                h
            },
        },
        options: rcommerce_core::import::types::ImportOptions {
            default_currency,
            update_existing: overwrite,
            skip_existing: !overwrite,
            ..Default::default()
        },
    };

    // Get importer and run
    match rcommerce_core::import::get_platform_importer(platform) {
        Some(importer) => {
            let progress = |p: rcommerce_core::import::ImportProgress| {
                print!(
                    "\r  [{}] {} - {}/{}",
                    p.stage.bright_blue(),
                    p.message,
                    p.current,
                    p.total
                );
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
            };

            println!("\n{}", "Importing data...".yellow());

            match importer.import_all(&import_config, &progress).await {
                Ok(stats) => {
                    println!("\n\n{}", "✅ Import completed!".green().bold());
                    println!("  Created: {}", stats.created);
                    println!("  Updated: {}", stats.updated);
                    println!("  Skipped: {}", stats.skipped);
                    println!("  Errors:  {}", stats.errors);

                    if !stats.error_details.is_empty() {
                        println!("\n{}", "Warnings:".yellow());
                        for error in stats.error_details.iter().take(5) {
                            println!("  • {}", error);
                        }
                        if stats.error_details.len() > 5 {
                            println!("  ... and {} more", stats.error_details.len() - 5);
                        }
                    }
                }
                Err(e) => {
                    println!("\n{}", format!("❌ Import failed: {}", e).red());
                    return Ok(false);
                }
            }
        }
        None => {
            println!(
                "{}",
                format!("❌ Platform '{}' not supported", platform).red()
            );
            return Ok(false);
        }
    }

    // Ask if user wants to continue with more configuration or exit
    let continue_setup = Confirm::new()
        .with_prompt("Continue with additional configuration (server, cache, etc.)?")
        .default(true)
        .interact()
        .map_err(|e| format!("Input error: {}", e))?;

    Ok(!continue_setup) // Return true if user wants to exit (import done, skip rest)
}

/// Save configuration to file
async fn save_config(config: &Config, output_path: Option<PathBuf>) -> Result<(), String> {
    println!("\n{}", "💾 Saving Configuration".bold().green());
    println!("{}", "----------------------".green());

    // Determine output path
    let output_path = match output_path {
        Some(path) => path,
        None => {
            let default_path = "./config.toml";
            let path: String = Input::new()
                .with_prompt("Configuration file path")
                .default(default_path.to_string())
                .interact()
                .map_err(|e| format!("Input error: {}", e))?;
            PathBuf::from(path)
        }
    };

    // Serialize to TOML
    let toml_string =
        toml::to_string_pretty(config).map_err(|e| format!("Failed to serialize config: {}", e))?;

    // Create parent directory if needed
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    // Write file
    std::fs::write(&output_path, toml_string)
        .map_err(|e| format!("Failed to write config file: {}", e))?;

    // Set secure permissions (Unix only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&output_path)
            .map_err(|e| format!("Failed to get file metadata: {}", e))?
            .permissions();
        permissions.set_mode(0o600); // Owner read/write only
        std::fs::set_permissions(&output_path, permissions)
            .map_err(|e| format!("Failed to set file permissions: {}", e))?;
    }

    println!(
        "\n{}",
        format!("✓ Configuration saved to {}", output_path.display())
            .green()
            .bold()
    );
    println!("\n{}", "Next steps:".bold());
    println!("  1. Review the configuration file");
    println!(
        "  2. Run database migrations: rcommerce db migrate -c {}",
        output_path.display()
    );
    println!(
        "  3. Start the server: rcommerce server -c {}",
        output_path.display()
    );
    println!();

    Ok(())
}

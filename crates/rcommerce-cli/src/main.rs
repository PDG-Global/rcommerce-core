use clap::{Parser, Subcommand};
use colored::Colorize;
use dialoguer::{Confirm, Input, Password, Select};
use std::path::PathBuf;
use tracing::info;

use rcommerce_core::models::{Currency, ProductType};
use rcommerce_core::{Config, Result};

mod commands {
    pub mod setup;
    pub mod shell;
}

/// Security checks for CLI operations
mod security {
    use colored::Colorize;
    use std::path::PathBuf;

    /// Check if running as root
    pub fn check_not_root() -> Result<(), String> {
        #[cfg(unix)]
        {
            let uid = unsafe { libc::getuid() };
            if uid == 0 {
                return Err(format!(
                    "\n{}\n{}\n{}",
                    "❌ ERROR: Running as root is not allowed!".red().bold(),
                    "   The rcommerce CLI should not be run as root for security reasons.",
                    "   Please run as a non-privileged user."
                ));
            }
        }
        Ok(())
    }

    /// Check config file permissions
    pub fn check_config_permissions(path: &PathBuf) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;

        let metadata =
            std::fs::metadata(path).map_err(|e| format!("Cannot read config file: {}", e))?;

        let permissions = metadata.permissions();
        let mode = permissions.mode();

        // Check if world-readable (last digit in octal)
        let world_readable = (mode & 0o004) != 0;
        let world_writable = (mode & 0o002) != 0;

        if world_writable {
            return Err(format!(
                "\n{}\n   Path: {}\n   Run: chmod 600 {}",
                "❌ ERROR: Config file is world-writable!".red().bold(),
                path.display(),
                path.display()
            ));
        }

        if world_readable {
            eprintln!(
                "\n{}\n   Path: {}\n   Consider running: chmod 600 {}",
                "⚠️  WARNING: Config file is world-readable".yellow().bold(),
                path.display(),
                path.display()
            );
        }

        Ok(())
    }
}

#[derive(Parser)]
#[command(name = "rcommerce")]
#[command(about = "R Commerce Headless E-Commerce Platform")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, global = true, help = "Configuration file path")]
    config: Option<PathBuf>,

    #[arg(short, long, global = true, help = "Set log level")]
    log_level: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the API server
    Server {
        #[arg(short = 'H', long, help = "Bind address", default_value = "0.0.0.0")]
        host: String,

        #[arg(short = 'P', long, help = "Port number", default_value = "8080")]
        port: u16,

        #[arg(long, help = "Skip automatic database migration on startup")]
        skip_migrate: bool,
    },

    /// Database operations
    Db {
        #[command(subcommand)]
        command: DbCommands,
    },

    /// Product management
    Product {
        #[command(subcommand)]
        command: ProductCommands,
    },

    /// Order management
    Order {
        #[command(subcommand)]
        command: OrderCommands,
    },

    /// Customer management
    Customer {
        #[command(subcommand)]
        command: CustomerCommands,
    },

    /// API Key management
    ApiKey {
        #[command(subcommand)]
        command: ApiKeyCommands,
    },

    /// Import data from platforms or files
    Import {
        #[command(subcommand)]
        command: ImportCommands,
    },

    /// TLS certificate management
    Tls {
        #[command(subcommand)]
        command: TlsCommands,
    },

    /// Show configuration
    Config,

    /// Email testing and management
    Email {
        #[command(subcommand)]
        command: EmailCommands,
    },

    /// Interactive setup wizard
    Setup {
        /// Output file path for the configuration
        #[arg(short, long, help = "Output configuration file path")]
        output: Option<PathBuf>,
    },

    /// Interactive shell for managing your R Commerce installation
    Shell,
}

#[derive(Subcommand, Debug)]
pub enum DbCommands {
    /// Run database migrations
    Migrate,

    /// Reset database (DANGEROUS - deletes all data)
    Reset {
        #[arg(long, help = "Skip confirmation prompt")]
        force: bool,
    },

    /// Seed database with sample data
    Seed,

    /// Show database status
    Status,
}

#[derive(Subcommand, Debug)]
pub enum ProductCommands {
    /// List products
    List,

    /// Create a product
    Create,

    /// Get product details
    Get {
        #[arg(help = "Product ID")]
        id: String,
    },

    /// Update a product
    Update {
        #[arg(help = "Product ID")]
        id: String,
    },

    /// Delete a product
    Delete {
        #[arg(help = "Product ID")]
        id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum OrderCommands {
    /// List orders
    List,

    /// Get order details
    Get {
        #[arg(help = "Order ID")]
        id: String,
    },

    /// Create a test order
    Create,

    /// Update order status
    Update {
        #[arg(help = "Order ID")]
        id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum CustomerCommands {
    /// List customers
    List,

    /// Get customer details
    Get {
        #[arg(help = "Customer ID")]
        id: String,
    },

    /// Create a customer
    Create,
}

#[derive(Subcommand, Debug)]
pub enum ImportCommands {
    /// Import from an e-commerce platform (shopify, woocommerce, magento, medusa)
    Platform {
        /// Platform name
        #[arg(help = "Platform name (shopify, woocommerce, magento, medusa)")]
        platform: String,

        /// API base URL (or set in config file)
        #[arg(
            short = 'u',
            long,
            help = "API base URL (or set in config file)",
            default_value = ""
        )]
        api_url: String,

        /// API key or access token (or set in config file)
        #[arg(
            short = 'k',
            long,
            help = "API key or access token (or set in config file)",
            default_value = ""
        )]
        api_key: String,

        /// Additional API secret (for WooCommerce)
        #[arg(long, help = "API secret (for WooCommerce)")]
        api_secret: Option<String>,

        /// Entity types to import (products, customers, orders, all)
        #[arg(short, long, help = "Entity types to import", default_value = "all")]
        entities: String,

        /// Maximum number of records to import
        #[arg(long, help = "Maximum records to import")]
        limit: Option<usize>,

        /// Dry run - validate without importing
        #[arg(long, help = "Dry run without importing")]
        dry_run: bool,

        /// Overwrite existing products instead of skipping
        #[arg(long, help = "Update existing products instead of skipping")]
        overwrite: bool,

        /// Default currency for imported records (ISO 4217 code)
        #[arg(
            short = 'C',
            long,
            help = "Default currency code (USD, AUD, EUR, etc.)",
            default_value = "USD"
        )]
        currency: String,
    },

    /// Import from a file (csv, json, xml)
    File {
        /// Path to the import file
        #[arg(help = "Path to import file")]
        path: PathBuf,

        /// File format (csv, json, xml)
        #[arg(short, long, help = "File format (csv, json, xml)")]
        format: String,

        /// Entity type (products, customers, orders)
        #[arg(short, long, help = "Entity type (products, customers, orders)")]
        entity: String,

        /// Dry run - validate without importing
        #[arg(long, help = "Dry run without importing")]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum TlsCommands {
    /// Check certificate status and expiry for a domain
    Check {
        #[arg(short, long, help = "Domain to check")]
        domain: String,
    },

    /// List all certificates in the cache
    List,

    /// Manually renew a certificate
    Renew {
        #[arg(short, long, help = "Domain to renew")]
        domain: String,

        #[arg(long, help = "Force renewal even if not expired")]
        force: bool,
    },

    /// Show detailed certificate information
    Info {
        #[arg(short, long, help = "Domain to show info for")]
        domain: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum EmailCommands {
    /// Test all email templates with mock data (saves to filesystem)
    TestAll {
        #[arg(
            short,
            long,
            help = "Output directory for test emails",
            default_value = "./test-emails"
        )]
        output_dir: String,

        #[arg(short, long, help = "Recipient email address for test")]
        recipient: Option<String>,
    },

    /// Test a specific email template
    Test {
        #[arg(
            help = "Template type (order_confirmation, order_shipped, payment_failed, payment_successful, subscription_created, subscription_renewal, subscription_cancelled, dunning_first, dunning_retry, dunning_final, welcome, password_reset, abandoned_cart)"
        )]
        template: String,

        #[arg(
            short,
            long,
            help = "Output directory for test email",
            default_value = "./test-emails"
        )]
        output_dir: String,

        #[arg(short, long, help = "Recipient email address")]
        recipient: Option<String>,
    },

    /// Send a test email via SMTP (requires SMTP configuration)
    Send {
        #[arg(help = "Template type to send")]
        template: String,

        #[arg(short, long, help = "Recipient email address (required)")]
        to: String,

        #[arg(short, long, help = "Use mock mode instead of SMTP")]
        mock: bool,
    },

    /// List all available email templates
    List,
}

#[derive(Subcommand, Debug)]
pub enum ApiKeyCommands {
    /// List all API keys
    List {
        #[arg(short = 'u', long, help = "Filter by customer ID")]
        customer_id: Option<String>,
    },

    /// Create a new API key
    Create {
        #[arg(short = 'u', long, help = "Customer ID (optional for system keys)")]
        customer_id: Option<String>,

        #[arg(short = 'n', long, help = "Key name/description")]
        name: Option<String>,

        #[arg(
            short = 's',
            long,
            help = "Scopes (comma-separated)",
            default_value = "read"
        )]
        scopes: String,

        #[arg(short = 'e', long, help = "Expiration in days (optional)")]
        expires_days: Option<i64>,
    },

    /// Get API key details
    Get {
        #[arg(help = "Key prefix or ID")]
        prefix: String,
    },

    /// Revoke an API key
    Revoke {
        #[arg(help = "Key prefix or ID")]
        prefix: String,

        #[arg(short, long, help = "Reason for revocation")]
        reason: Option<String>,
    },

    /// Delete an API key permanently
    Delete {
        #[arg(help = "Key prefix or ID")]
        prefix: String,

        #[arg(long, help = "Skip confirmation")]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = cli.log_level.as_deref().unwrap_or("info");
    tracing_subscriber::fmt().with_env_filter(log_level).init();

    // Load configuration
    let config = if let Some(ref config_path) = cli.config {
        // Check config file permissions
        if let Err(e) = security::check_config_permissions(config_path) {
            eprintln!("{}", e);
            std::process::exit(1);
        }
        Config::load(config_path.to_str().unwrap())?
    } else {
        Config::from_env()?
    };

    info!(
        "Starting R Commerce v{} with config: {}",
        rcommerce_core::VERSION,
        config.server.host
    );

    // Execute command
    match cli.command {
        Commands::Server {
            host,
            port,
            skip_migrate,
        } => {
            // Override config with CLI arguments
            let mut config = config;
            config.server.host = host;
            config.server.port = port;

            // Auto-migrate database unless skipped
            if !skip_migrate {
                info!("Running database migrations...");
                match run_migrations(&config).await {
                    Ok(_) => info!("Database migrations completed successfully"),
                    Err(e) => {
                        eprintln!("❌ Database migration failed: {}", e);
                        eprintln!("Use --skip-migrate to start without migration");
                        std::process::exit(1);
                    }
                }
            }

            rcommerce_api::run(config).await?;
        }

        Commands::Db { command } => {
            use colored::*;

            // Create database pool
            let pool = create_pool(&config).await?;
            let migrator = rcommerce_core::Migrator::new(pool);

            match command {
                DbCommands::Migrate => {
                    println!("{}", "Running database migrations...".yellow());
                    match migrator.migrate().await {
                        Ok(_) => {
                            println!("{}", "✅ Migrations completed successfully!".green());
                        }
                        Err(e) => {
                            eprintln!("{}", format!("❌ Migration failed: {}", e).red());
                            std::process::exit(1);
                        }
                    }
                }
                DbCommands::Reset { force } => {
                    if !force {
                        println!("{}", "⚠️  WARNING: This will DELETE ALL DATA!".red().bold());
                        print!("Type 'yes' to confirm: ");
                        use std::io::Write;
                        std::io::stdout().flush().unwrap();

                        let mut input = String::new();
                        std::io::stdin().read_line(&mut input).unwrap();

                        if input.trim() != "yes" {
                            println!("Aborted.");
                            return Ok(());
                        }
                    }

                    println!("{}", "Resetting database...".red());
                    match migrator.reset().await {
                        Ok(_) => {
                            println!("{}", "✅ Database reset complete!".green());
                        }
                        Err(e) => {
                            eprintln!("{}", format!("❌ Reset failed: {}", e).red());
                            std::process::exit(1);
                        }
                    }
                }
                DbCommands::Seed => {
                    println!("{}", "Seeding database with demo data...".green());
                    match migrator.seed().await {
                        Ok(_) => {
                            println!("{}", "✅ Demo data seeded successfully!".green());
                        }
                        Err(e) => {
                            eprintln!("{}", format!("❌ Seed failed: {}", e).red());
                            std::process::exit(1);
                        }
                    }
                }
                DbCommands::Status => match migrator.status().await {
                    Ok(status) => {
                        println!("{}", "Database Status".bold().underline());
                        println!("  Host: {}:{}", config.database.host, config.database.port);
                        println!("  Database: {}", config.database.database);
                        println!("  Applied migrations: {}", status.applied_migrations);
                        println!("  Products: {}", status.product_count);
                        println!("  Customers: {}", status.customer_count);
                        println!("  Orders: {}", status.order_count);
                    }
                    Err(e) => {
                        eprintln!("{}", format!("❌ Failed to get status: {}", e).red());
                        std::process::exit(1);
                    }
                },
            }
        }

        Commands::Product { command } => {
            // Security checks
            if let Err(e) = security::check_not_root() {
                eprintln!("{}", e);
                std::process::exit(1);
            }

            let pool = create_pool(&config).await?;

            match command {
                ProductCommands::List => match list_products(&pool).await {
                    Ok(products) => {
                        if products.is_empty() {
                            println!("{}", "No products found".yellow());
                        } else {
                            println!("{}", "Products".bold().underline());
                            println!(
                                "{:<36} {:<30} {:<10} {:<8} {:<12}",
                                "ID", "Title", "Price", "Currency", "Status"
                            );
                            println!("{}", "-".repeat(100));
                            for p in &products {
                                let status = if p.is_active {
                                    "✓ Active".green()
                                } else {
                                    "✗ Inactive".red()
                                };
                                println!(
                                    "{:<36} {:<30} {:<10.2} {:<8} {:<12}",
                                    p.id.to_string(),
                                    truncate(&p.title, 28),
                                    p.price,
                                    p.currency.to_string(),
                                    status
                                );
                            }
                            println!("\nTotal: {} products", products.len());
                        }
                    }
                    Err(e) => {
                        eprintln!("{}", format!("❌ Failed to list products: {}", e).red());
                        std::process::exit(1);
                    }
                },
                ProductCommands::Get { id } => match get_product(&pool, &id).await {
                    Ok(Some(p)) => {
                        println!("{}", "Product Details".bold().underline());
                        println!("  ID:          {}", p.id);
                        println!("  Title:       {}", p.title);
                        println!("  Slug:        {}", p.slug);
                        println!("  Price:       {} {}", p.price, p.currency);
                        println!(
                            "  Status:      {}",
                            if p.is_active {
                                "✓ Active".green()
                            } else {
                                "✗ Inactive".red()
                            }
                        );
                        println!("  Inventory:   {}", p.inventory_quantity);
                        println!("  Created:     {}", p.created_at);
                        if let Some(desc) = p.description {
                            println!("  Description: {}", truncate(&desc, 100));
                        }
                    }
                    Ok(None) => {
                        println!("{}", format!("Product '{}' not found", id).yellow());
                    }
                    Err(e) => {
                        eprintln!("{}", format!("❌ Failed to get product: {}", e).red());
                        std::process::exit(1);
                    }
                },
                ProductCommands::Create => match interactive_create_product(&pool).await {
                    Ok(product) => {
                        println!("{}", "\n✅ Product created successfully!".green().bold());
                        println!("  ID:    {}", product.id);
                        println!("  Title: {}", product.title);
                        println!("  Slug:  {}", product.slug);
                        println!("  Price: {} {}", product.price, product.currency);
                    }
                    Err(e) => {
                        eprintln!("{}", format!("❌ Failed to create product: {}", e).red());
                        std::process::exit(1);
                    }
                },
                ProductCommands::Update { id } => {
                    println!(
                        "{}",
                        format!("Product update for '{}' coming soon!", id).yellow()
                    );
                }
                ProductCommands::Delete { id } => {
                    println!("{}", "⚠️  Product deletion".red().bold());
                    print!("Type 'yes' to delete product '{}': ", id);
                    use std::io::Write;
                    std::io::stdout().flush().unwrap();

                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input).unwrap();

                    if input.trim() != "yes" {
                        println!("Aborted.");
                        return Ok(());
                    }

                    match delete_product(&pool, &id).await {
                        Ok(true) => println!("{}", format!("✅ Product '{}' deleted", id).green()),
                        Ok(false) => println!("{}", format!("Product '{}' not found", id).yellow()),
                        Err(e) => {
                            eprintln!("{}", format!("❌ Failed to delete product: {}", e).red());
                            std::process::exit(1);
                        }
                    }
                }
            }
        }

        Commands::Order { command } => {
            if let Err(e) = security::check_not_root() {
                eprintln!("{}", e);
                std::process::exit(1);
            }

            let pool = create_pool(&config).await?;

            match command {
                OrderCommands::List => match list_orders(&pool).await {
                    Ok(orders) => {
                        if orders.is_empty() {
                            println!("{}", "No orders found".yellow());
                        } else {
                            println!("{}", "Orders".bold().underline());
                            println!(
                                "{:<36} {:<20} {:<12} {:<15} {:<12}",
                                "ID", "Customer", "Status", "Total", "Created"
                            );
                            println!("{}", "-".repeat(100));
                            for o in &orders {
                                println!(
                                    "{:<36} {:<20} {:<12} {:<15.2} {:<12}",
                                    o.id.to_string(),
                                    truncate(&o.customer_email, 18),
                                    o.status,
                                    o.total,
                                    o.created_at.format("%Y-%m-%d")
                                );
                            }
                            println!("\nTotal: {} orders", orders.len());
                        }
                    }
                    Err(e) => {
                        eprintln!("{}", format!("❌ Failed to list orders: {}", e).red());
                        std::process::exit(1);
                    }
                },
                OrderCommands::Get { id } => {
                    println!(
                        "{}",
                        format!("Order details for '{}' coming soon!", id).yellow()
                    );
                }
                OrderCommands::Create => {
                    println!("{}", "Order creation via CLI coming soon!".yellow());
                }
                OrderCommands::Update { id } => {
                    println!(
                        "{}",
                        format!("Order update for '{}' coming soon!", id).yellow()
                    );
                }
            }
        }

        Commands::Customer { command } => {
            if let Err(e) = security::check_not_root() {
                eprintln!("{}", e);
                std::process::exit(1);
            }

            let pool = create_pool(&config).await?;

            match command {
                CustomerCommands::List => match list_customers(&pool).await {
                    Ok(customers) => {
                        if customers.is_empty() {
                            println!("{}", "No customers found".yellow());
                        } else {
                            println!("{}", "Customers".bold().underline());
                            println!(
                                "{:<36} {:<30} {:<20} {:<12}",
                                "ID", "Email", "Name", "Created"
                            );
                            println!("{}", "-".repeat(100));
                            for c in &customers {
                                let name = format!("{} {}", c.first_name, c.last_name);
                                println!(
                                    "{:<36} {:<30} {:<20} {:<12}",
                                    c.id.to_string(),
                                    truncate(&c.email, 28),
                                    truncate(&name, 18),
                                    c.created_at.format("%Y-%m-%d")
                                );
                            }
                            println!("\nTotal: {} customers", customers.len());
                        }
                    }
                    Err(e) => {
                        eprintln!("{}", format!("❌ Failed to list customers: {}", e).red());
                        std::process::exit(1);
                    }
                },
                CustomerCommands::Get { id } => {
                    println!(
                        "{}",
                        format!("Customer details for '{}' coming soon!", id).yellow()
                    );
                }
                CustomerCommands::Create => match interactive_create_customer(&pool).await {
                    Ok(customer) => {
                        println!("{}", "\n✅ Customer created successfully!".green().bold());
                        println!("  ID:    {}", customer.id);
                        println!("  Name:  {} {}", customer.first_name, customer.last_name);
                        println!("  Email: {}", customer.email);
                    }
                    Err(e) => {
                        eprintln!("{}", format!("❌ Failed to create customer: {}", e).red());
                        std::process::exit(1);
                    }
                },
            }
        }

        Commands::ApiKey { command } => {
            use colored::*;

            // Create database pool
            let pool = create_pool(&config).await?;

            match command {
                ApiKeyCommands::List { customer_id } => {
                    match list_api_keys(&pool, customer_id).await {
                        Ok(keys) => {
                            if keys.is_empty() {
                                println!("{}", "No API keys found".yellow());
                            } else {
                                println!("{}", "API Keys".bold().underline());
                                println!(
                                    "{:<12} {:<20} {:<30} {:<10} {:<12}",
                                    "Prefix", "Name", "Scopes", "Active", "Expires"
                                );
                                println!("{}", "-".repeat(90));
                                for key in keys {
                                    let expires = key
                                        .expires_at
                                        .map(|d| d.format("%Y-%m-%d").to_string())
                                        .unwrap_or_else(|| "Never".to_string());
                                    println!(
                                        "{:<12} {:<20} {:<30} {:<10} {:<12}",
                                        key.key_prefix,
                                        key.name,
                                        key.scopes.join(", "),
                                        if key.is_active {
                                            "✓".green()
                                        } else {
                                            "✗".red()
                                        },
                                        expires
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("{}", format!("❌ Failed to list API keys: {}", e).red());
                            std::process::exit(1);
                        }
                    }
                }

                ApiKeyCommands::Create {
                    customer_id,
                    name,
                    scopes,
                    expires_days,
                } => {
                    let auth_service = rcommerce_core::services::AuthService::new(config);

                    match create_api_key(
                        &pool,
                        &auth_service,
                        customer_id,
                        name,
                        scopes,
                        expires_days,
                    )
                    .await
                    {
                        Ok((key, full_key)) => {
                            println!("{}", "✅ API Key created successfully!".green().bold());
                            println!();
                            println!(
                                "{}",
                                "IMPORTANT: Copy this key now - it won't be shown again!"
                                    .red()
                                    .bold()
                            );
                            println!();
                            println!("  Key: {}", full_key.bright_cyan());
                            println!();
                            println!("  Prefix:      {}", key.key_prefix);
                            println!("  Name:        {}", key.name);
                            println!("  Scopes:      {}", key.scopes.join(", "));
                            println!(
                                "  Customer ID: {}",
                                key.customer_id
                                    .map(|id: Uuid| id.to_string())
                                    .unwrap_or_else(|| "System".to_string())
                            );
                            println!(
                                "  Expires:     {}",
                                key.expires_at
                                    .map(|d| d.to_string())
                                    .unwrap_or_else(|| "Never".to_string())
                            );
                        }
                        Err(e) => {
                            eprintln!("{}", format!("❌ Failed to create API key: {}", e).red());
                            std::process::exit(1);
                        }
                    }
                }

                ApiKeyCommands::Get { prefix } => match get_api_key(&pool, &prefix).await {
                    Ok(Some(key)) => {
                        println!("{}", "API Key Details".bold().underline());
                        println!("  ID:           {}", key.id);
                        println!("  Prefix:       {}", key.key_prefix);
                        println!("  Name:         {}", key.name);
                        println!("  Scopes:       {}", key.scopes.join(", "));
                        println!(
                            "  Active:       {}",
                            if key.is_active {
                                "✓ Yes".green()
                            } else {
                                "✗ No".red()
                            }
                        );
                        println!(
                            "  Customer ID:  {}",
                            key.customer_id
                                .map(|id: Uuid| id.to_string())
                                .unwrap_or_else(|| "System".to_string())
                        );
                        println!("  Created:      {}", key.created_at);
                        println!("  Updated:      {}", key.updated_at);
                        println!(
                            "  Expires:      {}",
                            key.expires_at
                                .map(|d| d.to_string())
                                .unwrap_or_else(|| "Never".to_string())
                        );
                        println!(
                            "  Last Used:    {}",
                            key.last_used_at
                                .map(|d| d.to_string())
                                .unwrap_or_else(|| "Never".to_string())
                        );
                        if let Some(last_ip) = key.last_used_ip {
                            println!("  Last IP:      {}", last_ip);
                        }
                        if let Some(revoked_at) = key.revoked_at {
                            println!(
                                "  Revoked:      {} {}",
                                revoked_at,
                                key.revoked_reason.unwrap_or_default().red()
                            );
                        }
                        println!("  Key Hash:     {}...", &key.key_hash[..16]);
                    }
                    Ok(None) => {
                        println!(
                            "{}",
                            format!("API key with prefix '{}' not found", prefix).yellow()
                        );
                    }
                    Err(e) => {
                        eprintln!("{}", format!("❌ Failed to get API key: {}", e).red());
                        std::process::exit(1);
                    }
                },

                ApiKeyCommands::Revoke { prefix, reason } => {
                    match revoke_api_key(&pool, &prefix, reason).await {
                        Ok(true) => {
                            println!(
                                "{}",
                                format!("✅ API key '{}' revoked successfully", prefix).green()
                            );
                        }
                        Ok(false) => {
                            println!(
                                "{}",
                                format!("API key with prefix '{}' not found", prefix).yellow()
                            );
                        }
                        Err(e) => {
                            eprintln!("{}", format!("❌ Failed to revoke API key: {}", e).red());
                            std::process::exit(1);
                        }
                    }
                }

                ApiKeyCommands::Delete { prefix, force } => {
                    if !force {
                        println!(
                            "{}",
                            "⚠️  WARNING: This will PERMANENTLY delete the API key!"
                                .red()
                                .bold()
                        );
                        print!("Type 'yes' to confirm: ");
                        use std::io::Write;
                        std::io::stdout().flush().unwrap();

                        let mut input = String::new();
                        std::io::stdin().read_line(&mut input).unwrap();

                        if input.trim() != "yes" {
                            println!("Aborted.");
                            return Ok(());
                        }
                    }

                    match delete_api_key(&pool, &prefix).await {
                        Ok(true) => {
                            println!(
                                "{}",
                                format!("✅ API key '{}' deleted permanently", prefix).green()
                            );
                        }
                        Ok(false) => {
                            println!(
                                "{}",
                                format!("API key with prefix '{}' not found", prefix).yellow()
                            );
                        }
                        Err(e) => {
                            eprintln!("{}", format!("❌ Failed to delete API key: {}", e).red());
                            std::process::exit(1);
                        }
                    }
                }
            }
        }

        Commands::Import { command } => {
            use colored::*;
            use rcommerce_core::import::types::{ImportOptions, SourceConfig};
            use rcommerce_core::import::{
                get_file_importer, get_platform_importer, ImportConfig as ImportToolConfig,
            };

            match command {
                ImportCommands::Platform {
                    platform,
                    api_url,
                    api_key,
                    api_secret,
                    entities,
                    limit,
                    dry_run,
                    overwrite,
                    currency,
                } => {
                    println!("{} {}", "Importing from".bold(), platform.cyan());

                    // Get the platform importer
                    let importer = match get_platform_importer(&platform) {
                        Some(imp) => imp,
                        None => {
                            eprintln!("{}", format!("❌ Unsupported platform: {}", platform).red());
                            eprintln!("Supported platforms: shopify, woocommerce, magento, medusa");
                            std::process::exit(1);
                        }
                    };

                    // Try to get config from file if not provided via CLI
                    let platform_config = match platform.as_str() {
                        "shopify" => config.import.shopify.as_ref(),
                        "woocommerce" => config.import.woocommerce.as_ref(),
                        "magento" => config.import.magento.as_ref(),
                        "medusa" => config.import.medusa.as_ref(),
                        _ => None,
                    };

                    // Use CLI values or fall back to config file values
                    let final_api_url = if api_url.is_empty() {
                        platform_config
                            .map(|c| c.api_url.clone())
                            .unwrap_or_default()
                    } else {
                        api_url
                    };

                    let final_api_key = if api_key.is_empty() {
                        platform_config
                            .map(|c| c.api_key.clone())
                            .unwrap_or_default()
                    } else {
                        api_key
                    };

                    let final_api_secret =
                        api_secret.or_else(|| platform_config.and_then(|c| c.api_secret.clone()));

                    let final_entities = if entities == "all" {
                        platform_config
                            .map(|c| c.entities.join(","))
                            .unwrap_or_else(|| "all".to_string())
                    } else {
                        entities
                    };

                    // Validate we have required values
                    if final_api_url.is_empty() {
                        eprintln!(
                            "{}",
                            "❌ API URL is required. Provide via --api-url or config file.".red()
                        );
                        std::process::exit(1);
                    }
                    if final_api_key.is_empty() {
                        eprintln!(
                            "{}",
                            "❌ API key is required. Provide via --api-key or config file.".red()
                        );
                        std::process::exit(1);
                    }

                    // Build headers for WooCommerce
                    let mut headers = std::collections::HashMap::new();
                    if let Some(ref secret) = final_api_secret {
                        headers.insert("consumer_secret".to_string(), secret.clone());
                    }
                    // Add any additional headers from config
                    if let Some(pc) = platform_config {
                        for (key, value) in &pc.headers {
                            headers.insert(key.clone(), value.clone());
                        }
                    }

                    // Create import config
                    let import_config = ImportToolConfig {
                        database_url: config.database.url(),
                        source: SourceConfig::Platform {
                            platform: platform.clone(),
                            api_url: final_api_url,
                            api_key: final_api_key,
                            headers,
                        },
                        options: ImportOptions {
                            dry_run,
                            limit: limit.unwrap_or(0),
                            batch_size: config.import.default_options.batch_size,
                            skip_existing: !overwrite, // If overwrite is true, don't skip existing
                            update_existing: overwrite, // If overwrite is true, update existing
                            continue_on_error: config.import.default_options.continue_on_error,
                            default_currency: currency,
                            ..Default::default()
                        },
                    };

                    // Progress callback
                    let progress = |p: rcommerce_core::import::ImportProgress| {
                        let pct = p.percentage();
                        print!(
                            "\r  [{}] {} - {} ({:.1}%)",
                            p.stage.bright_blue(),
                            p.message,
                            format!("{}/{}", p.current, p.total).dimmed(),
                            pct
                        );
                        std::io::Write::flush(&mut std::io::stdout()).unwrap();
                    };

                    // Parse and validate entity types
                    let entity_list: Vec<&str> = if final_entities == "all" {
                        vec!["all"]
                    } else {
                        final_entities.split(',').map(|s| s.trim()).collect()
                    };

                    // Validate all entity types first
                    for entity in &entity_list {
                        match *entity {
                            "products" | "customers" | "orders" | "all" => {}
                            _ => {
                                eprintln!(
                                    "{}",
                                    format!("❌ Invalid entity type: {}", entity).red()
                                );
                                std::process::exit(1);
                            }
                        }
                    }

                    // Run imports for each entity type
                    let mut all_stats = rcommerce_core::import::ImportStats::default();
                    let mut has_error = false;

                    for entity in entity_list {
                        if entity == "all" {
                            println!("\n  {}", "Importing all entities...".bold());
                        } else {
                            println!("\n  {} {}", "Importing".bold(), entity.cyan());
                        }

                        let result = match entity {
                            "products" => importer.import_products(&import_config, &progress).await,
                            "customers" => {
                                importer.import_customers(&import_config, &progress).await
                            }
                            "orders" => importer.import_orders(&import_config, &progress).await,
                            "all" => importer.import_all(&import_config, &progress).await,
                            _ => unreachable!(),
                        };

                        match result {
                            Ok(stats) => {
                                all_stats.created += stats.created;
                                all_stats.updated += stats.updated;
                                all_stats.skipped += stats.skipped;
                                all_stats.errors += stats.errors;
                                all_stats.error_details.extend(stats.error_details);
                                all_stats.total += stats.total;
                            }
                            Err(e) => {
                                eprintln!(
                                    "\n{}",
                                    format!("❌ Error importing {}: {}", entity, e).red()
                                );
                                has_error = true;
                                if !import_config.options.continue_on_error {
                                    std::process::exit(1);
                                }
                            }
                        }
                    }

                    let result = if has_error {
                        Err(rcommerce_core::Error::internal(
                            "One or more imports failed",
                        ))
                    } else {
                        Ok(all_stats)
                    };

                    println!(); // New line after progress

                    match result {
                        Ok(stats) => {
                            println!("{}", "✅ Import completed successfully!".green().bold());
                            println!("  Created:  {}", stats.created.to_string().green());
                            println!("  Updated:  {}", stats.updated.to_string().yellow());
                            println!("  Skipped:  {}", stats.skipped.to_string().dimmed());
                            println!("  Errors:   {}", stats.errors.to_string().red());
                            println!("  Total:    {}", stats.total);

                            if !stats.error_details.is_empty() {
                                println!("\n{}", "Error Details:".red().bold());
                                for error in stats.error_details.iter().take(10) {
                                    println!("  • {}", error);
                                }
                                if stats.error_details.len() > 10 {
                                    println!(
                                        "  ... and {} more errors",
                                        stats.error_details.len() - 10
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("{}", format!("❌ Import failed: {}", e).red());
                            std::process::exit(1);
                        }
                    }
                }

                ImportCommands::File {
                    path,
                    format,
                    entity,
                    dry_run,
                } => {
                    println!(
                        "{} {} → {}",
                        "Importing".bold(),
                        path.display().to_string().cyan(),
                        entity.cyan()
                    );

                    // Get the file importer
                    let importer = match get_file_importer(&format) {
                        Some(imp) => imp,
                        None => {
                            eprintln!("{}", format!("❌ Unsupported format: {}", format).red());
                            eprintln!("Supported formats: csv, json, xml");
                            std::process::exit(1);
                        }
                    };

                    // Parse entity type
                    let entity_type = match entity.as_str() {
                        "products" => rcommerce_core::import::EntityType::Products,
                        "customers" => rcommerce_core::import::EntityType::Customers,
                        "orders" => rcommerce_core::import::EntityType::Orders,
                        _ => {
                            eprintln!("{}", format!("❌ Invalid entity type: {}", entity).red());
                            std::process::exit(1);
                        }
                    };

                    // Create import config
                    let import_config = ImportToolConfig {
                        database_url: config.database.url(),
                        source: SourceConfig::File {
                            path: path.clone(),
                            format: format.clone(),
                        },
                        options: ImportOptions {
                            dry_run,
                            batch_size: config.import.default_options.batch_size,
                            skip_existing: config.import.default_options.skip_existing,
                            continue_on_error: config.import.default_options.continue_on_error,
                            ..Default::default()
                        },
                    };

                    // Progress callback
                    let progress = |p: rcommerce_core::import::ImportProgress| {
                        let pct = p.percentage();
                        print!(
                            "\r  {} - {} ({:.1}%)",
                            p.message,
                            format!("{}/{}", p.current, p.total).dimmed(),
                            pct
                        );
                        std::io::Write::flush(&mut std::io::stdout()).unwrap();
                    };

                    // Run import
                    match importer
                        .import_file(&path, entity_type, &import_config, &progress)
                        .await
                    {
                        Ok(stats) => {
                            println!();
                            println!("{}", "✅ Import completed successfully!".green().bold());
                            println!("  Created:  {}", stats.created.to_string().green());
                            println!("  Updated:  {}", stats.updated.to_string().yellow());
                            println!("  Skipped:  {}", stats.skipped.to_string().dimmed());
                            println!("  Errors:   {}", stats.errors.to_string().red());
                            println!("  Total:    {}", stats.total);
                        }
                        Err(e) => {
                            eprintln!("{}", format!("❌ Import failed: {}", e).red());
                            std::process::exit(1);
                        }
                    }
                }
            }
        }

        Commands::Tls { command } => {
            use colored::*;
            use rcommerce_core::config::TlsVersion;

            // Get TLS config from the main config
            let tls_config = &config.tls;

            // Ensure TLS is enabled
            if !tls_config.enabled {
                eprintln!("{}", "❌ TLS is not enabled in configuration".red());
                std::process::exit(1);
            }

            // Check minimum TLS version
            if tls_config.min_tls_version < TlsVersion::Tls1_3 {
                eprintln!(
                    "{}",
                    "⚠️  Warning: Minimum TLS version should be 1.3 or higher".yellow()
                );
            }

            match command {
                TlsCommands::Check { domain } => {
                    println!("{} {}", "Checking certificate for".bold(), domain.cyan());

                    // Get Let's Encrypt config
                    let le_config = match &tls_config.lets_encrypt {
                        Some(config) if config.enabled => config.clone(),
                        _ => {
                            eprintln!("{}", "❌ Let's Encrypt is not configured or disabled".red());
                            std::process::exit(1);
                        }
                    };

                    // Check certificate on disk
                    match check_certificate_disk(&le_config, &domain).await {
                        Ok(Some(info)) => {
                            let now = chrono::Utc::now();
                            let days_until_expiry = (info.expires_at - now).num_days();

                            println!("\n{}", "Certificate Status".bold().underline());
                            println!("  Domain:          {}", info.domain.cyan());
                            println!(
                                "  Status:          {}",
                                if days_until_expiry > 30 {
                                    "✓ Valid".green()
                                } else if days_until_expiry > 7 {
                                    "⚠ Expiring soon".yellow()
                                } else {
                                    "✗ Critical".red()
                                }
                            );
                            println!(
                                "  Expires:         {} ({} days)",
                                info.expires_at.format("%Y-%m-%d %H:%M:%S UTC"),
                                days_until_expiry
                            );
                            println!(
                                "  Issued:          {}",
                                info.issued_at.format("%Y-%m-%d %H:%M:%S UTC")
                            );
                            println!("  Serial Number:   {}", info.serial_number);
                            println!("  Certificate:     {}", info.certificate_path.display());
                            println!("  Private Key:     {}", info.private_key_path.display());
                        }
                        Ok(None) => {
                            println!(
                                "{}",
                                format!("No certificate found for '{}'", domain).yellow()
                            );
                            println!(
                                "Run 'rcommerce tls renew --domain {}' to obtain a certificate",
                                domain
                            );
                        }
                        Err(e) => {
                            eprintln!("{}", format!("❌ Failed to check certificate: {}", e).red());
                            std::process::exit(1);
                        }
                    }
                }

                TlsCommands::List => {
                    println!("{}", "TLS Certificates".bold().underline());

                    // Get cache directory from config
                    let cache_dir = tls_config
                        .lets_encrypt
                        .as_ref()
                        .map(|le| le.cache_dir.clone())
                        .or_else(|| Some(std::path::PathBuf::from("/var/lib/rcommerce/certs")))
                        .unwrap();

                    // List certificates in cache directory
                    match list_certificates(&cache_dir).await {
                        Ok(certs) => {
                            if certs.is_empty() {
                                println!("{}", "No certificates found in cache".yellow());
                                println!("Cache directory: {}", cache_dir.display());
                            } else {
                                println!(
                                    "{:<30} {:<12} {:<20} {:<12}",
                                    "Domain", "Status", "Expires", "Days Left"
                                );
                                println!("{}", "-".repeat(80));

                                let now = chrono::Utc::now();
                                for cert in &certs {
                                    let days_left = (cert.expires_at - now).num_days();
                                    let status = if days_left > 30 {
                                        "✓ Valid".green()
                                    } else if days_left > 7 {
                                        "⚠ Soon".yellow()
                                    } else if days_left > 0 {
                                        "✗ Critical".red()
                                    } else {
                                        "✗ Expired".red().bold()
                                    };

                                    println!(
                                        "{:<30} {:<12} {:<20} {:<12}",
                                        truncate(&cert.domain, 28),
                                        status,
                                        cert.expires_at.format("%Y-%m-%d"),
                                        if days_left >= 0 {
                                            days_left.to_string()
                                        } else {
                                            "Expired".to_string()
                                        }
                                    );
                                }
                                println!("\nTotal: {} certificates", certs.len());
                            }
                        }
                        Err(e) => {
                            eprintln!("{}", format!("❌ Failed to list certificates: {}", e).red());
                            std::process::exit(1);
                        }
                    }
                }

                TlsCommands::Renew { domain, force } => {
                    println!(
                        "{} {}",
                        if force {
                            "Force renewing".bold()
                        } else {
                            "Renewing certificate for".bold()
                        },
                        domain.cyan()
                    );

                    // Get Let's Encrypt config
                    let le_config = match &tls_config.lets_encrypt {
                        Some(config) if config.enabled => config.clone(),
                        _ => {
                            eprintln!("{}", "❌ Let's Encrypt is not configured or disabled".red());
                            std::process::exit(1);
                        }
                    };

                    // Check if domain is in configured domains
                    if !le_config.domains.contains(&domain)
                        && !le_config.domains.iter().any(|d| d.starts_with("*."))
                    {
                        eprintln!(
                            "{}",
                            format!("⚠️  Warning: '{}' is not in configured domains", domain)
                                .yellow()
                        );
                        eprintln!("Configured domains: {}", le_config.domains.join(", "));
                    }

                    // Check if renewal is needed (unless force)
                    if !force {
                        if let Ok(Some(info)) = check_certificate_disk(&le_config, &domain).await {
                            let now = chrono::Utc::now();
                            let days_until_expiry = (info.expires_at - now).num_days();

                            if days_until_expiry > 30 {
                                println!("{}", format!("Certificate is still valid for {} days. Use --force to renew anyway.", days_until_expiry).yellow());
                                return Ok(());
                            }
                        }
                    }

                    match obtain_certificate_stub(&le_config, &domain).await {
                        Ok(info) => {
                            println!(
                                "{}",
                                format!("✅ Certificate renewed successfully for '{}'", domain)
                                    .green()
                                    .bold()
                            );
                            println!(
                                "  Expires:       {}",
                                info.expires_at.format("%Y-%m-%d %H:%M:%S UTC")
                            );
                            println!("  Serial Number: {}", info.serial_number);
                            println!("  Certificate:   {}", info.certificate_path.display());
                            println!("  Private Key:   {}", info.private_key_path.display());
                        }
                        Err(e) => {
                            eprintln!("{}", format!("❌ Failed to renew certificate: {}", e).red());
                            std::process::exit(1);
                        }
                    }
                }

                TlsCommands::Info { domain } => {
                    println!("{} {}", "Certificate information for".bold(), domain.cyan());

                    // Get Let's Encrypt config
                    let le_config = match &tls_config.lets_encrypt {
                        Some(config) if config.enabled => config.clone(),
                        _ => {
                            eprintln!("{}", "❌ Let's Encrypt is not configured or disabled".red());
                            std::process::exit(1);
                        }
                    };

                    match check_certificate_disk(&le_config, &domain).await {
                        Ok(Some(info)) => {
                            let now = chrono::Utc::now();
                            let days_until_expiry = (info.expires_at - now).num_days();

                            println!("\n{}", "Certificate Details".bold().underline());
                            println!("  Domain:             {}", info.domain.cyan());
                            println!("  Serial Number:      {}", info.serial_number);
                            println!(
                                "  Issued At:          {}",
                                info.issued_at.format("%Y-%m-%d %H:%M:%S UTC")
                            );
                            println!(
                                "  Expires At:         {}",
                                info.expires_at.format("%Y-%m-%d %H:%M:%S UTC")
                            );
                            println!(
                                "  Days Until Expiry:  {}",
                                if days_until_expiry > 30 {
                                    days_until_expiry.to_string().green()
                                } else if days_until_expiry > 7 {
                                    days_until_expiry.to_string().yellow()
                                } else {
                                    days_until_expiry.to_string().red()
                                }
                            );

                            println!("\n{}", "File Locations".bold().underline());
                            println!("  Certificate Path:   {}", info.certificate_path.display());
                            println!("  Private Key Path:   {}", info.private_key_path.display());

                            println!("\n{}", "Let's Encrypt Configuration".bold().underline());
                            println!("  Email:              {}", le_config.email);
                            println!("  Domains:            {}", le_config.domains.join(", "));
                            println!("  ACME Directory:     {}", le_config.acme_directory);
                            println!(
                                "  Use Staging:        {}",
                                if le_config.use_staging {
                                    "Yes".yellow()
                                } else {
                                    "No".green()
                                }
                            );
                            println!(
                                "  Auto Renew:         {}",
                                if le_config.auto_renew {
                                    "Enabled".green()
                                } else {
                                    "Disabled".red()
                                }
                            );
                            println!(
                                "  Renewal Threshold:  {} days",
                                le_config.renewal_threshold_days
                            );
                            println!("  Cache Directory:    {}", le_config.cache_dir.display());

                            // Check if files exist
                            let cert_exists = info.certificate_path.exists();
                            let key_exists = info.private_key_path.exists();

                            println!("\n{}", "File Status".bold().underline());
                            println!(
                                "  Certificate File:   {}",
                                if cert_exists {
                                    "✓ Exists".green()
                                } else {
                                    "✗ Missing".red()
                                }
                            );
                            println!(
                                "  Private Key File:   {}",
                                if key_exists {
                                    "✓ Exists".green()
                                } else {
                                    "✗ Missing".red()
                                }
                            );
                        }
                        Ok(None) => {
                            println!(
                                "{}",
                                format!("No certificate found for '{}'", domain).yellow()
                            );
                            println!("\nLet's Encrypt Configuration:");
                            println!("  Email:           {}", le_config.email);
                            println!("  Domains:         {}", le_config.domains.join(", "));
                            println!("  Cache Directory: {}", le_config.cache_dir.display());
                            println!(
                                "\nRun 'rcommerce tls renew --domain {}' to obtain a certificate",
                                domain
                            );
                        }
                        Err(e) => {
                            eprintln!(
                                "{}",
                                format!("❌ Failed to get certificate info: {}", e).red()
                            );
                            std::process::exit(1);
                        }
                    }
                }
            }
        }

        Commands::Config => {
            println!(
                "Configuration loaded from: {}",
                cli.config
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "environment".to_string())
            );
            println!("{:#?}", config);
        }

        Commands::Email { command } => {
            use colored::*;

            match command {
                EmailCommands::List => {
                    println!("{}", "Available Email Templates".bold().underline());
                    println!();

                    let templates = vec![
                        (
                            "order_confirmation",
                            "Order Confirmation",
                            "Sent when a new order is placed",
                        ),
                        (
                            "order_shipped",
                            "Order Shipped",
                            "Sent when an order is shipped",
                        ),
                        (
                            "order_cancelled",
                            "Order Cancelled",
                            "Sent when an order is cancelled",
                        ),
                        (
                            "payment_successful",
                            "Payment Successful",
                            "Sent when payment is confirmed",
                        ),
                        (
                            "payment_failed",
                            "Payment Failed",
                            "Sent when payment fails",
                        ),
                        (
                            "refund_processed",
                            "Refund Processed",
                            "Sent when a refund is processed",
                        ),
                        (
                            "subscription_created",
                            "Subscription Created",
                            "Sent when a subscription is created",
                        ),
                        (
                            "subscription_renewal",
                            "Subscription Renewal",
                            "Sent when a subscription renews",
                        ),
                        (
                            "subscription_cancelled",
                            "Subscription Cancelled",
                            "Sent when a subscription is cancelled",
                        ),
                        (
                            "dunning_first",
                            "Dunning: First Notice",
                            "First payment failure notice",
                        ),
                        (
                            "dunning_retry",
                            "Dunning: Retry Notice",
                            "Subsequent payment retry notice",
                        ),
                        (
                            "dunning_final",
                            "Dunning: Final Notice",
                            "Final notice before cancellation",
                        ),
                        ("welcome", "Welcome", "Sent to new customers"),
                        (
                            "password_reset",
                            "Password Reset",
                            "Sent for password reset requests",
                        ),
                        (
                            "abandoned_cart",
                            "Abandoned Cart",
                            "Sent for abandoned cart reminders",
                        ),
                    ];

                    println!("{:<25} {:<30} Description", "Template ID", "Name");
                    println!("{}", "-".repeat(100));
                    for (id, name, desc) in &templates {
                        println!("{:<25} {:<30} {}", id.cyan(), name, desc.dimmed());
                    }
                    println!();
                    println!("Total: {} templates", templates.len());
                }

                EmailCommands::TestAll {
                    output_dir,
                    recipient,
                } => {
                    let recipient = recipient.unwrap_or_else(|| "test@example.com".to_string());
                    println!("{}", "Testing All Email Templates".bold().underline());
                    println!("  Output directory: {}", output_dir.cyan());
                    println!("  Recipient: {}", recipient.cyan());
                    println!();

                    match test_all_email_templates(&output_dir, &recipient).await {
                        Ok(count) => {
                            println!(
                                "\n{}",
                                format!("✅ Successfully generated {} test emails", count)
                                    .green()
                                    .bold()
                            );
                            println!("  Files saved to: {}", output_dir.cyan());
                        }
                        Err(e) => {
                            eprintln!(
                                "{}",
                                format!("❌ Failed to generate test emails: {}", e).red()
                            );
                            std::process::exit(1);
                        }
                    }
                }

                EmailCommands::Test {
                    template,
                    output_dir,
                    recipient,
                } => {
                    let recipient = recipient.unwrap_or_else(|| "test@example.com".to_string());
                    println!(
                        "{}",
                        format!("Testing Email Template: {}", template)
                            .bold()
                            .underline()
                    );
                    println!("  Output directory: {}", output_dir.cyan());
                    println!("  Recipient: {}", recipient.cyan());
                    println!();

                    match test_email_template(&template, &output_dir, &recipient).await {
                        Ok(filepath) => {
                            println!(
                                "\n{}",
                                "✅ Test email generated successfully".green().bold()
                            );
                            println!("  File: {}", filepath.cyan());
                        }
                        Err(e) => {
                            eprintln!(
                                "{}",
                                format!("❌ Failed to generate test email: {}", e).red()
                            );
                            std::process::exit(1);
                        }
                    }
                }

                EmailCommands::Send { template, to, mock } => {
                    println!(
                        "{}",
                        format!("Sending Test Email: {}", template)
                            .bold()
                            .underline()
                    );
                    println!("  Recipient: {}", to.cyan());
                    println!(
                        "  Mode: {}",
                        if mock {
                            "Mock (console output)".yellow()
                        } else {
                            "SMTP".green()
                        }
                    );
                    println!();

                    if mock {
                        match send_mock_email(&template, &to).await {
                            Ok(_) => {
                                println!("\n{}", "✅ Mock email sent to console".green());
                            }
                            Err(e) => {
                                eprintln!(
                                    "{}",
                                    format!("❌ Failed to send mock email: {}", e).red()
                                );
                                std::process::exit(1);
                            }
                        }
                    } else {
                        // SMTP mode requires configuration
                        eprintln!("{}", "❌ SMTP sending not yet implemented via CLI".red());
                        eprintln!("Use --mock flag to test with console output, or use the API.");
                        std::process::exit(1);
                    }
                }
            }
        }

        Commands::Setup { output } => {
            if let Err(e) = commands::setup::run_setup(output).await {
                eprintln!("{}", format!("❌ Setup failed: {}", e).red().bold());
                std::process::exit(1);
            }
        }

        Commands::Shell => {
            if let Err(e) = security::check_not_root() {
                eprintln!("{}", e);
                std::process::exit(1);
            }

            let pool = create_pool(&config).await?;

            if let Err(e) = commands::shell::run_shell(pool).await {
                eprintln!("{}", format!("❌ Shell error: {}", e).red().bold());
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

/// Create database pool from config
async fn create_pool(config: &Config) -> Result<sqlx::PgPool> {
    use sqlx::postgres::PgPoolOptions;

    let database_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        config.database.username,
        config.database.password,
        config.database.host,
        config.database.port,
        config.database.database
    );

    let pool = PgPoolOptions::new()
        .max_connections(config.database.pool_size)
        .connect(&database_url)
        .await
        .map_err(rcommerce_core::Error::Database)?;

    Ok(pool)
}

/// Run database migrations
async fn run_migrations(config: &Config) -> Result<()> {
    let pool = create_pool(config).await?;
    rcommerce_core::auto_migrate(&pool).await?;
    Ok(())
}

// API Key management functions

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// API Key record from database
#[derive(Debug, sqlx::FromRow)]
struct ApiKeyRecord {
    id: Uuid,
    customer_id: Option<Uuid>,
    key_prefix: String,
    key_hash: String,
    name: String,
    scopes: Vec<String>,
    expires_at: Option<DateTime<Utc>>,
    last_used_at: Option<DateTime<Utc>>,
    last_used_ip: Option<String>,
    is_active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    revoked_reason: Option<String>,
}

/// List API keys
async fn list_api_keys(
    pool: &sqlx::PgPool,
    customer_id: Option<String>,
) -> Result<Vec<ApiKeyRecord>> {
    let keys = if let Some(cid) = customer_id {
        let cid = Uuid::parse_str(&cid).map_err(|e| {
            rcommerce_core::Error::validation(format!("Invalid customer ID: {}", e))
        })?;
        sqlx::query_as::<_, ApiKeyRecord>(
            "SELECT * FROM api_keys WHERE customer_id = $1 ORDER BY created_at DESC",
        )
        .bind(cid)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, ApiKeyRecord>("SELECT * FROM api_keys ORDER BY created_at DESC")
            .fetch_all(pool)
            .await?
    };

    Ok(keys)
}

/// Create a new API key
async fn create_api_key(
    pool: &sqlx::PgPool,
    auth_service: &rcommerce_core::services::AuthService,
    customer_id: Option<String>,
    name: Option<String>,
    scopes: String,
    expires_days: Option<i64>,
) -> Result<(ApiKeyRecord, String)> {
    // Generate API key
    let api_key = auth_service.generate_api_key();
    let full_key = api_key.full_key.clone().unwrap();

    // Parse customer ID if provided
    let customer_uuid = if let Some(cid) = customer_id {
        Some(Uuid::parse_str(&cid).map_err(|e| {
            rcommerce_core::Error::validation(format!("Invalid customer ID: {}", e))
        })?)
    } else {
        None
    };

    // Calculate expiration
    let expires_at = expires_days.map(|days| Utc::now() + chrono::Duration::days(days));

    // Parse scopes
    let scopes_vec: Vec<String> = scopes.split(',').map(|s| s.trim().to_string()).collect();

    // Insert into database
    let key = sqlx::query_as::<_, ApiKeyRecord>(
        r#"
        INSERT INTO api_keys (customer_id, key_prefix, key_hash, name, scopes, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(customer_uuid)
    .bind(&api_key.prefix)
    .bind(&api_key.hash)
    .bind(name.unwrap_or_else(|| "API Key".to_string()))
    .bind(&scopes_vec)
    .bind(expires_at)
    .fetch_one(pool)
    .await?;

    Ok((key, full_key))
}

/// Get API key by prefix
async fn get_api_key(pool: &sqlx::PgPool, prefix: &str) -> Result<Option<ApiKeyRecord>> {
    let key = sqlx::query_as::<_, ApiKeyRecord>("SELECT * FROM api_keys WHERE key_prefix = $1")
        .bind(prefix)
        .fetch_optional(pool)
        .await?;

    Ok(key)
}

/// Revoke an API key
async fn revoke_api_key(pool: &sqlx::PgPool, prefix: &str, reason: Option<String>) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE api_keys 
        SET is_active = false, revoked_at = NOW(), revoked_reason = $2
        WHERE key_prefix = $1
        "#,
    )
    .bind(prefix)
    .bind(reason)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Delete an API key permanently
async fn delete_api_key(pool: &sqlx::PgPool, prefix: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM api_keys WHERE key_prefix = $1")
        .bind(prefix)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

// Product CLI functions

#[derive(Debug, sqlx::FromRow)]
struct ProductRecord {
    id: Uuid,
    title: String,
    slug: String,
    price: rust_decimal::Decimal,
    currency: String,
    description: Option<String>,
    is_active: bool,
    inventory_quantity: i32,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// List all products
async fn list_products(pool: &sqlx::PgPool) -> Result<Vec<ProductRecord>> {
    let products = sqlx::query_as::<_, ProductRecord>(
        "SELECT id, title, slug, price, currency::text, description, is_active, inventory_quantity, created_at 
         FROM products ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await?;

    Ok(products)
}

/// Get product by ID
async fn get_product(pool: &sqlx::PgPool, id: &str) -> Result<Option<ProductRecord>> {
    let product_id = Uuid::parse_str(id)
        .map_err(|e| rcommerce_core::Error::validation(format!("Invalid product ID: {}", e)))?;

    let product = sqlx::query_as::<_, ProductRecord>(
        "SELECT id, title, slug, price, currency::text, description, is_active, inventory_quantity, created_at 
         FROM products WHERE id = $1"
    )
    .bind(product_id)
    .fetch_optional(pool)
    .await?;

    Ok(product)
}

/// Delete product by ID
async fn delete_product(pool: &sqlx::PgPool, id: &str) -> Result<bool> {
    let product_id = Uuid::parse_str(id)
        .map_err(|e| rcommerce_core::Error::validation(format!("Invalid product ID: {}", e)))?;

    let result = sqlx::query("DELETE FROM products WHERE id = $1")
        .bind(product_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

// Order CLI functions

#[derive(Debug, sqlx::FromRow)]
struct OrderRecord {
    id: Uuid,
    customer_email: String,
    status: String,
    total: rust_decimal::Decimal,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// List all orders
async fn list_orders(pool: &sqlx::PgPool) -> Result<Vec<OrderRecord>> {
    let orders = sqlx::query_as::<_, OrderRecord>(
        "SELECT o.id, c.email as customer_email, o.status::text, o.total, o.created_at 
         FROM orders o 
         JOIN customers c ON o.customer_id = c.id 
         ORDER BY o.created_at DESC",
    )
    .fetch_all(pool)
    .await?;

    Ok(orders)
}

// Customer CLI functions

#[derive(Debug, sqlx::FromRow)]
struct CustomerRecord {
    id: Uuid,
    email: String,
    first_name: String,
    last_name: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// List all customers
async fn list_customers(pool: &sqlx::PgPool) -> Result<Vec<CustomerRecord>> {
    let customers = sqlx::query_as::<_, CustomerRecord>(
        "SELECT id, email, first_name, last_name, created_at 
         FROM customers ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;

    Ok(customers)
}

/// Helper function to truncate strings
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    } else {
        s.to_string()
    }
}

// Interactive creation functions

/// Simple slugify function - converts string to URL-friendly slug
fn slugify(input: &str) -> String {
    input
        .to_lowercase()
        .replace(' ', "-")
        .replace(|c: char| !c.is_alphanumeric() && c != '-', "")
        .replace("--", "-")
        .trim_matches('-')
        .to_string()
}

/// Interactive product creation using dialoguer
async fn interactive_create_product(pool: &sqlx::PgPool) -> Result<ProductRecord> {
    use colored::Colorize;
    use rust_decimal::Decimal;

    println!("{}", "\n📦 Create New Product".bold().underline());
    println!("{}", "Press Ctrl+C to cancel at any time.\n".dimmed());

    // Product title
    let title: String = Input::new()
        .with_prompt("Product title")
        .validate_with(|input: &String| {
            if input.trim().is_empty() {
                Err("Title is required")
            } else if input.len() > 255 {
                Err("Title must be less than 255 characters")
            } else {
                Ok(())
            }
        })
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

    // Auto-generate slug from title
    let default_slug = slugify(&title);
    let slug: String = Input::new()
        .with_prompt("URL slug")
        .default(default_slug)
        .validate_with(|input: &String| {
            if input.trim().is_empty() {
                Err("Slug is required")
            } else if input.len() > 255 {
                Err("Slug must be less than 255 characters")
            } else {
                Ok(())
            }
        })
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

    // Product type selection
    let product_types = vec!["Simple", "Variable", "Digital", "Bundle"];
    let type_index = Select::new()
        .with_prompt("Product type")
        .items(&product_types)
        .default(0)
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Selection error: {}", e)))?;
    let product_type = match type_index {
        0 => ProductType::Simple,
        1 => ProductType::Variable,
        2 => ProductType::Digital,
        3 => ProductType::Bundle,
        _ => ProductType::Simple,
    };

    // Price
    let price_str: String = Input::new()
        .with_prompt("Price")
        .validate_with(|input: &String| match input.parse::<Decimal>() {
            Ok(d) if d >= Decimal::ZERO => Ok(()),
            Ok(_) => Err("Price must be non-negative"),
            Err(_) => Err("Please enter a valid number"),
        })
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;
    let price: Decimal = price_str.parse().unwrap();

    // Currency selection
    let currencies = vec![
        "USD", "EUR", "GBP", "JPY", "AUD", "CAD", "CNY", "HKD", "SGD",
    ];
    let currency_index = Select::new()
        .with_prompt("Currency")
        .items(&currencies)
        .default(0)
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Selection error: {}", e)))?;
    let currency = match currency_index {
        0 => Currency::USD,
        1 => Currency::EUR,
        2 => Currency::GBP,
        3 => Currency::JPY,
        4 => Currency::AUD,
        5 => Currency::CAD,
        6 => Currency::CNY,
        7 => Currency::HKD,
        8 => Currency::SGD,
        _ => Currency::USD,
    };

    // SKU (optional) - use String then convert to Option
    let sku_input: String = Input::new()
        .with_prompt("SKU (optional)")
        .allow_empty(true)
        .validate_with(|input: &String| {
            if input.len() > 100 {
                Err("SKU must be less than 100 characters")
            } else {
                Ok(())
            }
        })
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;
    let sku = if sku_input.trim().is_empty() {
        None
    } else {
        Some(sku_input)
    };

    // Inventory quantity
    let inventory_str: String = Input::new()
        .with_prompt("Inventory quantity")
        .default("0".to_string())
        .validate_with(|input: &String| match input.parse::<i32>() {
            Ok(_) => Ok(()),
            Err(_) => Err("Please enter a valid integer"),
        })
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;
    let inventory_quantity: i32 = inventory_str.parse().unwrap();

    // Description (optional) - use String then convert to Option
    let desc_input: String = Input::new()
        .with_prompt("Description (optional)")
        .allow_empty(true)
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;
    let description = if desc_input.trim().is_empty() {
        None
    } else {
        Some(desc_input)
    };

    // Active status
    let is_active = Confirm::new()
        .with_prompt("Make product active?")
        .default(true)
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

    // Featured status
    let is_featured = Confirm::new()
        .with_prompt("Mark as featured?")
        .default(false)
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

    // Summary
    println!("\n{}", "📋 Product Summary".bold().underline());
    println!("  Title:       {}", title);
    println!("  Slug:        {}", slug);
    println!("  Type:        {:?}", product_type);
    println!("  Price:       {} {}", price, currency);
    println!("  SKU:         {}", sku.as_deref().unwrap_or("(none)"));
    println!("  Inventory:   {}", inventory_quantity);
    println!(
        "  Description: {}",
        description.as_deref().unwrap_or("(none)")
    );
    println!(
        "  Active:      {}",
        if is_active { "Yes".green() } else { "No".red() }
    );
    println!(
        "  Featured:    {}",
        if is_featured {
            "Yes".green()
        } else {
            "No".red()
        }
    );

    // Final confirmation
    let confirmed = Confirm::new()
        .with_prompt("\nCreate this product?")
        .default(true)
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

    if !confirmed {
        return Err(rcommerce_core::Error::validation(
            "Product creation cancelled",
        ));
    }

    // Insert into database
    let product = sqlx::query_as::<_, ProductRecord>(
        r#"
        INSERT INTO products (
            title, slug, description, sku, product_type, price, currency,
            inventory_quantity, inventory_management, is_active, is_featured,
            requires_shipping
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        RETURNING id, title, slug, price, currency::text, description, is_active, inventory_quantity, created_at
        "#
    )
    .bind(&title)
    .bind(&slug)
    .bind(&description)
    .bind(&sku)
    .bind(product_type)
    .bind(price)
    .bind(currency)
    .bind(inventory_quantity)
    .bind(true) // inventory_management
    .bind(is_active)
    .bind(is_featured)
    .bind(!matches!(product_type, ProductType::Digital)) // requires_shipping (false for digital)
    .fetch_one(pool)
    .await?;

    Ok(product)
}

/// Interactive customer creation using dialoguer
async fn interactive_create_customer(pool: &sqlx::PgPool) -> Result<CustomerRecord> {
    use colored::Colorize;

    println!("{}", "\n👤 Create New Customer".bold().underline());
    println!("{}", "Press Ctrl+C to cancel at any time.\n".dimmed());

    // Email
    let email: String = Input::new()
        .with_prompt("Email address")
        .validate_with(|input: &String| {
            if input.trim().is_empty() {
                Err("Email is required")
            } else if !input.contains('@') || !input.contains('.') {
                Err("Please enter a valid email address")
            } else if input.len() > 255 {
                Err("Email must be less than 255 characters")
            } else {
                Ok(())
            }
        })
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

    // First name
    let first_name: String = Input::new()
        .with_prompt("First name")
        .validate_with(|input: &String| {
            if input.trim().is_empty() {
                Err("First name is required")
            } else if input.len() > 100 {
                Err("First name must be less than 100 characters")
            } else {
                Ok(())
            }
        })
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

    // Last name
    let last_name: String = Input::new()
        .with_prompt("Last name")
        .validate_with(|input: &String| {
            if input.trim().is_empty() {
                Err("Last name is required")
            } else if input.len() > 100 {
                Err("Last name must be less than 100 characters")
            } else {
                Ok(())
            }
        })
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

    // Phone (optional) - use String then convert to Option
    let phone_input: String = Input::new()
        .with_prompt("Phone number (optional)")
        .allow_empty(true)
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;
    let phone = if phone_input.trim().is_empty() {
        None
    } else {
        Some(phone_input)
    };

    // Currency selection
    let currencies = vec![
        "USD", "EUR", "GBP", "JPY", "AUD", "CAD", "CNY", "HKD", "SGD",
    ];
    let currency_index = Select::new()
        .with_prompt("Preferred currency")
        .items(&currencies)
        .default(0)
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Selection error: {}", e)))?;
    let currency = match currency_index {
        0 => Currency::USD,
        1 => Currency::EUR,
        2 => Currency::GBP,
        3 => Currency::JPY,
        4 => Currency::AUD,
        5 => Currency::CAD,
        6 => Currency::CNY,
        7 => Currency::HKD,
        8 => Currency::SGD,
        _ => Currency::USD,
    };

    // Marketing consent
    let accepts_marketing = Confirm::new()
        .with_prompt("Accepts marketing emails?")
        .default(false)
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

    // Password
    let password = Password::new()
        .with_prompt("Password")
        .with_confirmation("Confirm password", "Passwords do not match")
        .validate_with(|input: &String| {
            if input.len() < 8 {
                Err("Password must be at least 8 characters")
            } else if input.len() > 128 {
                Err("Password must be less than 128 characters")
            } else {
                Ok(())
            }
        })
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Password error: {}", e)))?;

    // Summary
    println!("\n{}", "📋 Customer Summary".bold().underline());
    println!("  Name:              {} {}", first_name, last_name);
    println!("  Email:             {}", email);
    println!(
        "  Phone:             {}",
        phone.as_deref().unwrap_or("(none)")
    );
    println!("  Currency:          {:?}", currency);
    println!(
        "  Accepts Marketing: {}",
        if accepts_marketing {
            "Yes".green()
        } else {
            "No".red()
        }
    );

    // Final confirmation
    let confirmed = Confirm::new()
        .with_prompt("\nCreate this customer?")
        .default(true)
        .interact()
        .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

    if !confirmed {
        return Err(rcommerce_core::Error::validation(
            "Customer creation cancelled",
        ));
    }

    // Hash password
    let password_hash = bcrypt::hash(&password, bcrypt::DEFAULT_COST).map_err(|e| {
        rcommerce_core::Error::validation(format!("Failed to hash password: {}", e))
    })?;

    // Insert into database
    let customer = sqlx::query_as::<_, CustomerRecord>(
        r#"
        INSERT INTO customers (
            email, first_name, last_name, phone, accepts_marketing, 
            currency, password_hash, is_verified
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, email, first_name, last_name, created_at
        "#,
    )
    .bind(&email)
    .bind(&first_name)
    .bind(&last_name)
    .bind(&phone)
    .bind(accepts_marketing)
    .bind(currency)
    .bind(&password_hash)
    .bind(true) // is_verified
    .fetch_one(pool)
    .await?;

    Ok(customer)
}

// TLS helper structures and functions

use rcommerce_core::config::LetsEncryptConfig;

/// Certificate information structure
#[derive(Debug, Clone)]
pub struct CertificateInfo {
    pub domain: String,
    pub certificate_path: std::path::PathBuf,
    pub private_key_path: std::path::PathBuf,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub serial_number: String,
}

/// Get certificate path for a domain
fn get_cert_path(cache_dir: &std::path::Path, domain: &str) -> std::path::PathBuf {
    cache_dir.join(format!("{}-cert.pem", domain.replace('*', "wildcard")))
}

/// Get private key path for a domain
fn get_key_path(cache_dir: &std::path::Path, domain: &str) -> std::path::PathBuf {
    cache_dir.join(format!("{}-key.pem", domain.replace('*', "wildcard")))
}

/// Check certificate for a domain on disk
async fn check_certificate_disk(
    le_config: &LetsEncryptConfig,
    domain: &str,
) -> rcommerce_core::Result<Option<CertificateInfo>> {
    // Try to load certificate from disk
    let cert_path = get_cert_path(&le_config.cache_dir, domain);
    let key_path = get_key_path(&le_config.cache_dir, domain);

    if !cert_path.exists() {
        return Ok(None);
    }

    // Parse certificate info from file
    let cert_info = parse_certificate_file(domain, &cert_path, &key_path)?;
    Ok(Some(cert_info))
}

/// Parse certificate file to extract info
fn parse_certificate_file(
    domain: &str,
    cert_path: &std::path::Path,
    key_path: &std::path::Path,
) -> rcommerce_core::Result<CertificateInfo> {
    use std::io::Read;

    // Read certificate file
    let mut cert_file = std::fs::File::open(cert_path)
        .map_err(|e| rcommerce_core::Error::Config(format!("Failed to open cert file: {}", e)))?;

    let mut _cert_pem = String::new();
    cert_file
        .read_to_string(&mut _cert_pem)
        .map_err(|e| rcommerce_core::Error::Config(format!("Failed to read cert file: {}", e)))?;

    // For now, use file modification time as issued time
    let metadata = std::fs::metadata(cert_path).map_err(|e| {
        rcommerce_core::Error::Config(format!("Failed to read cert metadata: {}", e))
    })?;

    let issued_at = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| {
            chrono::DateTime::from_timestamp(d.as_secs() as i64, 0).unwrap_or_else(chrono::Utc::now)
        })
        .unwrap_or_else(chrono::Utc::now);

    // Assume 90-day validity for Let's Encrypt certificates
    let expires_at = issued_at + chrono::Duration::days(90);

    Ok(CertificateInfo {
        domain: domain.to_string(),
        certificate_path: cert_path.to_path_buf(),
        private_key_path: key_path.to_path_buf(),
        expires_at,
        issued_at,
        serial_number: format!("sn-{}", uuid::Uuid::new_v4()),
    })
}

/// List all certificates in the cache directory
async fn list_certificates(
    cache_dir: &std::path::Path,
) -> rcommerce_core::Result<Vec<CertificateInfo>> {
    let mut certificates = Vec::new();

    if !cache_dir.exists() {
        return Ok(certificates);
    }

    // Read directory and find certificate files
    for entry in std::fs::read_dir(cache_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Look for files ending with -cert.pem
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if filename.ends_with("-cert.pem") {
                // Extract domain from filename (e.g., "example.com-cert.pem" -> "example.com")
                let domain = filename
                    .trim_end_matches("-cert.pem")
                    .replace("wildcard", "*");

                let key_path = path.with_file_name(format!(
                    "{}-key.pem",
                    filename.trim_end_matches("-cert.pem")
                ));

                if let Ok(cert_info) = parse_certificate_file(&domain, &path, &key_path) {
                    certificates.push(cert_info);
                }
            }
        }
    }

    // Sort by domain name
    certificates.sort_by(|a, b| a.domain.cmp(&b.domain));

    Ok(certificates)
}

/// Stub implementation for obtaining a certificate
async fn obtain_certificate_stub(
    le_config: &LetsEncryptConfig,
    domain: &str,
) -> rcommerce_core::Result<CertificateInfo> {
    use tracing::warn;

    // Return a stub certificate info
    // In production, this would use ACME for real certificate management
    let issued_at = chrono::Utc::now();
    let expires_at = issued_at + chrono::Duration::days(90);

    let cert_path = get_cert_path(&le_config.cache_dir, domain);
    let key_path = get_key_path(&le_config.cache_dir, domain);

    let cert_info = CertificateInfo {
        domain: domain.to_string(),
        certificate_path: cert_path,
        private_key_path: key_path,
        expires_at,
        issued_at,
        serial_number: format!("stub-{}", uuid::Uuid::new_v4()),
    };

    warn!(
        "Using stub certificate for {} - NOT SUITABLE FOR PRODUCTION",
        domain
    );
    Ok(cert_info)
}

// Email testing functions

use rcommerce_core::notification::email_templates::{Address, OrderConfirmationParams, OrderItem};
use rcommerce_core::notification::{
    EmailNotificationFactory, Notification, NotificationTemplate, TemplateVariables,
};
use std::fs::File;
use std::io::Write;

/// Generate a test order confirmation email
fn generate_order_confirmation_email(recipient: &str) -> rcommerce_core::Result<Notification> {
    let items = vec![
        OrderItem {
            name: "High-Performance Server Blade".to_string(),
            sku: "SRV-BLD-01".to_string(),
            quantity: 2,
            price: "3,200.00".to_string(),
        },
        OrderItem {
            name: "Enterprise Rust Support".to_string(),
            sku: "LIC-ENT-YR".to_string(),
            quantity: 1,
            price: "850.00".to_string(),
        },
    ];

    let shipping = Address {
        name: "John Doe".to_string(),
        street: "123 Main Street, Suite 100".to_string(),
        city: "San Francisco".to_string(),
        state: "CA".to_string(),
        zip: "94102".to_string(),
        country: "United States".to_string(),
    };

    let billing = Address {
        name: "John Doe".to_string(),
        street: "456 Business Ave".to_string(),
        city: "New York".to_string(),
        state: "NY".to_string(),
        zip: "10001".to_string(),
        country: "United States".to_string(),
    };

    EmailNotificationFactory::order_confirmation(OrderConfirmationParams {
        recipient_email: recipient,
        customer_name: "John Doe",
        order_number: "ORD-2026-001234",
        order_date: "Feb 5, 2026",
        order_total: "7,250.00",
        items: &items,
        shipping_address: &shipping,
        billing_address: &billing,
    })
}

/// Generate a test order shipped email
fn generate_order_shipped_email(recipient: &str) -> rcommerce_core::Result<Notification> {
    let template = NotificationTemplate::load("order_shipped_html")?;

    let mut vars = TemplateVariables::new();
    vars.insert("customer_name", "John Doe");
    vars.insert("order_number", "ORD-2026-001234");
    vars.insert("order_date", "Feb 5, 2026");
    vars.insert("tracking_number", "1Z999AA10123456784");
    vars.insert(
        "tracking_url",
        "https://tracking.example.com/1Z999AA10123456784",
    );
    vars.insert("shipping_carrier", "UPS");
    vars.insert("estimated_delivery", "Feb 8, 2026");
    vars.insert("company_name", "R Commerce");
    vars.insert("support_email", "support@rcommerce.local");

    create_notification_from_template(recipient, &template, vars)
}

/// Generate a test payment successful email
fn generate_payment_successful_email(recipient: &str) -> rcommerce_core::Result<Notification> {
    let template = NotificationTemplate::load("payment_successful_html")?;

    let mut vars = TemplateVariables::new();
    vars.insert("customer_name", "John Doe");
    vars.insert("order_number", "ORD-2026-001234");
    vars.insert("amount", "$7,250.00");
    vars.insert("payment_method", "Visa ending in 4242");
    vars.insert("payment_date", "Feb 5, 2026");
    vars.insert("company_name", "R Commerce");
    vars.insert("support_email", "support@rcommerce.local");

    create_notification_from_template(recipient, &template, vars)
}

/// Generate a test payment failed email
fn generate_payment_failed_email(recipient: &str) -> rcommerce_core::Result<Notification> {
    let template = NotificationTemplate::load("payment_failed_html")?;

    let mut vars = TemplateVariables::new();
    vars.insert("customer_name", "John Doe");
    vars.insert("order_number", "ORD-2026-001234");
    vars.insert("amount", "$7,250.00");
    vars.insert(
        "error_message",
        "Your card was declined. Please try a different payment method.",
    );
    vars.insert(
        "retry_url",
        "https://rcommerce.local/payment/retry/ORD-2026-001234",
    );
    vars.insert("company_name", "R Commerce");
    vars.insert("support_email", "support@rcommerce.local");

    create_notification_from_template(recipient, &template, vars)
}

/// Generate a test subscription created email
fn generate_subscription_created_email(recipient: &str) -> rcommerce_core::Result<Notification> {
    let template = NotificationTemplate::load("subscription_created_html")?;

    let mut vars = TemplateVariables::new();
    vars.insert("customer_name", "John Doe");
    vars.insert("subscription_id", "SUB-2026-001");
    vars.insert("plan_name", "Enterprise Plan");
    vars.insert("amount", "$299.00");
    vars.insert("interval", "Monthly");
    vars.insert("next_billing_date", "Mar 5, 2026");
    vars.insert("trial_end_date", "Feb 12, 2026");
    vars.insert("company_name", "R Commerce");
    vars.insert("support_email", "support@rcommerce.local");

    create_notification_from_template(recipient, &template, vars)
}

/// Generate a test subscription renewal email
fn generate_subscription_renewal_email(recipient: &str) -> rcommerce_core::Result<Notification> {
    let template = NotificationTemplate::load("subscription_renewal_html")?;

    let mut vars = TemplateVariables::new();
    vars.insert("customer_name", "John Doe");
    vars.insert("plan_name", "Enterprise Plan");
    vars.insert("amount", "$299.00");
    vars.insert("billing_date", "Feb 5, 2026");
    vars.insert("next_billing_date", "Mar 5, 2026");
    vars.insert(
        "invoice_url",
        "https://rcommerce.local/invoices/INV-2026-001",
    );
    vars.insert("company_name", "R Commerce");
    vars.insert("support_email", "support@rcommerce.local");

    create_notification_from_template(recipient, &template, vars)
}

/// Generate a test subscription cancelled email
fn generate_subscription_cancelled_email(recipient: &str) -> rcommerce_core::Result<Notification> {
    let template = NotificationTemplate::load("subscription_cancelled_html")?;

    let mut vars = TemplateVariables::new();
    vars.insert("customer_name", "John Doe");
    vars.insert("plan_name", "Enterprise Plan");
    vars.insert("cancellation_date", "Feb 5, 2026");
    vars.insert("end_date", "Mar 5, 2026");
    vars.insert("access_until", "Mar 5, 2026");
    vars.insert("reason", "Customer requested cancellation");
    vars.insert("company_name", "R Commerce");
    vars.insert("support_email", "support@rcommerce.local");

    create_notification_from_template(recipient, &template, vars)
}

/// Generate a test dunning first email
fn generate_dunning_first_email(recipient: &str) -> rcommerce_core::Result<Notification> {
    let template = NotificationTemplate::load("dunning_first_html")?;

    let mut vars = TemplateVariables::new();
    vars.insert("customer_name", "John Doe");
    vars.insert("order_number", "ORD-2026-001234");
    vars.insert("amount", "$299.00");
    vars.insert("payment_method", "Visa ending in 4242");
    vars.insert("error_message", "Your card was declined.");
    vars.insert("retry_url", "https://rcommerce.local/payment/update");
    vars.insert("company_name", "R Commerce");
    vars.insert("support_email", "support@rcommerce.local");

    create_notification_from_template(recipient, &template, vars)
}

/// Generate a test dunning retry email
fn generate_dunning_retry_email(recipient: &str) -> rcommerce_core::Result<Notification> {
    let template = NotificationTemplate::load("dunning_retry_html")?;

    let mut vars = TemplateVariables::new();
    vars.insert("customer_name", "John Doe");
    vars.insert("order_number", "ORD-2026-001234");
    vars.insert("amount", "$299.00");
    vars.insert("attempt_number", "2");
    vars.insert("max_attempts", "4");
    vars.insert("next_retry_date", "Feb 7, 2026");
    vars.insert(
        "update_payment_url",
        "https://rcommerce.local/payment/update",
    );
    vars.insert("company_name", "R Commerce");
    vars.insert("support_email", "support@rcommerce.local");

    create_notification_from_template(recipient, &template, vars)
}

/// Generate a test dunning final email
fn generate_dunning_final_email(recipient: &str) -> rcommerce_core::Result<Notification> {
    let template = NotificationTemplate::load("dunning_final_html")?;

    let mut vars = TemplateVariables::new();
    vars.insert("customer_name", "John Doe");
    vars.insert("order_number", "ORD-2026-001234");
    vars.insert("amount", "$299.00");
    vars.insert("final_date", "Feb 10, 2026");
    vars.insert("cancellation_date", "Feb 12, 2026");
    vars.insert(
        "update_payment_url",
        "https://rcommerce.local/payment/update",
    );
    vars.insert("contact_support_url", "https://rcommerce.local/support");
    vars.insert("company_name", "R Commerce");
    vars.insert("support_email", "support@rcommerce.local");

    create_notification_from_template(recipient, &template, vars)
}

/// Generate a test welcome email
fn generate_welcome_email(recipient: &str) -> rcommerce_core::Result<Notification> {
    let template = NotificationTemplate::load("welcome_html")?;

    let mut vars = TemplateVariables::new();
    vars.insert("customer_name", "John Doe");
    vars.insert("company_name", "R Commerce");
    vars.insert("login_url", "https://rcommerce.local/login");
    vars.insert("shop_url", "https://rcommerce.local/shop");
    vars.insert("support_email", "support@rcommerce.local");
    vars.insert("help_center_url", "https://rcommerce.local/help");

    create_notification_from_template(recipient, &template, vars)
}

/// Generate a test password reset email
fn generate_password_reset_email(recipient: &str) -> rcommerce_core::Result<Notification> {
    let template = NotificationTemplate::load("password_reset_html")?;

    let mut vars = TemplateVariables::new();
    vars.insert("customer_name", "John Doe");
    vars.insert(
        "reset_url",
        "https://rcommerce.local/reset-password?token=abc123xyz",
    );
    vars.insert("reset_token", "ABC123XYZ789");
    vars.insert("expires_in", "24 hours");
    vars.insert("company_name", "R Commerce");
    vars.insert("support_email", "support@rcommerce.local");

    create_notification_from_template(recipient, &template, vars)
}

/// Generate a test abandoned cart email
fn generate_abandoned_cart_email(recipient: &str) -> rcommerce_core::Result<Notification> {
    let template = NotificationTemplate::load("abandoned_cart_html")?;

    let mut vars = TemplateVariables::new();
    vars.insert("customer_name", "John Doe");
    vars.insert(
        "cart_items",
        "High-Performance Server Blade (x2), Enterprise Rust Support (x1)",
    );
    vars.insert("cart_total", "$7,250.00");
    vars.insert("cart_url", "https://rcommerce.local/cart");
    vars.insert("discount_code", "COMEBACK10");
    vars.insert("company_name", "R Commerce");
    vars.insert("support_email", "support@rcommerce.local");

    create_notification_from_template(recipient, &template, vars)
}

/// Generate a test order cancelled email
fn generate_order_cancelled_email(recipient: &str) -> rcommerce_core::Result<Notification> {
    let template = NotificationTemplate::load("order_cancelled_html")?;

    let mut vars = TemplateVariables::new();
    vars.insert("customer_name", "John Doe");
    vars.insert("order_number", "ORD-2026-001234");
    vars.insert("order_date", "Feb 5, 2026");
    vars.insert("cancellation_reason", "Customer requested cancellation");
    vars.insert("refund_amount", "$7,250.00");
    vars.insert("company_name", "R Commerce");
    vars.insert("support_email", "support@rcommerce.local");

    create_notification_from_template(recipient, &template, vars)
}

/// Generate a test refund processed email
fn generate_refund_processed_email(recipient: &str) -> rcommerce_core::Result<Notification> {
    let template = NotificationTemplate::load("refund_processed_html")?;

    let mut vars = TemplateVariables::new();
    vars.insert("customer_name", "John Doe");
    vars.insert("order_number", "ORD-2026-001234");
    vars.insert("refund_amount", "$7,250.00");
    vars.insert(
        "refund_method",
        "Original payment method (Visa ending in 4242)",
    );
    vars.insert("processing_time", "5-7 business days");
    vars.insert("company_name", "R Commerce");
    vars.insert("support_email", "support@rcommerce.local");

    create_notification_from_template(recipient, &template, vars)
}

/// Helper function to create a notification from a template
fn create_notification_from_template(
    recipient: &str,
    template: &NotificationTemplate,
    variables: TemplateVariables,
) -> rcommerce_core::Result<Notification> {
    use rcommerce_core::notification::types::{DeliveryStatus, NotificationPriority};

    let subject = template.render_subject(&variables)?;
    let body = template.render(&variables)?;
    let html_body = template.render_html(&variables)?;

    Ok(Notification {
        id: uuid::Uuid::new_v4(),
        channel: rcommerce_core::notification::NotificationChannel::Email,
        recipient: recipient.to_string(),
        subject,
        body,
        html_body,
        priority: NotificationPriority::Normal,
        status: DeliveryStatus::Pending,
        attempt_count: 0,
        max_attempts: 3,
        error_message: None,
        metadata: serde_json::json!({"template_id": template.id}),
        scheduled_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    })
}

/// Test all email templates and save to filesystem
async fn test_all_email_templates(
    output_dir: &str,
    recipient: &str,
) -> rcommerce_core::Result<usize> {
    use std::fs;

    // Create output directory
    fs::create_dir_all(output_dir).map_err(|e| {
        rcommerce_core::Error::config(format!("Failed to create output directory: {}", e))
    })?;

    /// Type alias for email generator function
    type EmailGenerator = Box<dyn Fn(&str) -> rcommerce_core::Result<Notification>>;

    let generators: Vec<(&str, EmailGenerator)> = vec![
        (
            "order_confirmation",
            Box::new(generate_order_confirmation_email),
        ),
        ("order_shipped", Box::new(generate_order_shipped_email)),
        ("order_cancelled", Box::new(generate_order_cancelled_email)),
        (
            "payment_successful",
            Box::new(generate_payment_successful_email),
        ),
        ("payment_failed", Box::new(generate_payment_failed_email)),
        (
            "refund_processed",
            Box::new(generate_refund_processed_email),
        ),
        (
            "subscription_created",
            Box::new(generate_subscription_created_email),
        ),
        (
            "subscription_renewal",
            Box::new(generate_subscription_renewal_email),
        ),
        (
            "subscription_cancelled",
            Box::new(generate_subscription_cancelled_email),
        ),
        ("dunning_first", Box::new(generate_dunning_first_email)),
        ("dunning_retry", Box::new(generate_dunning_retry_email)),
        ("dunning_final", Box::new(generate_dunning_final_email)),
        ("welcome", Box::new(generate_welcome_email)),
        ("password_reset", Box::new(generate_password_reset_email)),
        ("abandoned_cart", Box::new(generate_abandoned_cart_email)),
    ];

    let mut count = 0;
    for (name, generator) in generators {
        match generator(recipient) {
            Ok(notification) => {
                let filename = format!(
                    "{}_{}.html",
                    name,
                    chrono::Utc::now().format("%Y%m%d_%H%M%S")
                );
                let filepath = format!("{}/{}", output_dir, filename);

                // Save email to file
                let mut content = format!("<!-- Subject: {} -->\n", notification.subject);
                content.push_str(&format!("<!-- To: {} -->\n", notification.recipient));
                content.push_str(&format!("<!-- Template: {} -->\n\n", name));

                if let Some(ref html) = notification.html_body {
                    content.push_str(html);
                } else {
                    content.push_str(&notification.body);
                }

                let mut file = File::create(&filepath).map_err(|e| {
                    rcommerce_core::Error::config(format!("Failed to create file: {}", e))
                })?;
                file.write_all(content.as_bytes()).map_err(|e| {
                    rcommerce_core::Error::config(format!("Failed to write file: {}", e))
                })?;

                println!("  ✓ Generated: {} -> {}", name.cyan(), filepath.dimmed());
                count += 1;
            }
            Err(e) => {
                eprintln!("  ✗ Failed to generate {}: {}", name.red(), e);
            }
        }
    }

    Ok(count)
}

/// Test a specific email template
async fn test_email_template(
    template: &str,
    output_dir: &str,
    recipient: &str,
) -> rcommerce_core::Result<String> {
    use std::fs;

    // Create output directory
    fs::create_dir_all(output_dir).map_err(|e| {
        rcommerce_core::Error::config(format!("Failed to create output directory: {}", e))
    })?;

    let notification = match template {
        "order_confirmation" => generate_order_confirmation_email(recipient),
        "order_shipped" => generate_order_shipped_email(recipient),
        "order_cancelled" => generate_order_cancelled_email(recipient),
        "payment_successful" => generate_payment_successful_email(recipient),
        "payment_failed" => generate_payment_failed_email(recipient),
        "refund_processed" => generate_refund_processed_email(recipient),
        "subscription_created" => generate_subscription_created_email(recipient),
        "subscription_renewal" => generate_subscription_renewal_email(recipient),
        "subscription_cancelled" => generate_subscription_cancelled_email(recipient),
        "dunning_first" => generate_dunning_first_email(recipient),
        "dunning_retry" => generate_dunning_retry_email(recipient),
        "dunning_final" => generate_dunning_final_email(recipient),
        "welcome" => generate_welcome_email(recipient),
        "password_reset" => generate_password_reset_email(recipient),
        "abandoned_cart" => generate_abandoned_cart_email(recipient),
        _ => {
            return Err(rcommerce_core::Error::validation(format!(
                "Unknown template: {}",
                template
            )))
        }
    }?;

    let filename = format!(
        "{}_{}.html",
        template,
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );
    let filepath = format!("{}/{}", output_dir, filename);

    // Save email to file
    let mut content = format!("<!-- Subject: {} -->\n", notification.subject);
    content.push_str(&format!("<!-- To: {} -->\n", notification.recipient));
    content.push_str(&format!("<!-- Template: {} -->\n\n", template));

    if let Some(ref html) = notification.html_body {
        content.push_str(html);
    } else {
        content.push_str(&notification.body);
    }

    let mut file = File::create(&filepath)
        .map_err(|e| rcommerce_core::Error::config(format!("Failed to create file: {}", e)))?;
    file.write_all(content.as_bytes())
        .map_err(|e| rcommerce_core::Error::config(format!("Failed to write file: {}", e)))?;

    Ok(filepath)
}

/// Send a mock email (outputs to console)
async fn send_mock_email(template: &str, recipient: &str) -> rcommerce_core::Result<()> {
    let notification = match template {
        "order_confirmation" => generate_order_confirmation_email(recipient),
        "order_shipped" => generate_order_shipped_email(recipient),
        "order_cancelled" => generate_order_cancelled_email(recipient),
        "payment_successful" => generate_payment_successful_email(recipient),
        "payment_failed" => generate_payment_failed_email(recipient),
        "refund_processed" => generate_refund_processed_email(recipient),
        "subscription_created" => generate_subscription_created_email(recipient),
        "subscription_renewal" => generate_subscription_renewal_email(recipient),
        "subscription_cancelled" => generate_subscription_cancelled_email(recipient),
        "dunning_first" => generate_dunning_first_email(recipient),
        "dunning_retry" => generate_dunning_retry_email(recipient),
        "dunning_final" => generate_dunning_final_email(recipient),
        "welcome" => generate_welcome_email(recipient),
        "password_reset" => generate_password_reset_email(recipient),
        "abandoned_cart" => generate_abandoned_cart_email(recipient),
        _ => {
            return Err(rcommerce_core::Error::validation(format!(
                "Unknown template: {}",
                template
            )))
        }
    }?;

    // Output to console in mock format
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                     MOCK EMAIL SENT                          ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║ To:      {:<50} ║", notification.recipient);
    println!("║ Subject: {:<50} ║", notification.subject);
    println!("╠══════════════════════════════════════════════════════════════╣");

    for line in notification.body.lines() {
        if line.len() > 60 {
            println!("║ {:<60} ║", &line[..60]);
        } else {
            println!("║ {:<60} ║", line);
        }
    }

    if let Some(ref html) = notification.html_body {
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║ HTML Body: {:<48} ║", format!("{} bytes", html.len()));
    }

    println!("╚══════════════════════════════════════════════════════════════╝");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse() {
        let cli = Cli::parse_from(&["rcommerce", "server"]);
        assert!(matches!(cli.command, Commands::Server { .. }));
    }

    #[test]
    fn test_tls_commands_parse() {
        // Test check command
        let cli = Cli::parse_from(&["rcommerce", "tls", "check", "--domain", "example.com"]);
        assert!(matches!(
            cli.command,
            Commands::Tls {
                command: TlsCommands::Check { .. }
            }
        ));

        // Test list command
        let cli = Cli::parse_from(&["rcommerce", "tls", "list"]);
        assert!(matches!(
            cli.command,
            Commands::Tls {
                command: TlsCommands::List
            }
        ));

        // Test renew command
        let cli = Cli::parse_from(&["rcommerce", "tls", "renew", "--domain", "example.com"]);
        assert!(matches!(
            cli.command,
            Commands::Tls {
                command: TlsCommands::Renew { .. }
            }
        ));

        // Test info command
        let cli = Cli::parse_from(&["rcommerce", "tls", "info", "--domain", "example.com"]);
        assert!(matches!(
            cli.command,
            Commands::Tls {
                command: TlsCommands::Info { .. }
            }
        ));
    }
}

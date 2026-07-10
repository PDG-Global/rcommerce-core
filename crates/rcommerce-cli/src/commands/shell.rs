//! Interactive Shell for R Commerce
//!
//! Provides a command-line interface for managing products, orders, customers,
//! and other e-commerce operations through an interactive REPL.

use colored::Colorize;
use dialoguer::{Confirm, Input, Select};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::io::{self, Write};
use uuid::Uuid;

use rcommerce_core::models::{Currency, ProductType};
use rcommerce_core::Result;

/// Shell command parser result
#[derive(Debug, Clone)]
pub enum ShellCommand {
    /// Show help
    Help,
    /// List entities
    List {
        entity: String,
        limit: Option<usize>,
    },
    /// Get entity details
    Get { entity: String, id: String },
    /// Create new entity
    Create { entity: String },
    /// Update entity
    Update { entity: String, id: String },
    /// Delete entity
    Delete { entity: String, id: String },
    /// Search entities
    Search { entity: String, query: String },
    /// Show dashboard/overview
    Dashboard,
    /// Show database status
    Status,
    /// Clear screen
    Clear,
    /// Exit shell
    Exit,
    /// Empty command
    Empty,
    /// Unknown command
    Unknown(String),
}

/// Main shell state
pub struct Shell {
    pool: PgPool,
    running: bool,
    prompt: String,
}

impl Shell {
    /// Create a new shell instance
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            running: true,
            prompt: "rcommerce> ".to_string(),
        }
    }

    /// Run the interactive shell
    pub async fn run(&mut self) -> Result<()> {
        self.print_welcome();

        while self.running {
            print!("{}", self.prompt.bright_cyan());
            io::stdout().flush().unwrap();

            let mut input = String::new();
            match io::stdin().read_line(&mut input) {
                Ok(_) => {
                    let command = self.parse_command(&input);
                    if let Err(e) = self.execute_command(command).await {
                        eprintln!("{} {}", "Error:".red().bold(), e);
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Input error:".red(), e);
                }
            }
        }

        println!("\n{} Goodbye! 👋\n", "Bye:".green().bold());
        Ok(())
    }

    /// Parse input into a shell command
    fn parse_command(&self, input: &str) -> ShellCommand {
        let trimmed = input.trim();

        if trimmed.is_empty() {
            return ShellCommand::Empty;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        let cmd = parts[0].to_lowercase();

        match cmd.as_str() {
            "help" | "h" | "?" => ShellCommand::Help,
            "quit" | "exit" | "q" => ShellCommand::Exit,
            "clear" | "cls" => ShellCommand::Clear,
            "dashboard" | "dash" | "d" => ShellCommand::Dashboard,
            "status" | "st" => ShellCommand::Status,

            "list" | "ls" => {
                if parts.len() < 2 {
                    ShellCommand::Unknown(
                        "Usage: list <products|orders|customers|api-keys> [limit]".to_string(),
                    )
                } else {
                    let entity = parts[1].to_string();
                    let limit = parts.get(2).and_then(|s| s.parse().ok());
                    ShellCommand::List { entity, limit }
                }
            }

            "get" | "show" | "view" => {
                if parts.len() < 3 {
                    ShellCommand::Unknown("Usage: get <entity> <id>".to_string())
                } else {
                    ShellCommand::Get {
                        entity: parts[1].to_string(),
                        id: parts[2].to_string(),
                    }
                }
            }

            "create" | "new" | "add" => {
                if parts.len() < 2 {
                    ShellCommand::Unknown("Usage: create <product|customer|order>".to_string())
                } else {
                    ShellCommand::Create {
                        entity: parts[1].to_string(),
                    }
                }
            }

            "update" | "edit" => {
                if parts.len() < 3 {
                    ShellCommand::Unknown("Usage: update <entity> <id>".to_string())
                } else {
                    ShellCommand::Update {
                        entity: parts[1].to_string(),
                        id: parts[2].to_string(),
                    }
                }
            }

            "delete" | "del" | "rm" => {
                if parts.len() < 3 {
                    ShellCommand::Unknown("Usage: delete <entity> <id>".to_string())
                } else {
                    ShellCommand::Delete {
                        entity: parts[1].to_string(),
                        id: parts[2].to_string(),
                    }
                }
            }

            "search" | "find" | "s" => {
                if parts.len() < 3 {
                    ShellCommand::Unknown("Usage: search <entity> <query>".to_string())
                } else {
                    let query = parts[2..].join(" ");
                    ShellCommand::Search {
                        entity: parts[1].to_string(),
                        query,
                    }
                }
            }

            _ => ShellCommand::Unknown(format!(
                "Unknown command: '{}' (type 'help' for available commands)",
                cmd
            )),
        }
    }

    /// Execute a parsed command
    async fn execute_command(&mut self, command: ShellCommand) -> Result<()> {
        match command {
            ShellCommand::Empty => {}
            ShellCommand::Help => self.print_help(),
            ShellCommand::Exit => self.running = false,
            ShellCommand::Clear => self.clear_screen(),
            ShellCommand::Dashboard => self.show_dashboard().await?,
            ShellCommand::Status => self.show_status().await?,

            ShellCommand::List { entity, limit } => match entity.as_str() {
                "products" | "product" | "p" => self.list_products(limit).await?,
                "orders" | "order" | "o" => self.list_orders(limit).await?,
                "customers" | "customer" | "c" => self.list_customers(limit).await?,
                "api-keys" | "apikeys" | "keys" | "k" => self.list_api_keys(limit).await?,
                _ => println!(
                    "{} Unknown entity '{}'. Use: products, orders, customers, api-keys",
                    "Error:".red(),
                    entity
                ),
            },

            ShellCommand::Get { entity, id } => match entity.as_str() {
                "product" | "p" => self.get_product(&id).await?,
                "order" | "o" => self.get_order(&id).await?,
                "customer" | "c" => self.get_customer(&id).await?,
                "api-key" | "apikey" | "key" | "k" => self.get_api_key(&id).await?,
                _ => println!(
                    "{} Unknown entity '{}'. Use: product, order, customer, api-key",
                    "Error:".red(),
                    entity
                ),
            },

            ShellCommand::Create { entity } => match entity.as_str() {
                "product" | "p" => self.create_product_interactive().await?,
                "customer" | "c" => self.create_customer_interactive().await?,
                _ => println!(
                    "{} Interactive creation not yet supported for '{}'",
                    "Info:".yellow(),
                    entity
                ),
            },

            ShellCommand::Delete { entity, id } => match entity.as_str() {
                "product" | "p" => self.delete_product(&id).await?,
                "customer" | "c" => self.delete_customer(&id).await?,
                "api-key" | "apikey" | "key" | "k" => self.delete_api_key(&id).await?,
                _ => println!(
                    "{} Delete not yet supported for '{}'",
                    "Info:".yellow(),
                    entity
                ),
            },

            ShellCommand::Search { entity, query } => match entity.as_str() {
                "products" | "product" | "p" => self.search_products(&query).await?,
                "customers" | "customer" | "c" => self.search_customers(&query).await?,
                "orders" | "order" | "o" => self.search_orders(&query).await?,
                _ => println!(
                    "{} Search not yet supported for '{}'",
                    "Info:".yellow(),
                    entity
                ),
            },

            ShellCommand::Unknown(msg) => println!("{} {}", "Error:".red(), msg),
            ShellCommand::Update { entity, id } => {
                println!(
                    "{} Update for {} {} - coming soon!",
                    "Info:".yellow(),
                    entity,
                    id
                );
            }
        }

        Ok(())
    }

    /// Print welcome message
    fn print_welcome(&self) {
        println!(
            "\n{}",
            "╔═══════════════════════════════════════════════════════════════╗".bright_cyan()
        );
        println!(
            "{}",
            "║                                                               ║".bright_cyan()
        );
        println!(
            "{}",
            "║           🛒 R Commerce Interactive Shell                     ║"
                .bright_cyan()
                .bold()
        );
        println!(
            "{}",
            "║                                                               ║".bright_cyan()
        );
        println!(
            "{}",
            "║     Type 'help' for available commands or 'exit' to quit      ║".bright_cyan()
        );
        println!(
            "{}",
            "║                                                               ║".bright_cyan()
        );
        println!(
            "{}",
            "╚═══════════════════════════════════════════════════════════════╝".bright_cyan()
        );
        println!();
    }

    /// Print help information
    fn print_help(&self) {
        println!("\n{}", "Available Commands:".bold().underline());
        println!();

        println!("{}", "Navigation:".cyan().bold());
        println!("  {:<25} Show this help message", "help, h, ?");
        println!("  {:<25} Exit the shell", "exit, quit, q");
        println!("  {:<25} Clear the screen", "clear, cls");
        println!("  {:<25} Show dashboard overview", "dashboard, dash, d");
        println!("  {:<25} Show database status", "status, st");
        println!();

        println!("{}", "Entity Management:".cyan().bold());
        println!(
            "  {:<25} List entities (products, orders, customers, api-keys)",
            "list <entity> [limit]"
        );
        println!(
            "  {:<25} Get details of a specific entity",
            "get <entity> <id>"
        );
        println!(
            "  {:<25} Create a new entity interactively",
            "create <entity>"
        );
        println!(
            "  {:<25} Update an entity (coming soon)",
            "update <entity> <id>"
        );
        println!("  {:<25} Delete an entity", "delete <entity> <id>");
        println!("  {:<25} Search for entities", "search <entity> <query>");
        println!();

        println!("{}", "Entity Shortcuts:".cyan().bold());
        println!("  {:<15} → product(s)", "p");
        println!("  {:<15} → order(s)", "o");
        println!("  {:<15} → customer(s)", "c");
        println!("  {:<15} → api-keys", "k, keys");
        println!();

        println!("{}", "Examples:".cyan().bold());
        println!("  list products 10");
        println!("  get product abc-123");
        println!("  create customer");
        println!("  search products laptop");
        println!("  delete api-key ak_1234");
        println!();
    }

    /// Clear the terminal screen
    fn clear_screen(&self) {
        print!("\x1B[2J\x1B[1;1H");
        io::stdout().flush().unwrap();
    }

    /// Show dashboard with key metrics
    async fn show_dashboard(&self) -> Result<()> {
        println!("\n{}", "📊 Dashboard".bold().underline());
        println!();

        // Get counts from database
        let product_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
            .fetch_one(&self.pool)
            .await?;

        let order_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orders")
            .fetch_one(&self.pool)
            .await?;

        let customer_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM customers")
            .fetch_one(&self.pool)
            .await?;

        let total_revenue: Option<Decimal> = sqlx::query_scalar(
            "SELECT SUM(total) FROM orders WHERE status NOT IN ('cancelled', 'refunded')",
        )
        .fetch_one(&self.pool)
        .await?;

        // Recent orders
        let recent_orders: Vec<(String, String, Decimal, String)> = sqlx::query_as(
            "SELECT o.id::text, c.email, o.total, o.status::text 
             FROM orders o 
             JOIN customers c ON o.customer_id = c.id 
             ORDER BY o.created_at DESC 
             LIMIT 5",
        )
        .fetch_all(&self.pool)
        .await?;

        // Print metrics
        println!("{}", "Key Metrics:".cyan().bold());
        println!(
            "  {:<20} {}",
            "Products:",
            product_count.to_string().bright_green()
        );
        println!(
            "  {:<20} {}",
            "Orders:",
            order_count.to_string().bright_green()
        );
        println!(
            "  {:<20} {}",
            "Customers:",
            customer_count.to_string().bright_green()
        );
        println!(
            "  {:<20} {}",
            "Total Revenue:",
            format!("${:.2}", total_revenue.unwrap_or(Decimal::ZERO)).bright_green()
        );
        println!();

        // Print recent orders
        if !recent_orders.is_empty() {
            println!("{}", "Recent Orders:".cyan().bold());
            println!(
                "  {:<36} {:<25} {:<12} {:<12}",
                "ID", "Customer", "Total", "Status"
            );
            println!("  {}", "-".repeat(90));
            for (id, email, total, status) in recent_orders {
                let status_colored = match status.as_str() {
                    "completed" => status.green(),
                    "pending" => status.yellow(),
                    "cancelled" | "refunded" => status.red(),
                    _ => status.normal(),
                };
                println!(
                    "  {:<36} {:<25} ${:<11.2} {}",
                    id,
                    truncate(&email, 23),
                    total,
                    status_colored
                );
            }
            println!();
        }

        Ok(())
    }

    /// Show database status
    async fn show_status(&self) -> Result<()> {
        println!("\n{}", "Database Status".bold().underline());
        println!();

        let product_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
            .fetch_one(&self.pool)
            .await?;

        let active_products: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE is_active = true")
                .fetch_one(&self.pool)
                .await?;

        let order_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orders")
            .fetch_one(&self.pool)
            .await?;

        let pending_orders: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE status = 'pending'")
                .fetch_one(&self.pool)
                .await?;

        let customer_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM customers")
            .fetch_one(&self.pool)
            .await?;

        let api_key_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_keys")
            .fetch_one(&self.pool)
            .await?;

        println!("  {:<25} {}", "Total Products:", product_count);
        println!(
            "  {:<25} {}",
            "Active Products:",
            active_products.to_string().green()
        );
        println!("  {:<25} {}", "Total Orders:", order_count);
        println!(
            "  {:<25} {}",
            "Pending Orders:",
            pending_orders.to_string().yellow()
        );
        println!("  {:<25} {}", "Total Customers:", customer_count);
        println!("  {:<25} {}", "API Keys:", api_key_count);
        println!();

        Ok(())
    }

    /// List products
    async fn list_products(&self, limit: Option<usize>) -> Result<()> {
        let limit = limit.unwrap_or(20);

        let products: Vec<ProductRecord> = sqlx::query_as(
            "SELECT id, title, slug, price, currency::text, description, is_active, inventory_quantity, created_at 
             FROM products 
             ORDER BY created_at DESC 
             LIMIT $1"
        )
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;

        if products.is_empty() {
            println!("{} No products found", "Info:".yellow());
        } else {
            println!(
                "\n{}",
                format!("Products (showing {})", products.len())
                    .bold()
                    .underline()
            );
            println!(
                "  {:<36} {:<28} {:<10} {:<8} {:<12}",
                "ID", "Title", "Price", "Currency", "Status"
            );
            println!("  {}", "-".repeat(100));

            for p in &products {
                let status = if p.is_active {
                    "✓ Active".green()
                } else {
                    "✗ Inactive".red()
                };
                println!(
                    "  {:<36} {:<28} {:<10.2} {:<8} {}",
                    p.id.to_string(),
                    truncate(&p.title, 26),
                    p.price,
                    p.currency,
                    status
                );
            }
            println!();
        }

        Ok(())
    }

    /// List orders
    async fn list_orders(&self, limit: Option<usize>) -> Result<()> {
        let limit = limit.unwrap_or(20);

        let orders: Vec<OrderRecord> = sqlx::query_as(
            "SELECT o.id, c.email as customer_email, o.status::text, o.total, o.created_at 
             FROM orders o 
             JOIN customers c ON o.customer_id = c.id 
             ORDER BY o.created_at DESC 
             LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        if orders.is_empty() {
            println!("{} No orders found", "Info:".yellow());
        } else {
            println!(
                "\n{}",
                format!("Orders (showing {})", orders.len())
                    .bold()
                    .underline()
            );
            println!(
                "  {:<36} {:<22} {:<12} {:<12} {:<12}",
                "ID", "Customer", "Status", "Total", "Created"
            );
            println!("  {}", "-".repeat(100));

            for o in &orders {
                let status_colored = match o.status.as_str() {
                    "completed" => o.status.green(),
                    "pending" => o.status.yellow(),
                    "cancelled" | "refunded" => o.status.red(),
                    _ => o.status.normal(),
                };
                println!(
                    "  {:<36} {:<22} {:<12} ${:<11.2} {}",
                    o.id.to_string(),
                    truncate(&o.customer_email, 20),
                    status_colored,
                    o.total,
                    o.created_at.format("%Y-%m-%d")
                );
            }
            println!();
        }

        Ok(())
    }

    /// List customers
    async fn list_customers(&self, limit: Option<usize>) -> Result<()> {
        let limit = limit.unwrap_or(20);

        let customers: Vec<CustomerRecord> = sqlx::query_as(
            "SELECT id, email, first_name, last_name, created_at 
             FROM customers 
             ORDER BY created_at DESC 
             LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        if customers.is_empty() {
            println!("{} No customers found", "Info:".yellow());
        } else {
            println!(
                "\n{}",
                format!("Customers (showing {})", customers.len())
                    .bold()
                    .underline()
            );
            println!(
                "  {:<36} {:<28} {:<22} {:<12}",
                "ID", "Email", "Name", "Created"
            );
            println!("  {}", "-".repeat(100));

            for c in &customers {
                let name = format!("{} {}", c.first_name, c.last_name);
                println!(
                    "  {:<36} {:<28} {:<22} {}",
                    c.id.to_string(),
                    truncate(&c.email, 26),
                    truncate(&name, 20),
                    c.created_at.format("%Y-%m-%d")
                );
            }
            println!();
        }

        Ok(())
    }

    /// List API keys
    async fn list_api_keys(&self, limit: Option<usize>) -> Result<()> {
        let limit = limit.unwrap_or(20);

        let keys: Vec<ApiKeyRecord> = sqlx::query_as(
            "SELECT key_prefix, name, scopes, is_active, expires_at, created_at 
             FROM api_keys 
             ORDER BY created_at DESC 
             LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        if keys.is_empty() {
            println!("{} No API keys found", "Info:".yellow());
        } else {
            println!(
                "\n{}",
                format!("API Keys (showing {})", keys.len())
                    .bold()
                    .underline()
            );
            println!(
                "  {:<14} {:<20} {:<30} {:<10} {:<12}",
                "Prefix", "Name", "Scopes", "Active", "Expires"
            );
            println!("  {}", "-".repeat(90));

            for k in &keys {
                let expires = k
                    .expires_at
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| "Never".to_string());
                println!(
                    "  {:<14} {:<20} {:<30} {:<10} {}",
                    k.key_prefix,
                    truncate(&k.name, 18),
                    truncate(&k.scopes.join(", "), 28),
                    if k.is_active {
                        "✓".green()
                    } else {
                        "✗".red()
                    },
                    expires
                );
            }
            println!();
        }

        Ok(())
    }

    /// Get product details
    async fn get_product(&self, id: &str) -> Result<()> {
        let product_id = parse_uuid(id)?;

        let product: Option<ProductRecord> = sqlx::query_as(
            "SELECT id, title, slug, price, currency::text, description, is_active, inventory_quantity, created_at 
             FROM products WHERE id = $1"
        )
            .bind(product_id)
            .fetch_optional(&self.pool)
            .await?;

        match product {
            Some(p) => {
                println!("\n{}", "Product Details".bold().underline());
                println!("  {:<20} {}", "ID:", p.id);
                println!("  {:<20} {}", "Title:", p.title);
                println!("  {:<20} {}", "Slug:", p.slug);
                println!("  {:<20} {} {}", "Price:", p.price, p.currency);
                println!(
                    "  {:<20} {}",
                    "Status:",
                    if p.is_active {
                        "✓ Active".green()
                    } else {
                        "✗ Inactive".red()
                    }
                );
                println!("  {:<20} {}", "Inventory:", p.inventory_quantity);
                println!(
                    "  {:<20} {}",
                    "Created:",
                    p.created_at.format("%Y-%m-%d %H:%M:%S UTC")
                );
                if let Some(desc) = p.description {
                    println!("  {:<20} {}", "Description:", desc);
                }
                println!();
            }
            None => println!("{} Product '{}' not found", "Error:".red(), id),
        }

        Ok(())
    }

    /// Get order details
    async fn get_order(&self, id: &str) -> Result<()> {
        let order_id = parse_uuid(id)?;

        let order: Option<OrderDetailRecord> = sqlx::query_as(
            "SELECT o.id, c.email as customer_email, o.status::text, o.total, 
                    o.subtotal, o.tax_amount, o.shipping_amount, o.discount_amount,
                    o.currency::text, o.notes, o.created_at, o.updated_at
             FROM orders o 
             JOIN customers c ON o.customer_id = c.id 
             WHERE o.id = $1",
        )
        .bind(order_id)
        .fetch_optional(&self.pool)
        .await?;

        match order {
            Some(o) => {
                println!("\n{}", "Order Details".bold().underline());
                println!("  {:<20} {}", "ID:", o.id);
                println!("  {:<20} {}", "Customer:", o.customer_email);
                println!("  {:<20} {}", "Status:", format_status(&o.status));
                println!("  {:<20} {} {}", "Total:", o.currency, o.total);
                println!("  {:<20} {} {}", "Subtotal:", o.currency, o.subtotal);
                println!("  {:<20} {} {}", "Tax:", o.currency, o.tax_amount);
                println!("  {:<20} {} {}", "Shipping:", o.currency, o.shipping_amount);
                println!("  {:<20} {} {}", "Discount:", o.currency, o.discount_amount);
                if let Some(notes) = o.notes {
                    println!("  {:<20} {}", "Notes:", notes);
                }
                println!(
                    "  {:<20} {}",
                    "Created:",
                    o.created_at.format("%Y-%m-%d %H:%M:%S UTC")
                );
                println!();

                // Get order items
                let items: Vec<OrderItemRecord> = sqlx::query_as(
                    "SELECT p.title, oi.quantity, oi.price, oi.total 
                     FROM order_items oi 
                     JOIN products p ON oi.product_id = p.id 
                     WHERE oi.order_id = $1",
                )
                .bind(order_id)
                .fetch_all(&self.pool)
                .await?;

                if !items.is_empty() {
                    println!("  {}", "Items:".cyan().bold());
                    println!(
                        "    {:<30} {:<10} {:<12} {:<12}",
                        "Product", "Qty", "Price", "Total"
                    );
                    for item in items {
                        println!(
                            "    {:<30} {:<10} ${:<11.2} ${:<11.2}",
                            truncate(&item.title, 28),
                            item.quantity,
                            item.price,
                            item.total
                        );
                    }
                    println!();
                }
            }
            None => println!("{} Order '{}' not found", "Error:".red(), id),
        }

        Ok(())
    }

    /// Get customer details
    async fn get_customer(&self, id: &str) -> Result<()> {
        let customer_id = parse_uuid(id)?;

        let customer: Option<CustomerDetailRecord> = sqlx::query_as(
            "SELECT id, email, first_name, last_name, phone, accepts_marketing, 
                    currency::text, is_verified, created_at, updated_at
             FROM customers WHERE id = $1",
        )
        .bind(customer_id)
        .fetch_optional(&self.pool)
        .await?;

        match customer {
            Some(c) => {
                println!("\n{}", "Customer Details".bold().underline());
                println!("  {:<20} {}", "ID:", c.id);
                println!("  {:<20} {}", "Email:", c.email);
                println!("  {:<20} {} {}", "Name:", c.first_name, c.last_name);
                if let Some(phone) = c.phone {
                    println!("  {:<20} {}", "Phone:", phone);
                }
                println!("  {:<20} {:?}", "Currency:", c.currency);
                println!(
                    "  {:<20} {}",
                    "Verified:",
                    if c.is_verified {
                        "✓ Yes".green()
                    } else {
                        "✗ No".red()
                    }
                );
                println!(
                    "  {:<20} {}",
                    "Marketing:",
                    if c.accepts_marketing {
                        "✓ Yes".green()
                    } else {
                        "✗ No".red()
                    }
                );
                println!(
                    "  {:<20} {}",
                    "Created:",
                    c.created_at.format("%Y-%m-%d %H:%M:%S UTC")
                );
                println!();

                // Get order count
                let order_count: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE customer_id = $1")
                        .bind(customer_id)
                        .fetch_one(&self.pool)
                        .await?;

                let total_spent: Option<Decimal> = sqlx::query_scalar(
                    "SELECT SUM(total) FROM orders WHERE customer_id = $1 AND status NOT IN ('cancelled', 'refunded')"
                )
                    .bind(customer_id)
                    .fetch_one(&self.pool)
                    .await?;

                println!("  {}", "Statistics:".cyan().bold());
                println!("    {:<18} {}", "Total Orders:", order_count);
                println!(
                    "    {:<18} ${:.2}",
                    "Total Spent:",
                    total_spent.unwrap_or(Decimal::ZERO)
                );
                println!();
            }
            None => println!("{} Customer '{}' not found", "Error:".red(), id),
        }

        Ok(())
    }

    /// Get API key details
    async fn get_api_key(&self, prefix: &str) -> Result<()> {
        let key: Option<ApiKeyDetailRecord> = sqlx::query_as(
            "SELECT id, key_prefix, name, scopes, is_active, customer_id, 
                    created_at, updated_at, expires_at, last_used_at, last_used_ip
             FROM api_keys WHERE key_prefix = $1",
        )
        .bind(prefix)
        .fetch_optional(&self.pool)
        .await?;

        match key {
            Some(k) => {
                println!("\n{}", "API Key Details".bold().underline());
                println!("  {:<20} {}", "ID:", k.id);
                println!("  {:<20} {}", "Prefix:", k.key_prefix);
                println!("  {:<20} {}", "Name:", k.name);
                println!("  {:<20} {}", "Scopes:", k.scopes.join(", "));
                println!(
                    "  {:<20} {}",
                    "Active:",
                    if k.is_active {
                        "✓ Yes".green()
                    } else {
                        "✗ No".red()
                    }
                );
                if let Some(cid) = k.customer_id {
                    println!("  {:<20} {}", "Customer ID:", cid);
                }
                println!(
                    "  {:<20} {}",
                    "Created:",
                    k.created_at.format("%Y-%m-%d %H:%M:%S UTC")
                );
                if let Some(expires) = k.expires_at {
                    println!(
                        "  {:<20} {}",
                        "Expires:",
                        expires.format("%Y-%m-%d %H:%M:%S UTC")
                    );
                }
                if let Some(last_used) = k.last_used_at {
                    println!(
                        "  {:<20} {}",
                        "Last Used:",
                        last_used.format("%Y-%m-%d %H:%M:%S UTC")
                    );
                }
                if let Some(last_ip) = k.last_used_ip {
                    println!("  {:<20} {}", "Last IP:", last_ip);
                }
                println!();
            }
            None => println!("{} API key '{}' not found", "Error:".red(), prefix),
        }

        Ok(())
    }

    /// Create product interactively
    async fn create_product_interactive(&self) -> Result<()> {
        println!("\n{}", "📦 Create New Product".bold().underline());

        // Product title
        let title: String = Input::new()
            .with_prompt("Product title")
            .validate_with(|input: &String| {
                if input.trim().is_empty() {
                    Err("Title is required")
                } else {
                    Ok(())
                }
            })
            .interact()
            .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

        // Auto-generate slug
        let default_slug = slugify(&title);
        let slug: String = Input::new()
            .with_prompt("URL slug")
            .default(default_slug)
            .interact()
            .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

        // Product type
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

        // Currency
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

        // SKU (optional)
        let sku_input: String = Input::new()
            .with_prompt("SKU (optional)")
            .allow_empty(true)
            .interact()
            .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;
        let sku = if sku_input.trim().is_empty() {
            None
        } else {
            Some(sku_input)
        };

        // Inventory
        let inventory_str: String = Input::new()
            .with_prompt("Inventory quantity")
            .default("0".to_string())
            .interact()
            .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;
        let inventory_quantity: i32 = inventory_str.parse().unwrap_or(0);

        // Description (optional)
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

        // Confirmation
        println!("\n{}", "Summary:".cyan().bold());
        println!("  Title:     {}", title);
        println!("  Slug:      {}", slug);
        println!("  Type:      {:?}", product_type);
        println!("  Price:     {} {}", price, currency);
        println!("  Inventory: {}", inventory_quantity);

        let confirmed = Confirm::new()
            .with_prompt("\nCreate this product?")
            .default(true)
            .interact()
            .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

        if !confirmed {
            println!("{} Product creation cancelled", "Info:".yellow());
            return Ok(());
        }

        // Insert into database
        let product: ProductRecord = sqlx::query_as(
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
            .bind(true)
            .bind(is_active)
            .bind(is_featured)
            .bind(!matches!(product_type, ProductType::Digital))
            .fetch_one(&self.pool)
            .await?;

        println!("\n{} Product created successfully!", "✓".green().bold());
        println!("  ID:    {}", product.id);
        println!("  Title: {}", product.title);
        println!();

        Ok(())
    }

    /// Create customer interactively
    async fn create_customer_interactive(&self) -> Result<()> {
        println!("\n{}", "👤 Create New Customer".bold().underline());

        // Email
        let email: String = Input::new()
            .with_prompt("Email address")
            .validate_with(|input: &String| {
                if input.trim().is_empty() {
                    Err("Email is required")
                } else if !input.contains('@') {
                    Err("Please enter a valid email")
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
                } else {
                    Ok(())
                }
            })
            .interact()
            .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

        // Phone (optional)
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

        // Currency
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
        let password = dialoguer::Password::new()
            .with_prompt("Password")
            .with_confirmation("Confirm password", "Passwords do not match")
            .validate_with(|input: &String| {
                if input.len() < 8 {
                    Err("Password must be at least 8 characters")
                } else {
                    Ok(())
                }
            })
            .interact()
            .map_err(|e| rcommerce_core::Error::validation(format!("Password error: {}", e)))?;

        // Confirmation
        println!("\n{}", "Summary:".cyan().bold());
        println!("  Name:   {} {}", first_name, last_name);
        println!("  Email:  {}", email);

        let confirmed = Confirm::new()
            .with_prompt("\nCreate this customer?")
            .default(true)
            .interact()
            .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

        if !confirmed {
            println!("{} Customer creation cancelled", "Info:".yellow());
            return Ok(());
        }

        // Hash password
        let password_hash = bcrypt::hash(&password, bcrypt::DEFAULT_COST).map_err(|e| {
            rcommerce_core::Error::validation(format!("Failed to hash password: {}", e))
        })?;

        // Insert into database
        let customer: CustomerRecord = sqlx::query_as(
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
        .bind(true)
        .fetch_one(&self.pool)
        .await?;

        println!("\n{} Customer created successfully!", "✓".green().bold());
        println!("  ID:    {}", customer.id);
        println!("  Name:  {} {}", customer.first_name, customer.last_name);
        println!();

        Ok(())
    }

    /// Delete a product
    async fn delete_product(&self, id: &str) -> Result<()> {
        let product_id = parse_uuid(id)?;

        // Check if product exists
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM products WHERE id = $1)")
                .bind(product_id)
                .fetch_one(&self.pool)
                .await?;

        if !exists {
            println!("{} Product '{}' not found", "Error:".red(), id);
            return Ok(());
        }

        // Confirm deletion
        let confirmed = Confirm::new()
            .with_prompt(format!("Are you sure you want to delete product '{}'?", id))
            .default(false)
            .interact()
            .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

        if !confirmed {
            println!("{} Deletion cancelled", "Info:".yellow());
            return Ok(());
        }

        sqlx::query("DELETE FROM products WHERE id = $1")
            .bind(product_id)
            .execute(&self.pool)
            .await?;

        println!("{} Product '{}' deleted successfully", "✓".green(), id);
        Ok(())
    }

    /// Delete a customer
    async fn delete_customer(&self, id: &str) -> Result<()> {
        let customer_id = parse_uuid(id)?;

        // Check if customer exists
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM customers WHERE id = $1)")
                .bind(customer_id)
                .fetch_one(&self.pool)
                .await?;

        if !exists {
            println!("{} Customer '{}' not found", "Error:".red(), id);
            return Ok(());
        }

        // Check if customer has orders
        let has_orders: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM orders WHERE customer_id = $1)")
                .bind(customer_id)
                .fetch_one(&self.pool)
                .await?;

        if has_orders {
            println!(
                "{} Cannot delete customer '{}' - they have existing orders",
                "Error:".red(),
                id
            );
            return Ok(());
        }

        // Confirm deletion
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "Are you sure you want to delete customer '{}'?",
                id
            ))
            .default(false)
            .interact()
            .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

        if !confirmed {
            println!("{} Deletion cancelled", "Info:".yellow());
            return Ok(());
        }

        sqlx::query("DELETE FROM customers WHERE id = $1")
            .bind(customer_id)
            .execute(&self.pool)
            .await?;

        println!("{} Customer '{}' deleted successfully", "✓".green(), id);
        Ok(())
    }

    /// Delete an API key
    async fn delete_api_key(&self, prefix: &str) -> Result<()> {
        // Check if key exists
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM api_keys WHERE key_prefix = $1)")
                .bind(prefix)
                .fetch_one(&self.pool)
                .await?;

        if !exists {
            println!("{} API key '{}' not found", "Error:".red(), prefix);
            return Ok(());
        }

        // Confirm deletion
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "Are you sure you want to permanently delete API key '{}'?",
                prefix
            ))
            .default(false)
            .interact()
            .map_err(|e| rcommerce_core::Error::validation(format!("Input error: {}", e)))?;

        if !confirmed {
            println!("{} Deletion cancelled", "Info:".yellow());
            return Ok(());
        }

        sqlx::query("DELETE FROM api_keys WHERE key_prefix = $1")
            .bind(prefix)
            .execute(&self.pool)
            .await?;

        println!("{} API key '{}' deleted successfully", "✓".green(), prefix);
        Ok(())
    }

    /// Search products
    async fn search_products(&self, query: &str) -> Result<()> {
        let search_pattern = format!("%{}%", query);

        let products: Vec<ProductRecord> = sqlx::query_as(
            "SELECT id, title, slug, price, currency::text, description, is_active, inventory_quantity, created_at 
             FROM products 
             WHERE title ILIKE $1 OR slug ILIKE $1 OR description ILIKE $1
             ORDER BY created_at DESC 
             LIMIT 20"
        )
            .bind(&search_pattern)
            .fetch_all(&self.pool)
            .await?;

        if products.is_empty() {
            println!(
                "{} No products found matching '{}'",
                "Info:".yellow(),
                query
            );
        } else {
            println!(
                "\n{}",
                format!("Products matching '{}' ({})", query, products.len())
                    .bold()
                    .underline()
            );
            println!(
                "  {:<36} {:<28} {:<10} {:<8} {:<12}",
                "ID", "Title", "Price", "Currency", "Status"
            );
            println!("  {}", "-".repeat(100));

            for p in &products {
                let status = if p.is_active {
                    "✓ Active".green()
                } else {
                    "✗ Inactive".red()
                };
                println!(
                    "  {:<36} {:<28} {:<10.2} {:<8} {}",
                    p.id.to_string(),
                    truncate(&p.title, 26),
                    p.price,
                    p.currency,
                    status
                );
            }
            println!();
        }

        Ok(())
    }

    /// Search customers
    async fn search_customers(&self, query: &str) -> Result<()> {
        let search_pattern = format!("%{}%", query);

        let customers: Vec<CustomerRecord> = sqlx::query_as(
            "SELECT id, email, first_name, last_name, created_at 
             FROM customers 
             WHERE email ILIKE $1 OR first_name ILIKE $1 OR last_name ILIKE $1
             ORDER BY created_at DESC 
             LIMIT 20",
        )
        .bind(&search_pattern)
        .fetch_all(&self.pool)
        .await?;

        if customers.is_empty() {
            println!(
                "{} No customers found matching '{}'",
                "Info:".yellow(),
                query
            );
        } else {
            println!(
                "\n{}",
                format!("Customers matching '{}' ({})", query, customers.len())
                    .bold()
                    .underline()
            );
            println!(
                "  {:<36} {:<28} {:<22} {:<12}",
                "ID", "Email", "Name", "Created"
            );
            println!("  {}", "-".repeat(100));

            for c in &customers {
                let name = format!("{} {}", c.first_name, c.last_name);
                println!(
                    "  {:<36} {:<28} {:<22} {}",
                    c.id.to_string(),
                    truncate(&c.email, 26),
                    truncate(&name, 20),
                    c.created_at.format("%Y-%m-%d")
                );
            }
            println!();
        }

        Ok(())
    }

    /// Search orders
    async fn search_orders(&self, query: &str) -> Result<()> {
        let search_pattern = format!("%{}%", query);

        let orders: Vec<OrderRecord> = sqlx::query_as(
            "SELECT o.id, c.email as customer_email, o.status::text, o.total, o.created_at 
             FROM orders o 
             JOIN customers c ON o.customer_id = c.id 
             WHERE c.email ILIKE $1 OR o.id::text ILIKE $1
             ORDER BY o.created_at DESC 
             LIMIT 20",
        )
        .bind(&search_pattern)
        .fetch_all(&self.pool)
        .await?;

        if orders.is_empty() {
            println!("{} No orders found matching '{}'", "Info:".yellow(), query);
        } else {
            println!(
                "\n{}",
                format!("Orders matching '{}' ({})", query, orders.len())
                    .bold()
                    .underline()
            );
            println!(
                "  {:<36} {:<22} {:<12} {:<12} {:<12}",
                "ID", "Customer", "Status", "Total", "Created"
            );
            println!("  {}", "-".repeat(100));

            for o in &orders {
                let status_colored = match o.status.as_str() {
                    "completed" => o.status.green(),
                    "pending" => o.status.yellow(),
                    "cancelled" | "refunded" => o.status.red(),
                    _ => o.status.normal(),
                };
                println!(
                    "  {:<36} {:<22} {:<12} ${:<11.2} {}",
                    o.id.to_string(),
                    truncate(&o.customer_email, 20),
                    status_colored,
                    o.total,
                    o.created_at.format("%Y-%m-%d")
                );
            }
            println!();
        }

        Ok(())
    }
}

// Database record structures

#[derive(Debug, sqlx::FromRow)]
struct ProductRecord {
    id: Uuid,
    title: String,
    slug: String,
    price: Decimal,
    currency: String,
    description: Option<String>,
    is_active: bool,
    inventory_quantity: i32,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct OrderRecord {
    id: Uuid,
    customer_email: String,
    status: String,
    total: Decimal,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct OrderDetailRecord {
    id: Uuid,
    customer_email: String,
    status: String,
    total: Decimal,
    subtotal: Decimal,
    tax_amount: Decimal,
    shipping_amount: Decimal,
    discount_amount: Decimal,
    currency: String,
    notes: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct OrderItemRecord {
    title: String,
    quantity: i32,
    price: Decimal,
    total: Decimal,
}

#[derive(Debug, sqlx::FromRow)]
struct CustomerRecord {
    id: Uuid,
    email: String,
    first_name: String,
    last_name: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct CustomerDetailRecord {
    id: Uuid,
    email: String,
    first_name: String,
    last_name: String,
    phone: Option<String>,
    accepts_marketing: bool,
    currency: String,
    is_verified: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct ApiKeyRecord {
    key_prefix: String,
    name: String,
    scopes: Vec<String>,
    is_active: bool,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct ApiKeyDetailRecord {
    id: Uuid,
    key_prefix: String,
    name: String,
    scopes: Vec<String>,
    is_active: bool,
    customer_id: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    last_used_ip: Option<String>,
}

// Helper functions

/// Parse a UUID from string
fn parse_uuid(id: &str) -> Result<Uuid> {
    Uuid::parse_str(id)
        .map_err(|e| rcommerce_core::Error::validation(format!("Invalid ID format: {}", e)))
}

/// Truncate a string to max length
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    } else {
        s.to_string()
    }
}

/// Convert string to slug
fn slugify(input: &str) -> String {
    input
        .to_lowercase()
        .replace(' ', "-")
        .replace(|c: char| !c.is_alphanumeric() && c != '-', "")
        .replace("--", "-")
        .trim_matches('-')
        .to_string()
}

/// Format status with color
fn format_status(status: &str) -> colored::ColoredString {
    match status {
        "completed" => status.green(),
        "pending" => status.yellow(),
        "processing" => status.blue(),
        "cancelled" | "refunded" => status.red(),
        "on_hold" => status.magenta(),
        _ => status.normal(),
    }
}

/// Run the interactive shell
pub async fn run_shell(pool: PgPool) -> Result<()> {
    let mut shell = Shell::new(pool);
    shell.run().await
}

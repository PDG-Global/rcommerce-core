use anyhow::Result;
use clap::Parser;
use rcommerce::{config::Config, server::Server};
use tracing::{info, error};

#[derive(Parser)]
#[command(
    name = "rcommerce",
    about = "R commerce - A lightweight headless ecommerce platform",
    version
)]
struct Cli {
    #[arg(short, long, value_name = "FILE", help = "Configuration file path")]
    config: Option<String>,

    #[arg(short, long, help = "Enable verbose logging")]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Start the HTTP server
    Server {
        #[arg(short, long, default_value = "0.0.0.0", help = "Bind address")]
        host: String,
        
        #[arg(short, long, default_value_t = 8080, help = "Bind port")]
        port: u16,
        
        #[arg(long, help = "Number of worker threads")]
        worker_threads: Option<usize>,
    },
    
    /// Run database migrations
    Migrate {
        #[command(subcommand)]
        command: MigrateCommand,
    },
    
    /// Check system health
    Health,
    
    /// Show configuration
    Config {
        #[arg(short, long, help = "Show full configuration")]
        show: bool,
        
        #[arg(short, long, help = "Validate configuration")]
        validate: bool,
    },
}

#[derive(clap::Subcommand)]
enum MigrateCommand {
    /// Run pending migrations
    Run,
    
    /// Revert last migration
    Rollback,
    
    /// Show migration status
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize tracing
    init_tracing(cli.verbose);
    
    info!("Starting R commerce v{}", env!("CARGO_PKG_VERSION"));
    
    // Load configuration
    let config_path = cli.config
        .or_else(|| std::env::var("RCOMMERCE_CONFIG").ok())
        .unwrap_or_else(|| "config/default.toml".to_string());
    
    info!("Loading configuration from: {}", config_path);
    let config = Config::load(&config_path)?;
    
    // Execute command
    match cli.command {
        Some(Commands::Server { host, port, worker_threads }) => {
            run_server(config, host, port, worker_threads).await?;
        }
        
        Some(Commands::Migrate { command }) => {
            run_migrations(config, command).await?;
        }
        
        Some(Commands::Health) => {
            run_health_check(config).await?;
        }
        
        Some(Commands::Config { show, validate }) => {
            run_config_command(config, show, validate).await?;
        }
        
        None => {
            // Default to server if no command specified
            info!("No command specified, starting server...");
            run_server(config, "0.0.0.0".to_string(), 8080, None).await?;
        }
    }
    
    Ok(())
}

fn init_tracing(verbose: bool) {
    let log_directive = if verbose {
        "rcommerce=debug,tower_http=debug,sqlx=warn"
    } else {
        "rcommerce=info,tower_http=warn,sqlx=error"
    };
    
    tracing_subscriber::fmt()
        .with_env_filter(log_directive)
        .with_target(false)
        .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
        .init();
}

async fn run_server(
    config: Config,
    host: String,
    port: u16,
    worker_threads: Option<usize>,
) -> Result<()> {
    info!("Starting server on {}:{}", host, port);
    
    let server = Server::new(config)?;
    server.run((&host[..], port).into()).await?;
    
    Ok(())
}

async fn run_migrations(config: Config, command: MigrateCommand) -> Result<()> {
    use rcommerce_db::migrator::Migrator;
    
    info!("Running migrations...");
    
    let migrator = Migrator::new(&config.database).await?;
    
    match command {
        MigrateCommand::Run => {
            migrator.run_migrations().await?;
            info!("Migrations completed successfully");
        }
        MigrateCommand::Rollback => {
            migrator.rollback().await?;
            info!("Last migration rolled back");
        }
        MigrateCommand::Status => {
            let status = migrator.status().await?;
            info!("Migration status: {:?}", status);
        }
    }
    
    Ok(())
}

async fn run_health_check(config: Config) -> Result<()> {
    use rcommerce_services::health::HealthService;
    
    info!("Running health check...");
    
    let health = HealthService::new(&config)?;
    let status = health.check().await?;
    
    println!("{}", serde_json::to_string_pretty(&status)?);
    
    if status.healthy {
        info!("System is healthy");
    } else {
        error!("System health check failed");
        std::process::exit(1);
    }
    
    Ok(())
}

async fn run_config_command(config: Config, show: bool, validate: bool) -> Result<()> {
    if validate {
        info!("Validating configuration...");
        config.validate()?;
        info!("Configuration is valid");
    }
    
    if show {
        println!("{}", toml::to_string_pretty(&config)?);
    }
    
    Ok(())
}

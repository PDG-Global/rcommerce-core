use std::sync::Arc;

use rcommerce_core::cache::RedisPool;
use rcommerce_core::notification::channels::email::{
    EmailChannel, EmailMode, PostmarkConfig, SmtpConfig,
};
use rcommerce_core::notification::service::NotificationService;
use rcommerce_core::payment::agnostic::PaymentService;
use rcommerce_core::payment::PaymentGateway;
use rcommerce_core::repository::{
    Database, PostgresApiKeyRepository, PostgresSubscriptionRepository,
};
use rcommerce_core::services::{
    AuthService, CartService, CheckoutService, CouponService, CustomerService, OrderService,
    ProductService, SubscriptionService,
};
use rcommerce_core::shipping::ShippingProviderFactory;
use rcommerce_core::tax::DefaultTaxService;
use rcommerce_core::{BundleService, DigitalProductService, FileUploadService};

use crate::middleware::{AuthRateLimiter, ClientIpResolver, LoginThrottle};

/// Parameters for creating AppState
pub struct AppStateParams {
    pub product_service: ProductService,
    pub customer_service: CustomerService,
    pub auth_service: AuthService,
    pub db: Database,
    pub redis: Option<RedisPool>,
    pub api_key_repository: PostgresApiKeyRepository,
    pub subscription_repository: PostgresSubscriptionRepository,
    pub coupon_service: CouponService,
    pub payment_service: PaymentService,
    pub file_upload_service: FileUploadService,
    pub cart_service: Arc<CartService>,
    pub order_service: Arc<OrderService>,
    pub tax_service: Arc<DefaultTaxService>,
    pub shipping_factory: Arc<ShippingProviderFactory>,
    pub checkout_service: Arc<CheckoutService>,
    /// Legacy payment gateway used to charge subscription renewals against
    /// stored-card consents (same gateway instance the checkout uses).
    pub subscription_gateway: Option<Arc<dyn PaymentGateway>>,
    pub inventory_service: Arc<rcommerce_core::inventory::InventoryService>,
    pub smtp_config: Option<SmtpConfig>,
    pub xero_config: Option<rcommerce_core::config::XeroConfig>,
    pub notification_admin_email: Option<String>,
    pub branding: rcommerce_core::BrandingConfig,
    /// Accounting export configuration (store code/name, site URL)
    pub accounting: rcommerce_core::config::AccountingConfig,
    /// Store base currency for new carts (ISO code, e.g. "USD", "AUD")
    pub default_currency: String,
    /// Airwallex gateway config for settlement reconciliation
    pub airwallex_config: Option<rcommerce_core::config::AirwallexConfig>,
    /// Rate limiting / client-IP configuration used for the AppState-owned
    /// limiters. Defaults apply unless overridden via `with_rate_limiting`.
    pub rate_limiting: rcommerce_core::config::RateLimitConfig,
}

impl AppStateParams {
    /// Override the rate limiting configuration (used by the real servers;
    /// tests get the defaults from `new`).
    pub fn with_rate_limiting(mut self, config: rcommerce_core::config::RateLimitConfig) -> Self {
        self.rate_limiting = config;
        self
    }
}

impl AppStateParams {
    /// Create a new AppStateParams with all required fields
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        product_service: ProductService,
        customer_service: CustomerService,
        auth_service: AuthService,
        db: Database,
        redis: Option<RedisPool>,
        api_key_repository: PostgresApiKeyRepository,
        subscription_repository: PostgresSubscriptionRepository,
        coupon_service: CouponService,
        payment_service: PaymentService,
        file_upload_service: FileUploadService,
        cart_service: Arc<CartService>,
        order_service: Arc<OrderService>,
        tax_service: Arc<DefaultTaxService>,
        shipping_factory: Arc<ShippingProviderFactory>,
        checkout_service: Arc<CheckoutService>,
        subscription_gateway: Option<Arc<dyn PaymentGateway>>,
        inventory_service: Arc<rcommerce_core::inventory::InventoryService>,
        smtp_config: Option<SmtpConfig>,
        xero_config: Option<rcommerce_core::config::XeroConfig>,
        notification_admin_email: Option<String>,
        branding: rcommerce_core::BrandingConfig,
        accounting: rcommerce_core::config::AccountingConfig,
        default_currency: String,
        airwallex_config: Option<rcommerce_core::config::AirwallexConfig>,
    ) -> Self {
        Self {
            product_service,
            customer_service,
            auth_service,
            db,
            redis,
            api_key_repository,
            subscription_repository,
            coupon_service,
            payment_service,
            file_upload_service,
            cart_service,
            order_service,
            tax_service,
            shipping_factory,
            checkout_service,
            subscription_gateway,
            inventory_service,
            smtp_config,
            xero_config,
            notification_admin_email,
            branding,
            accounting,
            default_currency,
            airwallex_config,
            rate_limiting: rcommerce_core::config::RateLimitConfig::default(),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub product_service: ProductService,
    pub customer_service: CustomerService,
    pub auth_service: AuthService,
    pub subscription_service: SubscriptionService<PostgresSubscriptionRepository>,
    pub subscription_repository: Arc<PostgresSubscriptionRepository>,
    pub coupon_service: CouponService,
    pub payment_service: Arc<PaymentService>,
    pub digital_product_service: Arc<DigitalProductService>,
    pub bundle_service: Arc<BundleService>,
    pub file_upload_service: Arc<FileUploadService>,
    pub cart_service: Arc<CartService>,
    pub order_service: Arc<OrderService>,
    pub tax_service: Arc<DefaultTaxService>,
    pub shipping_factory: Arc<ShippingProviderFactory>,
    pub checkout_service: Arc<CheckoutService>,
    /// Legacy payment gateway used to charge subscription renewals against
    /// stored-card consents. None only when constructed by tests.
    pub subscription_gateway: Option<Arc<dyn PaymentGateway>>,
    pub inventory_service: Arc<rcommerce_core::inventory::InventoryService>,
    pub db: Database,
    pub redis: Option<RedisPool>,
    pub auth_rate_limiter: AuthRateLimiter,
    /// Trusted-proxy-aware client IP resolver (rate limiting, login
    /// throttling, API-key audit).
    pub client_ip_resolver: ClientIpResolver,
    /// Login throttling: source-IP attempt budget and (IP, e-mail) failure
    /// budgets.
    pub login_throttle: LoginThrottle,
    /// Eagerly computed Argon2 hash used to answer unknown e-mails with the
    /// same cost as real ones (defeats login enumeration by timing).
    pub dummy_password_hash: String,
    pub api_key_repository: Arc<PostgresApiKeyRepository>,
    pub email_channel: Option<Arc<EmailChannel>>,
    pub notification_service: Option<Arc<NotificationService>>,
    pub notification_from_email: Option<String>,
    pub notification_admin_email: Option<String>,
    pub xero_client: Option<Arc<rcommerce_core::accounting::XeroClient>>,
    /// Store branding for customer-facing emails and invoices
    pub branding: rcommerce_core::BrandingConfig,
    /// Accounting export configuration (store code/name, site URL)
    pub accounting: rcommerce_core::config::AccountingConfig,
    /// Store base currency for new carts (ISO code, e.g. "USD", "AUD")
    pub default_currency: String,
    /// Airwallex settlement reconciliation client (None when unconfigured)
    pub settlement_sync: Option<Arc<rcommerce_core::accounting::SettlementSync>>,
}

impl AppState {
    pub async fn new(params: AppStateParams) -> Self {
        let rate_limiting = params.rate_limiting.clone();

        // Create auth rate limiter (5 attempts per minute per IP)
        let auth_rate_limiter = AuthRateLimiter::new(5, 60, rate_limiting.max_tracked_ips.max(1));

        // Trusted-proxy-aware client IP resolution. Server startup validates
        // the configured CIDRs before AppState is built; this fallback only
        // fires for hand-constructed states with an invalid config.
        let client_ip_resolver =
            ClientIpResolver::from_config(&rate_limiting).unwrap_or_else(|e| {
                tracing::error!(
                    "Invalid rate limiting config in AppStateParams ({}); \
                     running without trusted proxies",
                    e
                );
                ClientIpResolver::new(Vec::new(), rate_limiting.max_forwarded_hops)
            });

        // Login throttling from the same configuration section.
        let login_throttle = LoginThrottle::new(&rate_limiting);

        // Eagerly compute the dummy hash so unknown e-mails cost the same
        // Argon2 work as real ones. Doing it once at startup keeps login
        // latency uniform; a failure here must not keep the server from
        // starting (verification errors are ignored by the login flow).
        let dummy_password_hash = match params
            .auth_service
            .hash_password("rcommerce-invalid-credentials-placeholder")
        {
            Ok(hash) => hash,
            Err(e) => {
                tracing::error!("Failed to pre-compute dummy password hash: {}", e);
                String::new()
            }
        };

        // Remember the configured sender address for shop-operator notifications
        let notification_from_email = params
            .smtp_config
            .as_ref()
            .map(|smtp| smtp.from_address.clone());

        // Store branding for customer-facing emails and invoices
        let branding = params.branding.clone();

        // Create subscription service
        let subscription_service = SubscriptionService::new(params.subscription_repository.clone());

        // Create digital product service
        let file_upload_service = Arc::new(params.file_upload_service);
        let digital_product_service = Arc::new(DigitalProductService::new(
            params.db.clone(),
            file_upload_service.clone(),
        ));

        // Create bundle service
        let bundle_service = Arc::new(BundleService::new(params.db.clone()));

        // Create notification service if email is configured
        let (email_channel, notification_service) = if let Some(smtp) = params.smtp_config {
            // Check if this is a Postmark configuration (host == "postmark")
            let channel_result = if smtp.host == "postmark" {
                let postmark_config = PostmarkConfig {
                    server_token: smtp.username.clone(),
                    from_address: smtp.from_address.clone(),
                    from_name: smtp.from_name.clone(),
                    message_stream: "outbound".to_string(),
                };
                Ok(EmailChannel::new_postmark(postmark_config))
            } else {
                EmailChannel::new_smtp(smtp).await
            };

            match channel_result {
                Ok(channel) => {
                    let channel = Arc::new(channel);
                    let svc = Arc::new(NotificationService::new(
                        channel.as_ref().clone(),
                        rcommerce_core::notification::channels::SmsChannel,
                        rcommerce_core::notification::channels::WebhookChannel,
                        params.db.pool().clone(),
                    ));
                    tracing::info!("Notification service initialized");
                    (Some(channel), Some(svc))
                }
                Err(e) => {
                    tracing::error!("Failed to initialize email notification service: {}", e);
                    (None, None)
                }
            }
        } else {
            tracing::info!("Email not configured; notification service disabled");
            (None, None)
        };

        // Create Xero client if configured
        let xero_client = params.xero_config.and_then(|cfg| {
            let client = rcommerce_core::accounting::XeroClient::new(cfg);
            if client.is_configured() {
                tracing::info!("Xero integration enabled");
                Some(Arc::new(client))
            } else {
                tracing::info!("Xero not fully configured; integration disabled");
                None
            }
        });

        // Create the Airwallex settlement sync client if configured
        let settlement_sync = params.airwallex_config.and_then(|cfg| {
            if cfg.enabled {
                tracing::info!("Airwallex settlement sync enabled");
                rcommerce_core::accounting::SettlementSync::new(&cfg).map(Arc::new)
            } else {
                None
            }
        });

        Self {
            product_service: params.product_service,
            customer_service: params.customer_service,
            auth_service: params.auth_service,
            subscription_service,
            subscription_repository: Arc::new(params.subscription_repository),
            coupon_service: params.coupon_service,
            payment_service: Arc::new(params.payment_service),
            digital_product_service,
            bundle_service,
            file_upload_service,
            cart_service: params.cart_service,
            order_service: params.order_service,
            tax_service: params.tax_service,
            shipping_factory: params.shipping_factory,
            checkout_service: params.checkout_service,
            subscription_gateway: params.subscription_gateway,
            inventory_service: params.inventory_service,
            db: params.db,
            redis: params.redis,
            auth_rate_limiter,
            client_ip_resolver,
            login_throttle,
            dummy_password_hash,
            api_key_repository: Arc::new(params.api_key_repository),
            email_channel,
            notification_service,
            notification_from_email,
            notification_admin_email: params.notification_admin_email,
            xero_client,
            branding,
            accounting: params.accounting,
            default_currency: params.default_currency,
            settlement_sync,
        }
    }
}

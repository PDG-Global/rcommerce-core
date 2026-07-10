# Shipping Integration Architecture

## Overview

The shipping system provides a unified interface for generating shipping rates, creating shipping labels, and tracking packages across multiple carriers and regional providers. The system is designed to handle both international shipping giants (UPS, FedEx, DHL) and regional providers (including ERP systems like dianxiaomi).

**Key Design Goals:**
- Support multiple carriers with unified API
- Real-time rate calculation
- Automated label generation
- Shipment tracking aggregation
- Multi-location inventory support
- Customs documentation for international shipping
- Weight-based and volumetric weight calculations

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     API Layer (ShippingController)           │
│  - Rate calculation endpoints                               │
│  - Label generation endpoints                               │
│  - Tracking endpoints                                       │
│  - Zone management                                          │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│              Shipping Orchestrator                          │
│  - Rate aggregation and comparison                          │
│  - Provider selection logic                                 │
│  - Shipment lifecycle management                            │
│  - Multi-package shipment coordination                      │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│              Shipping Calculation Engine                    │
│  - Weight-based calculations                                │
│  - Volumetric weight (dimensional weight)                   │
│  - Multi-unit conversions (kg, lb, oz, g)                   │
│  - Tiered rate calculations                                 │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│              Shipping Zone & Rules Engine                   │
│  - Zone-based rate calculation                              │
│  - Conditional shipping rules                               │
│  - Free shipping thresholds                                 │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│              Provider Factory                               │
│  - Dynamic provider loading                                 │
│  - Provider registry                                        │
│  - Credential management                                    │
└────────────┬────────────────────────┬───────────────────────┘
             │                        │
    ┌────────▼──────────┐    ┌────────▼──────────────┐
    │   Direct Carriers │    │   Aggregators         │
    │                   │    │                       │
    │ - DHL             │    │ - EasyPost            │
    │ - FedEx           │    │ - ShipStation         │
    │ - UPS             │    │                       │
    │ - USPS            │    │ - Multi-carrier API   │
    │                   │    │ - Rate shopping       │
    │ - Rate APIs       │    │ - Label generation    │
    │ - Label creation  │    │ - Tracking            │
    │ - Tracking        │    │ - Address validation  │
    └───────────────────┘    └───────────────────────┘
```

## Core Data Models

### Package Model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub weight: Decimal,
    pub weight_unit: String,  // "kg", "g", "lb", "oz"
    pub length: Option<Decimal>,
    pub width: Option<Decimal>,
    pub height: Option<Decimal>,
    pub dimension_unit: Option<String>,  // "cm", "m", "in", "ft"
    pub predefined_package: Option<String>, // "small_flat_rate_box"
}
```

### Shipment Model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shipment {
    pub id: Uuid,
    pub order_id: Option<Uuid>,
    pub provider_id: String,  // "ups", "fedex", "dhl", "easypost"
    pub carrier: String,
    pub service_code: String,
    pub service_name: String,
    pub status: ShipmentStatus,
    pub from_address: Address,
    pub to_address: Address,
    pub package: Package,
    pub tracking_number: Option<String>,
    pub tracking_url: Option<String>,
    pub label_url: Option<String>,
    pub customs_info: Option<CustomsInfo>,
    pub total_cost: Decimal,
    pub currency: String,
    pub created_at: DateTime<Utc>,
    pub shipped_at: Option<DateTime<Utc>>,
    pub estimated_delivery: Option<DateTime<Utc>>,
}
```

### Shipping Rate Model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShippingRate {
    pub provider_id: String,
    pub carrier: String,
    pub service_code: String,
    pub service_name: String,
    pub rate: Decimal,
    pub currency: String,
    pub delivery_days: Option<i32>,
    pub delivery_date: Option<DateTime<Utc>>,
    pub estimated: bool,
    pub insurance_fee: Option<Decimal>,
    pub fuel_surcharge: Option<Decimal>,
    pub handling_fee: Option<Decimal>,
    pub total_cost: Decimal,
}
```

## Weight Calculations

### Weight Converter

Converts between weight units:
- Kilograms (kg)
- Grams (g)
- Pounds (lb)
- Ounces (oz)

```rust
let kg = WeightConverter::to_kg(Decimal::from(10), WeightUnit::Lb);
// 10 lbs = 4.53592 kg
```

### Volumetric Weight Calculator

Calculates dimensional weight using industry-standard formulas:

**International (cm/kg):**
```rust
let calc = VolumetricWeightCalculator::standard_international();
// Factor: 5000 (DHL, FedEx, UPS standard)
// Formula: (L × W × H) / 5000 = kg
```

**Domestic US (in/lb):**
```rust
let calc = VolumetricWeightCalculator::standard_domestic_us();
// Factor: 139 (FedEx, UPS domestic)
// Formula: (L × W × H) / 139 = lb
```

**USPS:**
```rust
let calc = VolumetricWeightCalculator::usps();
// Factor: 166 (USPS standard)
```

### Chargeable Weight

The chargeable weight is the greater of actual weight or volumetric weight:

```rust
let calc = VolumetricWeightCalculator::standard_international();
let chargeable = calc.calculate_chargeable_weight(&package);

println!("Actual: {} kg", chargeable.actual_weight);
println!("Volumetric: {:?} kg", chargeable.volumetric_weight);
println!("Chargeable: {} kg", chargeable.chargeable_weight);
```

## Shipping Provider Trait

```rust
#[async_trait]
pub trait ShippingProvider: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn is_available(&self) -> bool;
    
    async fn get_rates(
        &self,
        from_address: &Address,
        to_address: &Address,
        package: &Package,
        options: &RateOptions,
    ) -> Result<Vec<ShippingRate>>;
    
    async fn create_shipment(
        &self,
        from_address: &Address,
        to_address: &Address,
        package: &Package,
        service_code: &str,
        customs_info: Option<&CustomsInfo>,
    ) -> Result<Shipment>;
    
    async fn track_shipment(&self, tracking_number: &str) -> Result<TrackingInfo>;
    async fn cancel_shipment(&self, shipment_id: &str) -> Result<bool>;
    async fn validate_address(&self, address: &Address) -> Result<AddressValidation>;
    fn get_services(&self) -> Vec<ShippingService>;
}
```

## Supported Carriers

### Direct Carrier Integrations

#### DHL Express
```rust
let dhl = DhlProvider::new(api_key, api_secret, account_number)
    .with_test_mode(true);
```

**Services:**
- EXPRESS_WORLDWIDE - 1-3 business days
- EXPRESS_12:00 - Next business day by 12:00
- ECONOMY_SELECT - 4-7 business days

#### FedEx
```rust
let fedex = FedExProvider::new(api_key, api_secret, account_number)
    .with_test_mode(true);
```

**Services:**
- FEDEX_GROUND - 1-5 business days
- FEDEX_2_DAY - 2 business days
- PRIORITY_OVERNIGHT - Next business day
- INTERNATIONAL_PRIORITY - 1-3 business days

#### UPS
```rust
let ups = UpsProvider::new(api_key, username, password, account_number)
    .with_test_mode(true);
```

**Services:**
- 03 (Ground) - 1-5 business days
- 02 (2nd Day Air) - 2 business days
- 01 (Next Day Air) - Next business day
- 65 (Worldwide Saver) - 1-3 business days

#### USPS
```rust
let usps = UspsProvider::new(api_key);
```

**Services:**
- USPS_GROUND_ADVANTAGE - 2-5 business days
- PRIORITY_MAIL - 1-3 business days
- PRIORITY_MAIL_EXPRESS - 1 business day

### Third-Party Aggregators

#### EasyPost
```rust
let easypost = EasyPostProvider::new(api_key)
    .with_test_mode(true);
```

**Features:**
- Multi-carrier rate shopping
- Single API for 100+ carriers
- Address verification
- Insurance options

#### ShipStation
```rust
let shipstation = ShipStationProvider::new(api_key, api_secret);
```

**Features:**
- Order management integration
- Multi-carrier label generation
- Batch processing
- Inventory sync

## Shipping Zones

Define geographic zones with specific rates:

```rust
let domestic = ShippingZone::new("domestic", "United States")
    .with_country("US")
    .with_rate(
        ZoneRate::new("Standard Ground", Decimal::from(8), Decimal::from(1))
            .with_free_shipping_threshold(Decimal::from(100))
    );

let canada = ShippingZone::new("canada", "Canada")
    .with_country("CA")
    .with_rate(
        ZoneRate::new("Standard International", Decimal::from(20), Decimal::from(3))
    );

let mut calculator = ZoneCalculator::new();
calculator.add_zone(domestic);
calculator.add_zone(canada);

// Calculate shipping
if let Some((cost, rate)) = calculator.calculate_shipping(&address, weight, subtotal) {
    println!("Shipping cost: ${}", cost);
}
```

## Shipping Rules Engine

Create conditional shipping logic:

```rust
// Free shipping for orders over $100
let free_shipping = ShippingRule::new(
    "Free Shipping Threshold",
    RuleCondition::OrderTotal { 
        min: Some(Decimal::from(100)), 
        max: None 
    },
    RuleAction::FreeShipping,
).with_priority(100);

// Hide express for heavy orders
let hide_express = ShippingRule::new(
    "Hide Express for Heavy",
    RuleCondition::OrderWeight { 
        min: Some(Decimal::from(50)), 
        max: None 
    },
    RuleAction::HideMethods { 
        methods: vec!["express".to_string()] 
    },
);

let mut engine = ShippingRuleEngine::new();
engine.add_rule(free_shipping);
engine.add_rule(hide_express);

// Apply rules
engine.apply_rules(&order, &mut rates);
```

## Packaging Calculator

Calculate optimal packaging for items:

```rust
let calculator = PackagingCalculator::new();

// Calculate optimal boxes for items
let recommendations = calculator.calculate_optimal_packaging(
    &items,
    Decimal::from(50), // max weight per package
);

// Find best flat rate option
if let Some(package_type) = calculator.find_best_flat_rate(&items, Some("USPS")) {
    println!("Best flat rate: {} at ${}", 
        package_type.name, 
        package_type.flat_rate.unwrap()
    );
}
```

## Predefined Package Types

Standard carrier packaging:

- **USPS:** Flat Rate Envelope, Small/Medium/Large Flat Rate Boxes
- **UPS:** Express Box Small/Medium/Large
- **FedEx:** Small/Medium/Large Boxes
- **DHL:** Express Envelope, Small/Medium Boxes

## Rate Aggregation

Get rates from multiple providers:

```rust
let mut factory = ShippingProviderFactory::new();
factory.register(Box::new(ups));
factory.register(Box::new(fedex));
factory.register(Box::new(dhl));

let aggregator = ShippingRateAggregator::new(factory);

// Get all rates
let rates = aggregator.get_all_rates(
    &from_address,
    &to_address,
    &package,
    &RateOptions::default(),
).await?;

// Rates sorted by total cost
for rate in rates {
    println!("{} {}: ${}", 
        rate.carrier, 
        rate.service_name, 
        rate.total_cost
    );
}
```

## Customs Information

For international shipments:

```rust
let customs = CustomsInfo {
    contents_type: ContentsType::Merchandise,
    contents_description: "Electronic components".to_string(),
    non_delivery_option: NonDeliveryOption::Return,
    customs_items: vec![
        CustomsItem {
            description: "Circuit board".to_string(),
            quantity: 2,
            value: Decimal::from(50),
            currency: "USD".to_string(),
            weight: Some(Decimal::from(1)),
            weight_unit: Some("kg".to_string()),
            hs_tariff_number: Some("8517.62.00".to_string()),
            origin_country: "US".to_string(),
        }
    ],
    declaration_value: Decimal::from(100),
    declaration_currency: "USD".to_string(),
};
```

## Tracking

### Detect Carrier from Tracking Number

```rust
let carrier = detect_carrier_from_tracking("1Z999AA10123456784");
// Returns: Some("ups")

let url = get_tracking_url("ups", "1Z999AA10123456784");
// Returns: "https://www.ups.com/track?tracknum=1Z999AA10123456784"
```

### Track Shipment

```rust
let tracking = provider.track_shipment("1Z999AA10123456784").await?;

println!("Status: {}", tracking.status.description());
for event in &tracking.events {
    println!("{} - {} at {:?}", 
        event.timestamp, 
        event.description,
        event.location
    );
}
```

## Configuration

Add to your `config.toml`:

```toml
[shipping]
default_provider = "ups"

[shipping.ups]
api_key = "your_api_key"
username = "your_username"
password = "your_password"
account_number = "your_account"

[shipping.fedex]
api_key = "your_api_key"
api_secret = "your_secret"
account_number = "your_account"

[shipping.dhl]
api_key = "your_api_key"
api_secret = "your_secret"
account_number = "your_account"

[shipping.easypost]
api_key = "your_api_key"

[shipping.zones]
default_rate = { base = 10.00, weight_rate = 1.00, unit = "kg" }
free_shipping_threshold = 100.00
```

## Testing

Use test mode for development:

```rust
let ups = UpsProvider::new(api_key, username, password, account)
    .with_test_mode(true);

// All API calls go to sandbox environment
// Labels are test labels (not valid for shipping)
```

## Error Handling

All provider methods return `Result<T, Error>`:

```rust
match provider.get_rates(&from, &to, &package, &options).await {
    Ok(rates) => {
        // Process rates
    }
    Err(Error::ApiError { status, message }) => {
        // Handle API error
    }
    Err(Error::RateNotFound) => {
        // No rates available for this route
    }
    Err(e) => {
        // Handle other errors
    }
}
```

## Performance Considerations

1. **Rate Caching:** Cache rates for common routes (respect TTL from carriers)
2. **Parallel Requests:** Request rates from multiple carriers in parallel
3. **Connection Pooling:** Use shared HTTP client for connection reuse
4. **Timeout Handling:** Set appropriate timeouts for carrier APIs

## Security

1. **Credential Storage:** Store API credentials securely (environment variables or secrets manager)
2. **Webhook Verification:** Always verify webhook signatures from carriers
3. **PII Handling:** Don't log addresses or tracking numbers in plain text
4. **TLS:** All carrier APIs require TLS 1.2+

---

Next: [07-order-management.md](07-order-management.md) - Order management system details

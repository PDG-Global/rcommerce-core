//! Packaging and package calculations

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::calculation::{VolumetricWeightCalculator, WeightUnit};

/// Package information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    /// Weight of the package
    pub weight: Decimal,
    /// Weight unit (kg, g, lb, oz)
    pub weight_unit: String,
    /// Length dimension
    pub length: Option<Decimal>,
    /// Width dimension
    pub width: Option<Decimal>,
    /// Height dimension
    pub height: Option<Decimal>,
    /// Dimension unit (cm, m, in, ft)
    pub dimension_unit: Option<String>,
    /// Predefined package type (if using carrier's flat rate boxes)
    pub predefined_package: Option<String>,
}

impl Package {
    /// Create a new package with weight only
    pub fn new(weight: Decimal, weight_unit: impl Into<String>) -> Self {
        Self {
            weight,
            weight_unit: weight_unit.into(),
            length: None,
            width: None,
            height: None,
            dimension_unit: None,
            predefined_package: None,
        }
    }

    /// Create a package with dimensions
    pub fn with_dimensions(
        mut self,
        length: Decimal,
        width: Decimal,
        height: Decimal,
        unit: impl Into<String>,
    ) -> Self {
        self.length = Some(length);
        self.width = Some(width);
        self.height = Some(height);
        self.dimension_unit = Some(unit.into());
        self
    }

    /// Set predefined package type
    pub fn with_predefined_package(mut self, package_type: impl Into<String>) -> Self {
        self.predefined_package = Some(package_type.into());
        self
    }

    /// Calculate volume in cubic centimeters
    pub fn volume_cm3(&self) -> Option<Decimal> {
        match (
            self.length,
            self.width,
            self.height,
            self.dimension_unit.as_deref(),
        ) {
            (Some(l), Some(w), Some(h), Some(unit)) => {
                let (l_cm, w_cm, h_cm) = match unit {
                    "cm" => (l, w, h),
                    "m" => (
                        l * Decimal::from(100),
                        w * Decimal::from(100),
                        h * Decimal::from(100),
                    ),
                    "in" => (
                        l * Decimal::from_str_exact("2.54").unwrap_or(Decimal::from(3)),
                        w * Decimal::from_str_exact("2.54").unwrap_or(Decimal::from(3)),
                        h * Decimal::from_str_exact("2.54").unwrap_or(Decimal::from(3)),
                    ),
                    "ft" => (
                        l * Decimal::from_str_exact("30.48").unwrap_or(Decimal::from(30)),
                        w * Decimal::from_str_exact("30.48").unwrap_or(Decimal::from(30)),
                        h * Decimal::from_str_exact("30.48").unwrap_or(Decimal::from(30)),
                    ),
                    _ => (l, w, h),
                };
                Some(l_cm * w_cm * h_cm)
            }
            _ => None,
        }
    }

    /// Check if package qualifies as a "large package" (girth > certain threshold)
    pub fn is_oversized(&self, max_girth_cm: Decimal) -> bool {
        match self.girth_cm() {
            Some(girth) => girth > max_girth_cm,
            None => false,
        }
    }

    /// Calculate girth (2 * (width + height))
    pub fn girth_cm(&self) -> Option<Decimal> {
        match (self.width, self.height, self.dimension_unit.as_deref()) {
            (Some(w), Some(h), Some(unit)) => {
                let (w_cm, h_cm) = match unit {
                    "cm" => (w, h),
                    "m" => (w * Decimal::from(100), h * Decimal::from(100)),
                    "in" => (
                        w * Decimal::from_str_exact("2.54").unwrap_or(Decimal::from(3)),
                        h * Decimal::from_str_exact("2.54").unwrap_or(Decimal::from(3)),
                    ),
                    "ft" => (
                        w * Decimal::from_str_exact("30.48").unwrap_or(Decimal::from(30)),
                        h * Decimal::from_str_exact("30.48").unwrap_or(Decimal::from(30)),
                    ),
                    _ => (w, h),
                };
                Some(Decimal::from(2) * (w_cm + h_cm))
            }
            _ => None,
        }
    }

    /// Get dimensional weight using standard calculator
    pub fn dimensional_weight(&self) -> Option<(Decimal, WeightUnit)> {
        let calc = VolumetricWeightCalculator::standard_international();
        calc.calculate_for_package(self)
            .map(|w| (w, WeightUnit::Kg))
    }
}

/// Package types (flat rate boxes, envelopes, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageType {
    pub code: String,
    pub name: String,
    pub carrier: String,
    pub dimensions: (Decimal, Decimal, Decimal), // (length, width, height)
    pub dimension_unit: String,
    pub max_weight: Decimal,
    pub weight_unit: String,
    pub flat_rate: Option<Decimal>,
    pub flat_rate_currency: Option<String>,
    pub description: Option<String>,
}

impl PackageType {
    /// Create a new package type
    pub fn new(
        code: impl Into<String>,
        name: impl Into<String>,
        carrier: impl Into<String>,
        dimensions: (Decimal, Decimal, Decimal),
        dimension_unit: impl Into<String>,
        max_weight: Decimal,
        weight_unit: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            name: name.into(),
            carrier: carrier.into(),
            dimensions,
            dimension_unit: dimension_unit.into(),
            max_weight,
            weight_unit: weight_unit.into(),
            flat_rate: None,
            flat_rate_currency: None,
            description: None,
        }
    }

    /// Set flat rate
    pub fn with_flat_rate(mut self, rate: Decimal, currency: impl Into<String>) -> Self {
        self.flat_rate = Some(rate);
        self.flat_rate_currency = Some(currency.into());
        self
    }

    /// Get volume in cubic centimeters
    pub fn volume_cm3(&self) -> Decimal {
        let (l, w, h) = self.dimensions;
        match self.dimension_unit.as_str() {
            "cm" => l * w * h,
            "m" => l * w * h * Decimal::from(1_000_000),
            "in" => l * w * h * Decimal::from_str_exact("16.387").unwrap_or(Decimal::from(16)),
            "ft" => l * w * h * Decimal::from_str_exact("28316.8").unwrap_or(Decimal::from(28317)),
            _ => l * w * h,
        }
    }

    /// Check if an item fits in this package
    pub fn can_fit(&self, item_dimensions: (Decimal, Decimal, Decimal), unit: &str) -> bool {
        // Convert item dimensions to package's unit
        let (item_l, item_w, item_h) = item_dimensions;
        let (item_l_conv, item_w_conv, item_h_conv) = match (unit, self.dimension_unit.as_str()) {
            ("cm", "cm") => (item_l, item_w, item_h),
            ("m", "cm") => (
                item_l * Decimal::from(100),
                item_w * Decimal::from(100),
                item_h * Decimal::from(100),
            ),
            ("in", "cm") => (
                item_l * Decimal::from_str_exact("2.54").unwrap_or(Decimal::from(3)),
                item_w * Decimal::from_str_exact("2.54").unwrap_or(Decimal::from(3)),
                item_h * Decimal::from_str_exact("2.54").unwrap_or(Decimal::from(3)),
            ),
            ("cm", "in") => (
                item_l / Decimal::from_str_exact("2.54").unwrap_or(Decimal::from(3)),
                item_w / Decimal::from_str_exact("2.54").unwrap_or(Decimal::from(3)),
                item_h / Decimal::from_str_exact("2.54").unwrap_or(Decimal::from(3)),
            ),
            _ => (item_l, item_w, item_h),
        };

        let (pkg_l, pkg_w, pkg_h) = self.dimensions;

        // Simple check - item must fit within package dimensions
        // This is a simplified check - real implementation would check all orientations
        item_l_conv <= pkg_l && item_w_conv <= pkg_w && item_h_conv <= pkg_h
    }
}

/// Predefined package types registry
pub struct PackageRegistry {
    packages: HashMap<String, PackageType>,
}

impl PackageRegistry {
    /// Create a new registry with default package types
    pub fn new() -> Self {
        let mut registry = Self {
            packages: HashMap::new(),
        };
        registry.register_defaults();
        registry
    }

    /// Register a package type
    pub fn register(&mut self, package_type: PackageType) {
        self.packages
            .insert(package_type.code.clone(), package_type);
    }

    /// Get a package type by code
    pub fn get(&self, code: &str) -> Option<&PackageType> {
        self.packages.get(code)
    }

    /// Get all package types for a carrier
    pub fn get_by_carrier(&self, carrier: &str) -> Vec<&PackageType> {
        self.packages
            .values()
            .filter(|p| p.carrier.eq_ignore_ascii_case(carrier))
            .collect()
    }

    /// Get all flat rate packages
    pub fn get_flat_rate(&self) -> Vec<&PackageType> {
        self.packages
            .values()
            .filter(|p| p.flat_rate.is_some())
            .collect()
    }

    /// Register default package types
    fn register_defaults(&mut self) {
        // USPS Flat Rate
        self.register(
            PackageType::new(
                "usps_flat_rate_envelope",
                "USPS Flat Rate Envelope",
                "USPS",
                (Decimal::from(12), Decimal::from(9), Decimal::from(1)),
                "in",
                Decimal::from(70),
                "lb",
            )
            .with_flat_rate(Decimal::from(8), "USD"),
        );

        self.register(
            PackageType::new(
                "usps_medium_flat_rate_box",
                "USPS Medium Flat Rate Box",
                "USPS",
                (Decimal::from(11), Decimal::from(8), Decimal::from(6)),
                "in",
                Decimal::from(70),
                "lb",
            )
            .with_flat_rate(Decimal::from(16), "USD"),
        );

        self.register(
            PackageType::new(
                "usps_large_flat_rate_box",
                "USPS Large Flat Rate Box",
                "USPS",
                (Decimal::from(12), Decimal::from(12), Decimal::from(6)),
                "in",
                Decimal::from(70),
                "lb",
            )
            .with_flat_rate(Decimal::from(21), "USD"),
        );

        // UPS Express Boxes
        self.register(PackageType::new(
            "ups_express_box_small",
            "UPS Express Box Small",
            "UPS",
            (Decimal::from(13), Decimal::from(11), Decimal::from(2)),
            "in",
            Decimal::from(30),
            "lb",
        ));

        self.register(PackageType::new(
            "ups_express_box_medium",
            "UPS Express Box Medium",
            "UPS",
            (Decimal::from(16), Decimal::from(11), Decimal::from(3)),
            "in",
            Decimal::from(30),
            "lb",
        ));

        self.register(PackageType::new(
            "ups_express_box_large",
            "UPS Express Box Large",
            "UPS",
            (Decimal::from(18), Decimal::from(13), Decimal::from(3)),
            "in",
            Decimal::from(30),
            "lb",
        ));

        // FedEx Boxes
        self.register(PackageType::new(
            "fedex_small_box",
            "FedEx Small Box",
            "FedEx",
            (Decimal::from(12), Decimal::from(10), Decimal::from(6)),
            "in",
            Decimal::from(20),
            "lb",
        ));

        self.register(PackageType::new(
            "fedex_medium_box",
            "FedEx Medium Box",
            "FedEx",
            (Decimal::from(14), Decimal::from(12), Decimal::from(6)),
            "in",
            Decimal::from(20),
            "lb",
        ));

        self.register(PackageType::new(
            "fedex_large_box",
            "FedEx Large Box",
            "FedEx",
            (Decimal::from(16), Decimal::from(14), Decimal::from(6)),
            "in",
            Decimal::from(20),
            "lb",
        ));

        // DHL Boxes
        self.register(PackageType::new(
            "dhl_express_envelope",
            "DHL Express Envelope",
            "DHL",
            (Decimal::from(32), Decimal::from(24), Decimal::from(1)),
            "cm",
            Decimal::from(1),
            "kg",
        ));

        self.register(PackageType::new(
            "dhl_small_box",
            "DHL Small Box",
            "DHL",
            (Decimal::from(40), Decimal::from(30), Decimal::from(20)),
            "cm",
            Decimal::from(5),
            "kg",
        ));

        self.register(PackageType::new(
            "dhl_medium_box",
            "DHL Medium Box",
            "DHL",
            (Decimal::from(50), Decimal::from(40), Decimal::from(30)),
            "cm",
            Decimal::from(10),
            "kg",
        ));
    }
}

impl Default for PackageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Packaging calculator for order items
pub struct PackagingCalculator {
    registry: PackageRegistry,
}

impl PackagingCalculator {
    /// Create a new packaging calculator
    pub fn new() -> Self {
        Self {
            registry: PackageRegistry::new(),
        }
    }

    /// Calculate optimal packaging for items
    pub fn calculate_optimal_packaging(
        &self,
        items: &[ItemDimensions],
        max_weight_per_package: Decimal,
    ) -> Vec<PackageRecommendation> {
        let mut packages = Vec::new();
        let mut remaining_items: Vec<&ItemDimensions> = items.iter().collect();

        // Simple bin packing algorithm
        while !remaining_items.is_empty() {
            let mut current_package_items: Vec<&ItemDimensions> = Vec::new();
            let mut current_weight = Decimal::ZERO;

            // Try to fit items into a package
            remaining_items.retain(|item| {
                if current_weight + item.weight <= max_weight_per_package {
                    current_package_items.push(item);
                    current_weight += item.weight;
                    false // Remove from remaining
                } else {
                    true // Keep in remaining
                }
            });

            if current_package_items.is_empty() {
                // Can't fit any more items, force add one
                if let Some(item) = remaining_items.pop() {
                    current_package_items.push(item);
                }
            }

            // Calculate package dimensions
            let (length, width, height, weight) =
                self.calculate_package_dimensions(&current_package_items);

            packages.push(PackageRecommendation {
                length,
                width,
                height,
                weight,
                item_count: current_package_items.len(),
            });
        }

        packages
    }

    /// Find best flat rate option
    pub fn find_best_flat_rate(
        &self,
        items: &[ItemDimensions],
        carrier: Option<&str>,
    ) -> Option<&PackageType> {
        let total_weight: Decimal = items.iter().map(|i| i.weight).sum();

        // Get max dimensions
        let max_length = items.iter().map(|i| i.length).max()?;
        let max_width = items.iter().map(|i| i.width).max()?;
        let max_height: Decimal = items.iter().map(|i| i.height).sum();

        let candidates: Vec<&PackageType> = if let Some(c) = carrier {
            self.registry.get_by_carrier(c)
        } else {
            self.registry.get_flat_rate()
        };

        candidates
            .into_iter()
            .filter(|p| {
                // Check weight limit
                let weight_ok = match (p.weight_unit.as_str(), "kg") {
                    ("lb", "kg") => {
                        total_weight
                            <= p.max_weight
                                * Decimal::from_str_exact("0.453592")
                                    .unwrap_or(Decimal::from(5) / Decimal::from(11))
                    }
                    ("kg", "kg") => total_weight <= p.max_weight,
                    _ => true, // Assume ok if units don't match
                };

                // Check dimensions (simplified)
                let dims_ok = p.can_fit((max_length, max_width, max_height), "cm");

                weight_ok && dims_ok && p.flat_rate.is_some()
            })
            .min_by_key(|p| p.flat_rate.unwrap_or(Decimal::MAX))
    }

    fn calculate_package_dimensions(
        &self,
        items: &[&ItemDimensions],
    ) -> (Decimal, Decimal, Decimal, Decimal) {
        let mut length = Decimal::ZERO;
        let mut width = Decimal::ZERO;
        let mut height = Decimal::ZERO;
        let mut weight = Decimal::ZERO;

        for item in items {
            length = length.max(item.length);
            width = width.max(item.width);
            height += item.height;
            weight += item.weight;
        }

        // Add padding for packaging materials
        let padding = Decimal::from_str_exact("1.5").unwrap_or(Decimal::from(2)); // 1.5cm padding
        (length + padding, width + padding, height + padding, weight)
    }
}

impl Default for PackagingCalculator {
    fn default() -> Self {
        Self::new()
    }
}

/// Item dimensions for packaging calculation
#[derive(Debug, Clone)]
pub struct ItemDimensions {
    pub length: Decimal,
    pub width: Decimal,
    pub height: Decimal,
    pub weight: Decimal,
    pub quantity: i32,
}

/// Package recommendation
#[derive(Debug, Clone)]
pub struct PackageRecommendation {
    pub length: Decimal,
    pub width: Decimal,
    pub height: Decimal,
    pub weight: Decimal,
    pub item_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_volume() {
        let pkg = Package::new(Decimal::from(5), "kg").with_dimensions(
            Decimal::from(50),
            Decimal::from(40),
            Decimal::from(30),
            "cm",
        );

        assert_eq!(pkg.volume_cm3(), Some(Decimal::from(60000)));
    }

    #[test]
    fn test_package_girth() {
        let pkg = Package::new(Decimal::from(5), "kg").with_dimensions(
            Decimal::from(50),
            Decimal::from(40),
            Decimal::from(30),
            "cm",
        );

        // Girth = 2 * (40 + 30) = 140cm
        assert_eq!(pkg.girth_cm(), Some(Decimal::from(140)));
    }

    #[test]
    fn test_package_type_volume() {
        let pkg_type = PackageType::new(
            "test",
            "Test Box",
            "TEST",
            (Decimal::from(10), Decimal::from(10), Decimal::from(10)),
            "cm",
            Decimal::from(10),
            "kg",
        );

        assert_eq!(pkg_type.volume_cm3(), Decimal::from(1000));
    }

    #[test]
    fn test_registry() {
        let registry = PackageRegistry::new();

        // Check USPS flat rate exists
        let usps = registry.get("usps_medium_flat_rate_box");
        assert!(usps.is_some());

        let usps = usps.unwrap();
        assert_eq!(usps.carrier, "USPS");
        assert!(usps.flat_rate.is_some());

        // Check by carrier
        let usps_packages = registry.get_by_carrier("USPS");
        assert!(!usps_packages.is_empty());
    }
}

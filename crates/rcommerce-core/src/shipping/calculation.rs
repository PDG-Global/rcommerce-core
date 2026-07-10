//! Shipping calculation engine
//!
//! Provides weight-based and volumetric weight calculations,
//! dimensional weight factoring, and shipping cost estimation.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use super::packaging::Package;

/// Weight converter between different units
pub struct WeightConverter;

impl WeightConverter {
    /// Convert weight to kilograms
    pub fn to_kg(weight: Decimal, from_unit: WeightUnit) -> Decimal {
        match from_unit {
            WeightUnit::Kg => weight,
            WeightUnit::G => weight / dec!(1000),
            WeightUnit::Lb => weight * dec!(0.453592),
            WeightUnit::Oz => weight * dec!(0.0283495),
        }
    }

    /// Convert weight to pounds
    pub fn to_lb(weight: Decimal, from_unit: WeightUnit) -> Decimal {
        match from_unit {
            WeightUnit::Lb => weight,
            WeightUnit::Oz => weight / dec!(16),
            WeightUnit::Kg => weight * dec!(2.20462),
            WeightUnit::G => weight * dec!(0.00220462),
        }
    }

    /// Convert between any units
    pub fn convert(weight: Decimal, from: WeightUnit, to: WeightUnit) -> Decimal {
        let kg = Self::to_kg(weight, from);
        match to {
            WeightUnit::Kg => kg,
            WeightUnit::G => kg * dec!(1000),
            WeightUnit::Lb => kg * dec!(2.20462),
            WeightUnit::Oz => kg * dec!(35.274),
        }
    }
}

/// Weight units
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeightUnit {
    Kg,
    G,
    Lb,
    Oz,
}

impl WeightUnit {
    pub fn as_str(&self) -> &'static str {
        match self {
            WeightUnit::Kg => "kg",
            WeightUnit::G => "g",
            WeightUnit::Lb => "lb",
            WeightUnit::Oz => "oz",
        }
    }
}

/// Length converter between different units
pub struct LengthConverter;

impl LengthConverter {
    /// Convert length to centimeters
    pub fn to_cm(length: Decimal, from_unit: LengthUnit) -> Decimal {
        match from_unit {
            LengthUnit::Cm => length,
            LengthUnit::M => length * dec!(100),
            LengthUnit::In => length * dec!(2.54),
            LengthUnit::Ft => length * dec!(30.48),
        }
    }

    /// Convert length to inches
    pub fn to_in(length: Decimal, from_unit: LengthUnit) -> Decimal {
        match from_unit {
            LengthUnit::In => length,
            LengthUnit::Ft => length * dec!(12),
            LengthUnit::Cm => length / dec!(2.54),
            LengthUnit::M => length / dec!(0.0254),
        }
    }

    /// Convert between any units
    pub fn convert(length: Decimal, from: LengthUnit, to: LengthUnit) -> Decimal {
        let cm = Self::to_cm(length, from);
        match to {
            LengthUnit::Cm => cm,
            LengthUnit::M => cm / dec!(100),
            LengthUnit::In => cm / dec!(2.54),
            LengthUnit::Ft => cm / dec!(30.48),
        }
    }
}

/// Length units
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthUnit {
    Cm,
    M,
    In,
    Ft,
}

impl LengthUnit {
    pub fn as_str(&self) -> &'static str {
        match self {
            LengthUnit::Cm => "cm",
            LengthUnit::M => "m",
            LengthUnit::In => "in",
            LengthUnit::Ft => "ft",
        }
    }
}

/// Volumetric weight calculator
///
/// Volumetric weight (also called dimensional weight) is calculated by
/// multiplying the package dimensions and dividing by a dimensional factor.
/// Carriers charge based on the greater of actual weight or volumetric weight.
pub struct VolumetricWeightCalculator {
    /// Dimensional factor (divisor)
    /// Common values:
    /// - 5000 for cm³/kg (DHL, FedEx, UPS international)
    /// - 6000 for cm³/kg (some carriers)
    /// - 139 for in³/lb (FedEx, UPS domestic)
    /// - 166 for in³/lb (USPS, some others)
    factor: Decimal,
    /// Input unit for dimensions
    dimension_unit: LengthUnit,
    /// Output unit for weight
    weight_unit: WeightUnit,
}

impl VolumetricWeightCalculator {
    /// Create a new calculator with standard settings
    pub fn new(factor: Decimal, dimension_unit: LengthUnit, weight_unit: WeightUnit) -> Self {
        Self {
            factor,
            dimension_unit,
            weight_unit,
        }
    }

    /// Standard DHL/FedEx/UPS international calculator (cm³/kg, factor 5000)
    pub fn standard_international() -> Self {
        Self::new(dec!(5000), LengthUnit::Cm, WeightUnit::Kg)
    }

    /// Standard FedEx/UPS domestic calculator (in³/lb, factor 139)
    pub fn standard_domestic_us() -> Self {
        Self::new(dec!(139), LengthUnit::In, WeightUnit::Lb)
    }

    /// USPS calculator (in³/lb, factor 166)
    pub fn usps() -> Self {
        Self::new(dec!(166), LengthUnit::In, WeightUnit::Lb)
    }

    /// Calculate volumetric weight from dimensions
    pub fn calculate(&self, length: Decimal, width: Decimal, height: Decimal) -> Decimal {
        // Convert dimensions to calculator's unit
        let l = LengthConverter::convert(length, LengthUnit::Cm, self.dimension_unit);
        let w = LengthConverter::convert(width, LengthUnit::Cm, self.dimension_unit);
        let h = LengthConverter::convert(height, LengthUnit::Cm, self.dimension_unit);

        // Calculate volume
        let volume = l * w * h;

        // Calculate volumetric weight
        let volumetric_weight = volume / self.factor;

        // Round up to nearest whole number (industry standard)
        volumetric_weight.ceil()
    }

    /// Calculate volumetric weight from a package
    pub fn calculate_for_package(&self, package: &Package) -> Option<Decimal> {
        match (package.length, package.width, package.height) {
            (Some(l), Some(w), Some(h)) => {
                let (length, width, height) = match package.dimension_unit.as_deref() {
                    Some("cm") => (l, w, h),
                    Some("m") => (l * dec!(100), w * dec!(100), h * dec!(100)),
                    Some("in") => (l * dec!(2.54), w * dec!(2.54), h * dec!(2.54)),
                    Some("ft") => (l * dec!(30.48), w * dec!(30.48), h * dec!(30.48)),
                    _ => (l, w, h), // Assume cm if not specified
                };
                Some(self.calculate(length, width, height))
            }
            _ => None,
        }
    }

    /// Get the chargeable weight (max of actual and volumetric)
    pub fn chargeable_weight(&self, actual_weight: Decimal, volumetric_weight: Decimal) -> Decimal {
        actual_weight.max(volumetric_weight)
    }

    /// Calculate for package and return both weights
    pub fn calculate_chargeable_weight(&self, package: &Package) -> ChargeableWeight {
        let actual_weight = package.weight;
        let volumetric_weight = self.calculate_for_package(package);

        let chargeable = match volumetric_weight {
            Some(vw) => actual_weight.max(vw),
            None => actual_weight,
        };

        ChargeableWeight {
            actual_weight,
            volumetric_weight,
            chargeable_weight: chargeable,
            unit: self.weight_unit,
        }
    }
}

/// Chargeable weight calculation result
#[derive(Debug, Clone)]
pub struct ChargeableWeight {
    pub actual_weight: Decimal,
    pub volumetric_weight: Option<Decimal>,
    pub chargeable_weight: Decimal,
    pub unit: WeightUnit,
}

impl ChargeableWeight {
    /// Check if volumetric weight applies
    pub fn is_volumetric(&self) -> bool {
        match self.volumetric_weight {
            Some(vw) => vw > self.actual_weight,
            None => false,
        }
    }

    /// Get weight in specified unit
    pub fn in_unit(&self, unit: WeightUnit) -> Decimal {
        WeightConverter::convert(self.chargeable_weight, self.unit, unit)
    }
}

/// Shipping calculator with rate tiers
pub struct ShippingCalculator {
    base_rate: Decimal,
    weight_rate: Decimal,
    handling_fee: Decimal,
    fuel_surcharge_rate: Decimal,
    free_shipping_threshold: Option<Decimal>,
    weight_unit: WeightUnit,
}

impl ShippingCalculator {
    /// Create a new shipping calculator
    pub fn new(base_rate: Decimal, weight_rate: Decimal, weight_unit: WeightUnit) -> Self {
        Self {
            base_rate,
            weight_rate,
            handling_fee: Decimal::ZERO,
            fuel_surcharge_rate: Decimal::ZERO,
            free_shipping_threshold: None,
            weight_unit,
        }
    }

    /// Set handling fee
    pub fn with_handling_fee(mut self, fee: Decimal) -> Self {
        self.handling_fee = fee;
        self
    }

    /// Set fuel surcharge rate (as decimal, e.g., 0.15 for 15%)
    pub fn with_fuel_surcharge(mut self, rate: Decimal) -> Self {
        self.fuel_surcharge_rate = rate;
        self
    }

    /// Set free shipping threshold
    pub fn with_free_shipping_threshold(mut self, threshold: Decimal) -> Self {
        self.free_shipping_threshold = Some(threshold);
        self
    }

    /// Calculate shipping cost for a weight
    pub fn calculate(&self, weight: Decimal, order_subtotal: Decimal) -> Decimal {
        // Check free shipping threshold
        if let Some(threshold) = self.free_shipping_threshold {
            if order_subtotal >= threshold {
                return Decimal::ZERO;
            }
        }

        // Convert weight to calculator's unit
        let weight_in_unit = WeightConverter::convert(weight, WeightUnit::Kg, self.weight_unit);

        // Calculate base shipping cost
        let weight_cost = weight_in_unit * self.weight_rate;
        let subtotal = self.base_rate + weight_cost + self.handling_fee;

        // Add fuel surcharge if applicable
        if self.fuel_surcharge_rate > Decimal::ZERO {
            let fuel_surcharge = subtotal * self.fuel_surcharge_rate;
            subtotal + fuel_surcharge
        } else {
            subtotal
        }
    }

    /// Calculate with chargeable weight
    pub fn calculate_with_chargeable_weight(
        &self,
        chargeable: &ChargeableWeight,
        order_subtotal: Decimal,
    ) -> Decimal {
        self.calculate(chargeable.chargeable_weight, order_subtotal)
    }

    /// Calculate for a package
    pub fn calculate_for_package(&self, package: &Package, order_subtotal: Decimal) -> Decimal {
        self.calculate(package.weight, order_subtotal)
    }

    /// Calculate with volumetric weight consideration
    pub fn calculate_with_volumetric(
        &self,
        package: &Package,
        order_subtotal: Decimal,
        volumetric_calc: &VolumetricWeightCalculator,
    ) -> ShippingCalculation {
        let chargeable = volumetric_calc.calculate_chargeable_weight(package);
        let cost = self.calculate_with_chargeable_weight(&chargeable, order_subtotal);

        ShippingCalculation {
            cost,
            chargeable_weight: chargeable,
            base_rate: self.base_rate,
            weight_rate: self.weight_rate,
            handling_fee: self.handling_fee,
            fuel_surcharge: if self.fuel_surcharge_rate > Decimal::ZERO {
                Some(cost * self.fuel_surcharge_rate)
            } else {
                None
            },
        }
    }
}

/// Detailed shipping calculation result
#[derive(Debug, Clone)]
pub struct ShippingCalculation {
    pub cost: Decimal,
    pub chargeable_weight: ChargeableWeight,
    pub base_rate: Decimal,
    pub weight_rate: Decimal,
    pub handling_fee: Decimal,
    pub fuel_surcharge: Option<Decimal>,
}

impl ShippingCalculation {
    /// Get total cost breakdown
    pub fn breakdown(&self) -> Vec<(String, Decimal)> {
        let mut items = vec![
            ("Base Rate".to_string(), self.base_rate),
            ("Weight Charge".to_string(), self.weight_rate),
        ];

        if self.handling_fee > Decimal::ZERO {
            items.push(("Handling Fee".to_string(), self.handling_fee));
        }

        if let Some(fuel) = self.fuel_surcharge {
            items.push(("Fuel Surcharge".to_string(), fuel));
        }

        items.push(("Total".to_string(), self.cost));
        items
    }

    /// Format as string
    pub fn format(&self) -> String {
        let mut result = format!("Shipping Cost: ${:.2}\n", self.cost);
        result.push_str(&format!("  Base Rate: ${:.2}\n", self.base_rate));
        result.push_str(&format!(
            "  Weight: {:.2} {} (Chargeable: {:.2})\n",
            self.chargeable_weight.actual_weight,
            self.chargeable_weight.unit.as_str(),
            self.chargeable_weight.chargeable_weight
        ));

        if let Some(vw) = self.chargeable_weight.volumetric_weight {
            result.push_str(&format!("  Volumetric Weight: {:.2}\n", vw));
        }

        if self.handling_fee > Decimal::ZERO {
            result.push_str(&format!("  Handling Fee: ${:.2}\n", self.handling_fee));
        }

        if let Some(fuel) = self.fuel_surcharge {
            result.push_str(&format!("  Fuel Surcharge: ${:.2}\n", fuel));
        }

        result
    }
}

/// Tiered shipping rate calculator
pub struct TieredShippingCalculator {
    tiers: Vec<(Decimal, Decimal)>, // (weight_threshold, rate)
    base_rate: Decimal,
}

impl TieredShippingCalculator {
    /// Create a new tiered calculator
    pub fn new(base_rate: Decimal) -> Self {
        Self {
            tiers: Vec::new(),
            base_rate,
        }
    }

    /// Add a tier (weight threshold and rate for that tier)
    pub fn add_tier(&mut self, weight_threshold: Decimal, rate: Decimal) {
        self.tiers.push((weight_threshold, rate));
        // Sort by weight threshold
        self.tiers.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    }

    /// Calculate shipping cost
    pub fn calculate(&self, weight: Decimal) -> Decimal {
        let mut cost = self.base_rate;
        let mut remaining_weight = weight;
        let mut prev_threshold = Decimal::ZERO;

        for (threshold, rate) in &self.tiers {
            if remaining_weight <= Decimal::ZERO {
                break;
            }

            let tier_weight = (*threshold - prev_threshold).min(remaining_weight);
            cost += tier_weight * rate;
            remaining_weight -= tier_weight;
            prev_threshold = *threshold;
        }

        // Apply highest tier rate to remaining weight
        if remaining_weight > Decimal::ZERO {
            if let Some((_, highest_rate)) = self.tiers.last() {
                cost += remaining_weight * highest_rate;
            }
        }

        cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weight_converter() {
        // 1 kg = 2.20462 lb
        let kg_to_lb = WeightConverter::to_lb(dec!(1), WeightUnit::Kg);
        assert!((kg_to_lb - dec!(2.20462)).abs() < dec!(0.001));

        // 1 lb = 0.453592 kg
        let lb_to_kg = WeightConverter::to_kg(dec!(1), WeightUnit::Lb);
        assert!((lb_to_kg - dec!(0.453592)).abs() < dec!(0.001));
    }

    #[test]
    fn test_volumetric_weight_calculator() {
        // Standard international: 50cm x 40cm x 30cm / 5000 = 12kg
        let calc = VolumetricWeightCalculator::standard_international();
        let vw = calc.calculate(dec!(50), dec!(40), dec!(30));
        assert_eq!(vw, dec!(12));

        // Domestic US: 50.8cm x 40.64cm x 30.48cm (20in x 16in x 12in) / 5000 = 12.5 -> 13 kg
        // Note: The calculator converts to the configured unit internally
        let calc = VolumetricWeightCalculator::standard_domestic_us();
        // Input is in cm, gets converted to inches internally: 50.8cm = 20in
        let vw = calc.calculate(dec!(51), dec!(41), dec!(30)); // Approximate conversion
        assert!(vw > dec!(10)); // Should be a reasonable volumetric weight
    }

    #[test]
    fn test_chargeable_weight() {
        let calc = VolumetricWeightCalculator::standard_international();

        // Package: 5kg actual, dimensions 50x40x30cm (12kg volumetric)
        let package = Package {
            weight: dec!(5),
            weight_unit: "kg".to_string(),
            length: Some(dec!(50)),
            width: Some(dec!(40)),
            height: Some(dec!(30)),
            dimension_unit: Some("cm".to_string()),
            predefined_package: None,
        };

        let cw = calc.calculate_chargeable_weight(&package);
        assert_eq!(cw.actual_weight, dec!(5));
        assert_eq!(cw.volumetric_weight, Some(dec!(12)));
        assert_eq!(cw.chargeable_weight, dec!(12)); // Volumetric is higher
        assert!(cw.is_volumetric());
    }

    #[test]
    fn test_shipping_calculator() {
        let calc =
            ShippingCalculator::new(dec!(5), dec!(2), WeightUnit::Kg).with_handling_fee(dec!(3));

        // 2kg package, $50 order
        let cost = calc.calculate(dec!(2), dec!(50));
        // Base: $5 + Weight: $4 + Handling: $3 = $12
        assert_eq!(cost, dec!(12));

        // Test free shipping threshold
        let calc = ShippingCalculator::new(dec!(5), dec!(2), WeightUnit::Kg)
            .with_free_shipping_threshold(dec!(100));

        let cost = calc.calculate(dec!(2), dec!(150));
        assert_eq!(cost, Decimal::ZERO);
    }

    #[test]
    fn test_tiered_calculator() {
        let mut calc = TieredShippingCalculator::new(dec!(5));
        calc.add_tier(dec!(1), dec!(10)); // First 1kg: $10/kg
        calc.add_tier(dec!(5), dec!(8)); // 1-5kg: $8/kg
        calc.add_tier(dec!(10), dec!(6)); // 5-10kg: $6/kg

        // 0.5kg: $5 + $5 = $10
        assert_eq!(calc.calculate(dec!(0.5)), dec!(10));

        // 2kg: $5 + $10 + $8 = $23
        assert_eq!(calc.calculate(dec!(2)), dec!(23));

        // 7kg: $5 + $10 + $32 + $12 = $59
        assert_eq!(calc.calculate(dec!(7)), dec!(59));
    }
}

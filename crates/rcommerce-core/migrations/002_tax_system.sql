-- Tax System Migration
-- Creates tables for comprehensive tax management including VAT, sales tax, and OSS reporting

-- Tax zones (countries, states, cities, postal code ranges)
CREATE TABLE IF NOT EXISTS tax_zones (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    code VARCHAR(20) UNIQUE NOT NULL,
    country_code CHAR(2) NOT NULL,
    region_code VARCHAR(10), -- State/Province code
    postal_code_pattern VARCHAR(50), -- Regex pattern for postal codes
    zone_type VARCHAR(20) NOT NULL DEFAULT 'country', -- 'country', 'state', 'city', 'postal_code', 'custom'
    parent_id UUID REFERENCES tax_zones(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tax_zones_country ON tax_zones(country_code);
CREATE INDEX IF NOT EXISTS idx_tax_zones_region ON tax_zones(country_code, region_code);
CREATE INDEX IF NOT EXISTS idx_tax_zones_type ON tax_zones(zone_type);

-- Tax categories (food, digital, luxury, medical, etc.)
CREATE TABLE IF NOT EXISTS tax_categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    code VARCHAR(50) UNIQUE NOT NULL,
    description TEXT,
    is_digital BOOLEAN NOT NULL DEFAULT false,
    is_food BOOLEAN NOT NULL DEFAULT false,
    is_luxury BOOLEAN NOT NULL DEFAULT false,
    is_medical BOOLEAN NOT NULL DEFAULT false,
    is_educational BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tax_categories_code ON tax_categories(code);

-- Tax rates
CREATE TABLE IF NOT EXISTS tax_rates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    tax_zone_id UUID NOT NULL REFERENCES tax_zones(id) ON DELETE CASCADE,
    tax_category_id UUID REFERENCES tax_categories(id) ON DELETE SET NULL,
    
    -- Rate details
    rate DECIMAL(5,4) NOT NULL CHECK (rate >= 0 AND rate <= 1),
    rate_type VARCHAR(20) NOT NULL DEFAULT 'percentage', -- 'percentage', 'fixed'
    
    -- VAT specific fields
    is_vat BOOLEAN NOT NULL DEFAULT false,
    vat_type VARCHAR(20), -- 'standard', 'reduced', 'super_reduced', 'zero', 'exempt'
    
    -- B2B rules
    b2b_exempt BOOLEAN NOT NULL DEFAULT false,
    reverse_charge BOOLEAN NOT NULL DEFAULT false,
    
    -- Validity period
    valid_from DATE NOT NULL DEFAULT CURRENT_DATE,
    valid_until DATE,
    
    -- Priority for overlapping zones (higher = more specific)
    priority INTEGER NOT NULL DEFAULT 0,
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    UNIQUE(tax_zone_id, tax_category_id, valid_from)
);

CREATE INDEX IF NOT EXISTS idx_tax_rates_zone ON tax_rates(tax_zone_id);
CREATE INDEX IF NOT EXISTS idx_tax_rates_category ON tax_rates(tax_category_id);
CREATE INDEX IF NOT EXISTS idx_tax_rates_valid ON tax_rates(valid_from, valid_until);
CREATE INDEX IF NOT EXISTS idx_tax_rates_vat ON tax_rates(is_vat, vat_type);

-- VAT ID validation cache
CREATE TABLE IF NOT EXISTS vat_id_validations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    vat_id VARCHAR(50) UNIQUE NOT NULL,
    country_code CHAR(2) NOT NULL,
    business_name VARCHAR(255),
    business_address TEXT,
    is_valid BOOLEAN NOT NULL,
    validated_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    raw_response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_vat_validations_id ON vat_id_validations(vat_id);
CREATE INDEX IF NOT EXISTS idx_vat_validations_expires ON vat_id_validations(expires_at);

-- Tax exemptions (resale certificates, nonprofit status, etc.)
CREATE TABLE IF NOT EXISTS tax_exemptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    customer_id UUID NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    tax_zone_id UUID REFERENCES tax_zones(id) ON DELETE CASCADE,
    exemption_type VARCHAR(50) NOT NULL, -- 'resale', 'nonprofit', 'government', 'diplomatic', 'educational', 'medical', 'other'
    exemption_number VARCHAR(100),
    document_url TEXT,
    valid_from DATE NOT NULL,
    valid_until DATE,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tax_exemptions_customer ON tax_exemptions(customer_id);
CREATE INDEX IF NOT EXISTS idx_tax_exemptions_zone ON tax_exemptions(tax_zone_id);
CREATE INDEX IF NOT EXISTS idx_tax_exemptions_active ON tax_exemptions(is_active, valid_from, valid_until);

-- Tax transactions (for reporting and audit trail)
CREATE TABLE IF NOT EXISTS tax_transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    order_item_id UUID REFERENCES order_items(id) ON DELETE SET NULL,
    
    -- Tax details
    tax_rate_id UUID REFERENCES tax_rates(id),
    tax_zone_id UUID REFERENCES tax_zones(id),
    tax_category_id UUID REFERENCES tax_categories(id),
    
    -- Amounts
    taxable_amount DECIMAL(19,4) NOT NULL,
    tax_amount DECIMAL(19,4) NOT NULL,
    tax_rate DECIMAL(5,4) NOT NULL,
    
    -- Jurisdiction
    country_code CHAR(2) NOT NULL,
    region_code VARCHAR(10),
    
    -- OSS reporting
    oss_scheme VARCHAR(20), -- 'union', 'non_union', 'import'
    oss_period VARCHAR(7), -- 'YYYY-MM'
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tax_transactions_order ON tax_transactions(order_id);
CREATE INDEX IF NOT EXISTS idx_tax_transactions_country ON tax_transactions(country_code, region_code);
CREATE INDEX IF NOT EXISTS idx_tax_transactions_oss ON tax_transactions(oss_scheme, oss_period);
CREATE INDEX IF NOT EXISTS idx_tax_transactions_created ON tax_transactions(created_at);

-- Economic nexus tracking (for US sales tax)
CREATE TABLE IF NOT EXISTS economic_nexus_tracking (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    country_code CHAR(2) NOT NULL,
    region_code VARCHAR(10) NOT NULL, -- State code for US
    year INTEGER NOT NULL,
    month INTEGER NOT NULL,
    total_sales DECIMAL(19,4) NOT NULL DEFAULT 0,
    total_transactions INTEGER NOT NULL DEFAULT 0,
    taxable_sales DECIMAL(19,4) NOT NULL DEFAULT 0,
    nexus_triggered BOOLEAN NOT NULL DEFAULT false,
    threshold_amount DECIMAL(19,4),
    threshold_transactions INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    UNIQUE(country_code, region_code, year, month)
);

CREATE INDEX IF NOT EXISTS idx_nexus_tracking_region ON economic_nexus_tracking(country_code, region_code);
CREATE INDEX IF NOT EXISTS idx_nexus_tracking_period ON economic_nexus_tracking(year, month);
CREATE INDEX IF NOT EXISTS idx_nexus_tracking_triggered ON economic_nexus_tracking(nexus_triggered);

-- Add tax_category_id to products
ALTER TABLE products ADD COLUMN IF NOT EXISTS tax_category_id UUID REFERENCES tax_categories(id);
CREATE INDEX IF NOT EXISTS idx_products_tax_category ON products(tax_category_id);

-- Add VAT ID to customers
ALTER TABLE customers ADD COLUMN IF NOT EXISTS vat_id VARCHAR(50);
ALTER TABLE customers ADD COLUMN IF NOT EXISTS vat_id_validated_at TIMESTAMPTZ;
ALTER TABLE customers ADD COLUMN IF NOT EXISTS vat_id_is_valid BOOLEAN;
CREATE INDEX IF NOT EXISTS idx_customers_vat_id ON customers(vat_id);

-- Insert default tax zones for major markets

-- EU Countries
INSERT INTO tax_zones (name, code, country_code, zone_type) VALUES
('Austria', 'AT', 'AT', 'country'),
('Belgium', 'BE', 'BE', 'country'),
('Bulgaria', 'BG', 'BG', 'country'),
('Croatia', 'HR', 'HR', 'country'),
('Cyprus', 'CY', 'CY', 'country'),
('Czech Republic', 'CZ', 'CZ', 'country'),
('Denmark', 'DK', 'DK', 'country'),
('Estonia', 'EE', 'EE', 'country'),
('Finland', 'FI', 'FI', 'country'),
('France', 'FR', 'FR', 'country'),
('Germany', 'DE', 'DE', 'country'),
('Greece', 'GR', 'GR', 'country'),
('Hungary', 'HU', 'HU', 'country'),
('Ireland', 'IE', 'IE', 'country'),
('Italy', 'IT', 'IT', 'country'),
('Latvia', 'LV', 'LV', 'country'),
('Lithuania', 'LT', 'LT', 'country'),
('Luxembourg', 'LU', 'LU', 'country'),
('Malta', 'MT', 'MT', 'country'),
('Netherlands', 'NL', 'NL', 'country'),
('Poland', 'PL', 'PL', 'country'),
('Portugal', 'PT', 'PT', 'country'),
('Romania', 'RO', 'RO', 'country'),
('Slovakia', 'SK', 'SK', 'country'),
('Slovenia', 'SI', 'SI', 'country'),
('Spain', 'ES', 'ES', 'country'),
('Sweden', 'SE', 'SE', 'country')
ON CONFLICT (code) DO NOTHING;

-- US States (major ones)
INSERT INTO tax_zones (name, code, country_code, region_code, zone_type) VALUES
('California', 'US-CA', 'US', 'CA', 'state'),
('New York', 'US-NY', 'US', 'NY', 'state'),
('Texas', 'US-TX', 'US', 'TX', 'state'),
('Florida', 'US-FL', 'US', 'FL', 'state'),
('Illinois', 'US-IL', 'US', 'IL', 'state'),
('Pennsylvania', 'US-PA', 'US', 'PA', 'state'),
('Ohio', 'US-OH', 'US', 'OH', 'state'),
('Georgia', 'US-GA', 'US', 'GA', 'state'),
('North Carolina', 'US-NC', 'US', 'NC', 'state'),
('Michigan', 'US-MI', 'US', 'MI', 'state')
ON CONFLICT (code) DO NOTHING;

-- Other major markets
INSERT INTO tax_zones (name, code, country_code, zone_type) VALUES
('United Kingdom', 'GB', 'GB', 'country'),
('Switzerland', 'CH', 'CH', 'country'),
('Norway', 'NO', 'NO', 'country'),
('Canada', 'CA', 'CA', 'country'),
('Australia', 'AU', 'AU', 'country'),
('Japan', 'JP', 'JP', 'country'),
('Singapore', 'SG', 'SG', 'country')
ON CONFLICT (code) DO NOTHING;

-- Insert default tax categories
INSERT INTO tax_categories (name, code, description, is_digital, is_food, is_luxury, is_medical, is_educational) VALUES
('Standard', 'standard', 'Standard taxable goods and services', false, false, false, false, false),
('Digital Goods', 'digital', 'Digital products and services', true, false, false, false, false),
('Food', 'food', 'Food and beverages', false, true, false, false, false),
('Luxury', 'luxury', 'Luxury items with higher tax rates', false, false, true, false, false),
('Medical', 'medical', 'Medical supplies and services', false, false, false, true, false),
('Educational', 'educational', 'Educational materials and services', false, false, false, false, true),
('Zero Rated', 'zero_rated', 'Zero-rated items', false, false, false, false, false),
('Exempt', 'exempt', 'Tax exempt items', false, false, false, false, false)
ON CONFLICT (code) DO NOTHING;

-- Insert sample EU VAT rates (2026)
-- These are the standard rates; reduced rates would need separate entries
INSERT INTO tax_rates (name, tax_zone_id, rate, is_vat, vat_type, valid_from, priority)
SELECT 'Standard VAT', id, 0.20, true, 'standard', '2020-01-01', 0
FROM tax_zones WHERE code = 'FR'
ON CONFLICT DO NOTHING;

INSERT INTO tax_rates (name, tax_zone_id, rate, is_vat, vat_type, valid_from, priority)
SELECT 'Standard VAT', id, 0.19, true, 'standard', '2020-01-01', 0
FROM tax_zones WHERE code = 'DE'
ON CONFLICT DO NOTHING;

INSERT INTO tax_rates (name, tax_zone_id, rate, is_vat, vat_type, valid_from, priority)
SELECT 'Standard VAT', id, 0.20, true, 'standard', '2020-01-01', 0
FROM tax_zones WHERE code = 'IT'
ON CONFLICT DO NOTHING;

INSERT INTO tax_rates (name, tax_zone_id, rate, is_vat, vat_type, valid_from, priority)
SELECT 'Standard VAT', id, 0.21, true, 'standard', '2020-01-01', 0
FROM tax_zones WHERE code = 'ES'
ON CONFLICT DO NOTHING;

INSERT INTO tax_rates (name, tax_zone_id, rate, is_vat, vat_type, valid_from, priority)
SELECT 'Standard VAT', id, 0.20, true, 'standard', '2020-01-01', 0
FROM tax_zones WHERE code = 'NL'
ON CONFLICT DO NOTHING;

-- US State sales tax rates (simplified - 2026)
INSERT INTO tax_rates (name, tax_zone_id, rate, valid_from, priority)
SELECT 'California Sales Tax', id, 0.0725, '2020-01-01', 0
FROM tax_zones WHERE code = 'US-CA'
ON CONFLICT DO NOTHING;

INSERT INTO tax_rates (name, tax_zone_id, rate, valid_from, priority)
SELECT 'New York Sales Tax', id, 0.04, '2020-01-01', 0
FROM tax_zones WHERE code = 'US-NY'
ON CONFLICT DO NOTHING;

INSERT INTO tax_rates (name, tax_zone_id, rate, valid_from, priority)
SELECT 'Texas Sales Tax', id, 0.0625, '2020-01-01', 0
FROM tax_zones WHERE code = 'US-TX'
ON CONFLICT DO NOTHING;

-- Create function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_tax_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create triggers for updated_at
DROP TRIGGER IF EXISTS update_tax_zones_updated_at ON tax_zones;
CREATE TRIGGER update_tax_zones_updated_at
    BEFORE UPDATE ON tax_zones
    FOR EACH ROW EXECUTE FUNCTION update_tax_updated_at();

DROP TRIGGER IF EXISTS update_tax_categories_updated_at ON tax_categories;
CREATE TRIGGER update_tax_categories_updated_at
    BEFORE UPDATE ON tax_categories
    FOR EACH ROW EXECUTE FUNCTION update_tax_updated_at();

DROP TRIGGER IF EXISTS update_tax_rates_updated_at ON tax_rates;
CREATE TRIGGER update_tax_rates_updated_at
    BEFORE UPDATE ON tax_rates
    FOR EACH ROW EXECUTE FUNCTION update_tax_updated_at();

DROP TRIGGER IF EXISTS update_economic_nexus_tracking_updated_at ON economic_nexus_tracking;
CREATE TRIGGER update_economic_nexus_tracking_updated_at
    BEFORE UPDATE ON economic_nexus_tracking
    FOR EACH ROW EXECUTE FUNCTION update_tax_updated_at();

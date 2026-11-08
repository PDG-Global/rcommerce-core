-- Catalogue sync: a catalogue database publishes its product and category
-- changes; stores copy the products of their assortment from it.

-- Publishing switch (one row). Off by default: the triggers below then return
-- at once, so stores that do not publish a catalogue log nothing.
CREATE TABLE IF NOT EXISTS catalogue_settings (
    id BOOLEAN PRIMARY KEY DEFAULT true CHECK (id),
    publish_changes BOOLEAN NOT NULL DEFAULT false
);
INSERT INTO catalogue_settings (id) VALUES (true) ON CONFLICT DO NOTHING;

-- Change log of a publishing database: the product or category that changed.
-- A change to a variant, image, option or category link logs its product.
CREATE TABLE IF NOT EXISTS catalogue_changes (
    seq BIGSERIAL PRIMARY KEY,
    entity TEXT NOT NULL CHECK (entity IN ('product', 'category')),
    entity_id UUID NOT NULL,
    deleted BOOLEAN NOT NULL DEFAULT false,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_catalogue_changes_changed_at ON catalogue_changes (changed_at);

CREATE OR REPLACE FUNCTION catalogue_log(p_entity TEXT, p_entity_id UUID, p_deleted BOOLEAN)
RETURNS VOID AS $$
BEGIN
    IF p_entity_id IS NOT NULL
       AND EXISTS (SELECT 1 FROM catalogue_settings WHERE publish_changes) THEN
        INSERT INTO catalogue_changes (entity, entity_id, deleted)
        VALUES (p_entity, p_entity_id, p_deleted);
    END IF;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION catalogue_log_product() RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        PERFORM catalogue_log('product', OLD.id, true);
        RETURN OLD;
    END IF;
    PERFORM catalogue_log('product', NEW.id, false);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Child rows of a product: variants, images, options, category links
CREATE OR REPLACE FUNCTION catalogue_log_product_child() RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP IN ('UPDATE', 'DELETE') THEN
        PERFORM catalogue_log('product', OLD.product_id, false);
    END IF;
    IF TG_OP IN ('INSERT', 'UPDATE') THEN
        PERFORM catalogue_log('product', NEW.product_id, false);
        RETURN NEW;
    END IF;
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

-- Option values belong to a product through their option
CREATE OR REPLACE FUNCTION catalogue_log_option_value() RETURNS TRIGGER AS $$
DECLARE
    v_option_id UUID;
BEGIN
    v_option_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.option_id ELSE NEW.option_id END;
    PERFORM catalogue_log(
        'product', (SELECT product_id FROM product_options WHERE id = v_option_id), false);
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION catalogue_log_category() RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        PERFORM catalogue_log('category', OLD.id, true);
        RETURN OLD;
    END IF;
    PERFORM catalogue_log('category', NEW.id, false);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS catalogue_log_products ON products;
CREATE TRIGGER catalogue_log_products AFTER INSERT OR UPDATE OR DELETE ON products
    FOR EACH ROW EXECUTE FUNCTION catalogue_log_product();
DROP TRIGGER IF EXISTS catalogue_log_variants ON product_variants;
CREATE TRIGGER catalogue_log_variants AFTER INSERT OR UPDATE OR DELETE ON product_variants
    FOR EACH ROW EXECUTE FUNCTION catalogue_log_product_child();
DROP TRIGGER IF EXISTS catalogue_log_images ON product_images;
CREATE TRIGGER catalogue_log_images AFTER INSERT OR UPDATE OR DELETE ON product_images
    FOR EACH ROW EXECUTE FUNCTION catalogue_log_product_child();
DROP TRIGGER IF EXISTS catalogue_log_options ON product_options;
CREATE TRIGGER catalogue_log_options AFTER INSERT OR UPDATE OR DELETE ON product_options
    FOR EACH ROW EXECUTE FUNCTION catalogue_log_product_child();
DROP TRIGGER IF EXISTS catalogue_log_option_values ON product_option_values;
CREATE TRIGGER catalogue_log_option_values AFTER INSERT OR UPDATE OR DELETE ON product_option_values
    FOR EACH ROW EXECUTE FUNCTION catalogue_log_option_value();
DROP TRIGGER IF EXISTS catalogue_log_category_links ON product_category_relations;
CREATE TRIGGER catalogue_log_category_links AFTER INSERT OR UPDATE OR DELETE ON product_category_relations
    FOR EACH ROW EXECUTE FUNCTION catalogue_log_product_child();
DROP TRIGGER IF EXISTS catalogue_log_categories ON product_categories;
CREATE TRIGGER catalogue_log_categories AFTER INSERT OR UPDATE OR DELETE ON product_categories
    FOR EACH ROW EXECUTE FUNCTION catalogue_log_category();

-- Store side: where the copy is, and which products the store takes
CREATE TABLE IF NOT EXISTS catalogue_sync_state (
    id BOOLEAN PRIMARY KEY DEFAULT true CHECK (id),
    -- Change-log position the store has caught up with
    last_seq BIGINT NOT NULL DEFAULT 0,
    -- First run, or the assortment changed: copy the whole assortment
    needs_full_sync BOOLEAN NOT NULL DEFAULT true,
    last_run_at TIMESTAMPTZ,
    last_error TEXT
);
INSERT INTO catalogue_sync_state (id) VALUES (true) ON CONFLICT DO NOTHING;

-- Assortment rules: the whole catalogue, a category (with its sub-categories)
-- or single products, by their catalogue id
CREATE TABLE IF NOT EXISTS catalogue_subscriptions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    kind TEXT NOT NULL CHECK (kind IN ('all', 'category', 'product')),
    target_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK ((kind = 'all') = (target_id IS NULL))
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_catalogue_subscriptions_rule
    ON catalogue_subscriptions (kind, COALESCE(target_id, '00000000-0000-0000-0000-000000000000'));

-- Rows copied from a catalogue keep the catalogue's ids; NULL = the store's own
ALTER TABLE products ADD COLUMN IF NOT EXISTS catalogue_synced_at TIMESTAMPTZ;
ALTER TABLE product_categories ADD COLUMN IF NOT EXISTS catalogue_synced_at TIMESTAMPTZ;

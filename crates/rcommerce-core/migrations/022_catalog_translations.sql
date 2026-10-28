-- ============================================================================
-- Migration: Catalogue translations and full-text search
-- ============================================================================
-- Products and categories keep their base-language text in their own rows, so
-- orders, the admin and e-mails work unchanged. Other languages live in
-- translation rows that the storefront API picks with ?lang=. Each translation
-- row carries a stemmed search vector for its language, and an expression index
-- covers the base rows, so catalogue search no longer scans with ILIKE '%term%'.
-- ============================================================================

-- Trigram indexes back the substring fallback used when full-text search finds
-- nothing (Dutch/German compounds: "boormachine" inside "accuboormachine").
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Text-search configuration for a locale ('fr', 'fr-BE' -> french). Declared
-- IMMUTABLE so it can be used in generated columns and index expressions.
CREATE OR REPLACE FUNCTION rc_search_config(locale TEXT) RETURNS regconfig
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    SELECT CASE split_part(replace(lower(locale), '_', '-'), '-', 1)
        WHEN 'en' THEN 'english'::regconfig
        WHEN 'fr' THEN 'french'::regconfig
        WHEN 'nl' THEN 'dutch'::regconfig
        WHEN 'de' THEN 'german'::regconfig
        WHEN 'es' THEN 'spanish'::regconfig
        WHEN 'it' THEN 'italian'::regconfig
        WHEN 'pt' THEN 'portuguese'::regconfig
        ELSE 'simple'::regconfig
    END
$$;

CREATE TABLE IF NOT EXISTS product_translations (
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    locale VARCHAR(16) NOT NULL,
    title VARCHAR(255) NOT NULL,
    slug VARCHAR(255) NOT NULL,
    description TEXT,
    seo_title VARCHAR(255),
    seo_description TEXT,
    search_vector tsvector GENERATED ALWAYS AS (
        setweight(to_tsvector(rc_search_config(locale), title), 'A') ||
        setweight(to_tsvector(rc_search_config(locale), coalesce(description, '')), 'B')
    ) STORED,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (product_id, locale),
    UNIQUE (locale, slug)
);

CREATE INDEX IF NOT EXISTS idx_product_translations_search ON product_translations USING gin (search_vector);
-- Prefix matching ("perc" -> "perceuse") runs on titles only: a prefix checked against
-- the full vector would also hit description words ("colle" -> "click & collect").
CREATE INDEX IF NOT EXISTS idx_product_translations_title_search
    ON product_translations USING gin (to_tsvector(rc_search_config(locale), title));
CREATE INDEX IF NOT EXISTS idx_product_translations_title_trgm ON product_translations USING gin (title gin_trgm_ops);
-- A slug from another language (visitor switched language on a product page)
CREATE INDEX IF NOT EXISTS idx_product_translations_slug ON product_translations (slug);

CREATE TABLE IF NOT EXISTS category_translations (
    category_id UUID NOT NULL REFERENCES product_categories(id) ON DELETE CASCADE,
    locale VARCHAR(16) NOT NULL,
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(255) NOT NULL,
    description TEXT,
    seo_title VARCHAR(255),
    seo_description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (category_id, locale),
    UNIQUE (locale, slug)
);

CREATE INDEX IF NOT EXISTS idx_category_translations_slug ON category_translations (slug);

-- Base-language rows use the language-neutral 'simple' configuration; the
-- expressions must match SEARCH_FULL_TEXT in repository/catalog_i18n.rs exactly.
CREATE INDEX IF NOT EXISTS idx_products_search ON products USING gin ((
    setweight(to_tsvector('simple', title), 'A') ||
    setweight(to_tsvector('simple', coalesce(description, '')), 'B')
));
CREATE INDEX IF NOT EXISTS idx_products_title_search ON products USING gin (to_tsvector('simple', title));

CREATE INDEX IF NOT EXISTS idx_products_title_trgm ON products USING gin (title gin_trgm_ops);

-- Category pages list active products newest first
CREATE INDEX IF NOT EXISTS idx_products_active_created ON products (created_at DESC) WHERE is_active = true;

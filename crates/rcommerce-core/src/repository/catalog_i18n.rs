//! Catalogue translations and localized full-text search.
//!
//! Products and categories keep their base-language text in their own rows;
//! `product_translations` / `category_translations` hold the other languages
//! (migration 022). Locales are primary language subtags ("fr", "nl"), so a
//! request for "fr-BE" reads the "fr" rows.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

pub use crate::common::locale::normalize_locale;

use crate::{models::Product, Error, Result};

/// `products` row plus every translation, as returned by `product_with_translations`
#[derive(sqlx::FromRow)]
struct ProductWithTranslationsRow {
    #[sqlx(flatten)]
    product: Product,
    translations: sqlx::types::Json<Vec<ProductTranslation>>,
}

/// Product text in one locale
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct ProductTranslation {
    pub product_id: Uuid,
    pub locale: String,
    pub title: String,
    pub slug: String,
    pub description: Option<String>,
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
}

/// Category text in one locale
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct CategoryTranslation {
    pub category_id: Uuid,
    pub locale: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
}

/// One page of search hits, best match first
#[derive(Debug, Clone, Default)]
pub struct SearchPage {
    pub ids: Vec<Uuid>,
    pub total: i64,
}

/// The two tsqueries for a search box string: every word as a prefix (matched
/// against titles) and every word whole (matched against title + description):
/// "Perceuse sans-fil!" -> ("perceuse:* & sans:* & fil:*", "perceuse & sans & fil").
/// Only letters and digits survive, so neither can be a tsquery syntax error.
pub fn search_tsqueries(input: &str) -> Option<(String, String)> {
    let words: Vec<String> = input
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .take(8)
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        return None;
    }
    let prefix = words
        .iter()
        .map(|w| format!("{w}:*"))
        .collect::<Vec<_>>()
        .join(" & ");
    Some((prefix, words.join(" & ")))
}

/// ILIKE pattern matching the words in order anywhere in a title:
/// "boor machine" -> "%boor%machine%". Only letters and digits are kept, so the
/// input cannot add wildcards of its own.
fn substring_pattern(input: &str) -> Option<String> {
    let words: Vec<&str> = input
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .take(8)
        .collect();
    (!words.is_empty()).then(|| format!("%{}%", words.join("%")))
}

/// Translation constraint violations become client errors
fn map_unique_violation(e: sqlx::Error, what: &str) -> Error {
    match &e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => {
            Error::validation(format!("{} slug is already used in this locale", what))
        }
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23503") => {
            Error::not_found(format!("{} not found", what))
        }
        _ => Error::Database(e),
    }
}

/// Shared tail of both search queries: active products, optional category, one page
macro_rules! search_sql {
    ($hits:literal, $order:literal) => {
        concat!(
            $hits,
            "
            SELECT h.id, COUNT(*) OVER () AS total
            FROM hits h
            JOIN products p ON p.id = h.id
            WHERE p.is_active = true
              AND ($3::uuid IS NULL OR EXISTS (SELECT 1 FROM product_category_relations r
                                               WHERE r.product_id = p.id AND r.category_id = $3))
            ORDER BY ",
            $order,
            "
            LIMIT $4 OFFSET $5"
        )
    };
}

/// $1 = prefix tsquery, matched on titles; $6 = whole-word tsquery, matched on
/// title + description (see search_tsqueries). Translation rows use their
/// language's stemmer; products without a row in the locale ($2) use their base
/// text. Each expression must match an index of migration 022: GIN cannot apply
/// weight labels, so a prefix checked against the full vector would recheck
/// every description that contains a longer word ("colle" -> "collect").
const SEARCH_FULL_TEXT: &str = search_sql!(
    "
    WITH q AS (
        SELECT to_tsquery(rc_search_config($2), $1) AS localized_prefix,
               to_tsquery(rc_search_config($2), $6) AS localized_words,
               to_tsquery('simple', $1) AS base_prefix,
               to_tsquery('simple', $6) AS base_words
    ), hits AS (
        SELECT t.product_id AS id, ts_rank(t.search_vector, q.localized_prefix) AS rank
        FROM product_translations t, q
        WHERE t.locale = $2
          AND (to_tsvector(rc_search_config(t.locale), t.title) @@ q.localized_prefix
               OR t.search_vector @@ q.localized_words)
        UNION ALL
        SELECT p.id, ts_rank(setweight(to_tsvector('simple', p.title), 'A') ||
                             setweight(to_tsvector('simple', coalesce(p.description, '')), 'B'), q.base_prefix)
        FROM products p, q
        WHERE (to_tsvector('simple', p.title) @@ q.base_prefix
               OR (setweight(to_tsvector('simple', p.title), 'A') ||
                   setweight(to_tsvector('simple', coalesce(p.description, '')), 'B')) @@ q.base_words)
          AND NOT EXISTS (SELECT 1 FROM product_translations t WHERE t.product_id = p.id AND t.locale = $2)
    )",
    "h.rank DESC, p.created_at DESC, p.id"
);

/// $1 = substring_pattern, matched on titles (trigram indexes, migration 022)
const SEARCH_SUBSTRING: &str = search_sql!(
    "
    WITH hits AS (
        SELECT t.product_id AS id
        FROM product_translations t
        WHERE t.locale = $2 AND t.title ILIKE $1
        UNION ALL
        SELECT p.id
        FROM products p
        WHERE p.title ILIKE $1
          AND NOT EXISTS (SELECT 1 FROM product_translations t WHERE t.product_id = p.id AND t.locale = $2)
    )",
    "p.created_at DESC, p.id"
);

/// Catalogue translations (products and categories) and the localized product search
#[derive(Clone)]
pub struct CatalogI18nRepository {
    pool: PgPool,
}

impl CatalogI18nRepository {
    /// A repository on the given connection pool
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Translations of several products in one locale (one query per listing page)
    pub async fn product_translations(
        &self,
        product_ids: &[Uuid],
        locale: &str,
    ) -> Result<HashMap<Uuid, ProductTranslation>> {
        if product_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query_as::<_, ProductTranslation>(
            "SELECT product_id, locale, title, slug, description, seo_title, seo_description
             FROM product_translations WHERE locale = $1 AND product_id = ANY($2)",
        )
        .bind(locale)
        .bind(product_ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|t| (t.product_id, t)).collect())
    }

    /// Every translation of one product (detail page text and hreflang alternates)
    pub async fn all_product_translations(
        &self,
        product_id: Uuid,
    ) -> Result<Vec<ProductTranslation>> {
        Ok(sqlx::query_as::<_, ProductTranslation>(
            "SELECT product_id, locale, title, slug, description, seo_title, seo_description
             FROM product_translations WHERE product_id = $1 ORDER BY locale",
        )
        .bind(product_id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// A product and all its translations in one query, by id or else by slug
    /// (resolved like `product_id_by_slug`). Product pages need both.
    pub async fn product_with_translations(
        &self,
        id: Option<Uuid>,
        slug: &str,
        locale: Option<&str>,
    ) -> Result<Option<(Product, Vec<ProductTranslation>)>> {
        let row = sqlx::query_as::<_, ProductWithTranslationsRow>(
            "SELECT p.*, COALESCE((
                 SELECT json_agg(json_build_object(
                     'product_id', t.product_id, 'locale', t.locale, 'title', t.title, 'slug', t.slug,
                     'description', t.description, 'seo_title', t.seo_title,
                     'seo_description', t.seo_description) ORDER BY t.locale)
                 FROM product_translations t WHERE t.product_id = p.id), '[]') AS translations
             FROM products p
             WHERE p.id = COALESCE($1, (
                 (SELECT product_id FROM product_translations WHERE locale = $3 AND slug = $2)
                 UNION ALL (SELECT id FROM products WHERE slug = $2)
                 UNION ALL (SELECT product_id FROM product_translations WHERE slug = $2)
                 LIMIT 1))",
        )
        .bind(id)
        .bind(slug)
        .bind(locale)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| (r.product, r.translations.0)))
    }

    /// Product id for a slug: the requested locale first, then the base slug,
    /// then any other locale (a link shared from another language).
    pub async fn product_id_by_slug(
        &self,
        slug: &str,
        locale: Option<&str>,
    ) -> Result<Option<Uuid>> {
        Ok(sqlx::query_scalar::<_, Uuid>(
            "(SELECT product_id FROM product_translations WHERE locale = $2 AND slug = $1)
             UNION ALL (SELECT id FROM products WHERE slug = $1)
             UNION ALL (SELECT product_id FROM product_translations WHERE slug = $1)
             LIMIT 1",
        )
        .bind(slug)
        .bind(locale)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Category id for a slug, in the same order as `product_id_by_slug`
    pub async fn category_id_by_slug(
        &self,
        slug: &str,
        locale: Option<&str>,
    ) -> Result<Option<Uuid>> {
        Ok(sqlx::query_scalar::<_, Uuid>(
            "(SELECT category_id FROM category_translations WHERE locale = $2 AND slug = $1)
             UNION ALL (SELECT id FROM product_categories WHERE slug = $1)
             UNION ALL (SELECT category_id FROM category_translations WHERE slug = $1)
             LIMIT 1",
        )
        .bind(slug)
        .bind(locale)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Translations of several categories in one locale
    pub async fn category_translations(
        &self,
        category_ids: &[Uuid],
        locale: &str,
    ) -> Result<HashMap<Uuid, CategoryTranslation>> {
        if category_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query_as::<_, CategoryTranslation>(
            "SELECT category_id, locale, name, slug, description, seo_title, seo_description
             FROM category_translations WHERE locale = $1 AND category_id = ANY($2)",
        )
        .bind(locale)
        .bind(category_ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|t| (t.category_id, t)).collect())
    }

    /// Full-text search over active products, best match first. Every word of
    /// `query` must match, either as a prefix of title words ("perc" finds
    /// "perceuse") or as whole words in the title or description.
    ///
    /// Translation rows are matched with the stemmer of their language; products
    /// without a row in `locale` are matched on their base text. When nothing
    /// matches, titles are searched by substring instead, which finds words inside
    /// compounds that stemmers do not split ("boormachine" in "accuboormachine").
    pub async fn search_products(
        &self,
        query: &str,
        locale: Option<&str>,
        category_id: Option<Uuid>,
        limit: i64,
        offset: i64,
    ) -> Result<SearchPage> {
        let (Some((prefix, words)), Some(pattern)) =
            (search_tsqueries(query), substring_pattern(query))
        else {
            return Ok(SearchPage::default());
        };
        let page = self
            .search_page(
                SEARCH_FULL_TEXT,
                &prefix,
                Some(&words),
                locale,
                category_id,
                limit,
                offset,
            )
            .await?;
        if page.total > 0 {
            return Ok(page);
        }
        self.search_page(
            SEARCH_SUBSTRING,
            &pattern,
            None,
            locale,
            category_id,
            limit,
            offset,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn search_page(
        &self,
        sql: &str,
        query: &str,
        words: Option<&str>,
        locale: Option<&str>,
        category_id: Option<Uuid>,
        limit: i64,
        offset: i64,
    ) -> Result<SearchPage> {
        let rows = self
            .search_rows(sql, query, words, locale, category_id, limit, offset)
            .await?;
        let total = match rows.first() {
            Some((_, total)) => *total,
            // Past the last page no row carries the window count: ask for the first hit
            None if offset > 0 => self
                .search_rows(sql, query, words, locale, category_id, 1, 0)
                .await?
                .first()
                .map_or(0, |(_, total)| *total),
            None => 0,
        };
        Ok(SearchPage {
            ids: rows.into_iter().map(|(id, _)| id).collect(),
            total,
        })
    }

    /// (product id, total hits) rows for one page of SEARCH_FULL_TEXT (`words` = $6)
    /// or SEARCH_SUBSTRING (`words` = None)
    #[allow(clippy::too_many_arguments)]
    async fn search_rows(
        &self,
        sql: &str,
        query: &str,
        words: Option<&str>,
        locale: Option<&str>,
        category_id: Option<Uuid>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<(Uuid, i64)>> {
        let mut q = sqlx::query_as::<_, (Uuid, i64)>(sql)
            .bind(query)
            .bind(locale)
            .bind(category_id)
            .bind(limit)
            .bind(offset);
        if let Some(words) = words {
            q = q.bind(words);
        }
        Ok(q.fetch_all(&self.pool).await?)
    }

    /// Create or replace a product translation
    pub async fn upsert_product_translation(
        &self,
        t: &ProductTranslation,
    ) -> Result<ProductTranslation> {
        sqlx::query_as::<_, ProductTranslation>(
            "INSERT INTO product_translations
                 (product_id, locale, title, slug, description, seo_title, seo_description)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (product_id, locale) DO UPDATE SET
                 title = EXCLUDED.title, slug = EXCLUDED.slug, description = EXCLUDED.description,
                 seo_title = EXCLUDED.seo_title, seo_description = EXCLUDED.seo_description,
                 updated_at = NOW()
             RETURNING product_id, locale, title, slug, description, seo_title, seo_description",
        )
        .bind(t.product_id)
        .bind(&t.locale)
        .bind(&t.title)
        .bind(&t.slug)
        .bind(&t.description)
        .bind(&t.seo_title)
        .bind(&t.seo_description)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| map_unique_violation(e, "Product"))
    }

    /// Delete a product's translation in one locale; false when there was none
    pub async fn delete_product_translation(&self, product_id: Uuid, locale: &str) -> Result<bool> {
        let result =
            sqlx::query("DELETE FROM product_translations WHERE product_id = $1 AND locale = $2")
                .bind(product_id)
                .bind(locale)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Create or replace a category translation
    pub async fn upsert_category_translation(
        &self,
        t: &CategoryTranslation,
    ) -> Result<CategoryTranslation> {
        sqlx::query_as::<_, CategoryTranslation>(
            "INSERT INTO category_translations
                 (category_id, locale, name, slug, description, seo_title, seo_description)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (category_id, locale) DO UPDATE SET
                 name = EXCLUDED.name, slug = EXCLUDED.slug, description = EXCLUDED.description,
                 seo_title = EXCLUDED.seo_title, seo_description = EXCLUDED.seo_description,
                 updated_at = NOW()
             RETURNING category_id, locale, name, slug, description, seo_title, seo_description",
        )
        .bind(t.category_id)
        .bind(&t.locale)
        .bind(&t.name)
        .bind(&t.slug)
        .bind(&t.description)
        .bind(&t.seo_title)
        .bind(&t.seo_description)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| map_unique_violation(e, "Category"))
    }

    /// Delete a category's translation in one locale; false when there was none
    pub async fn delete_category_translation(
        &self,
        category_id: Uuid,
        locale: &str,
    ) -> Result<bool> {
        let result =
            sqlx::query("DELETE FROM category_translations WHERE category_id = $1 AND locale = $2")
                .bind(category_id)
                .bind(locale)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::{search_tsqueries, substring_pattern};

    #[test]
    fn search_tsqueries_builds_prefix_and_word_queries() {
        let q = |s| search_tsqueries(s).map(|(p, w)| format!("{p} | {w}"));
        assert_eq!(q("Perceuse").as_deref(), Some("perceuse:* | perceuse"));
        assert_eq!(
            q("  Perceuse sans-fil! ").as_deref(),
            Some("perceuse:* & sans:* & fil:* | perceuse & sans & fil")
        );
        assert_eq!(q("Échelle").as_deref(), Some("échelle:* | échelle"));
        assert_eq!(q("P000123").as_deref(), Some("p000123:* | p000123"));
    }

    #[test]
    fn substring_pattern_keeps_words_in_order() {
        assert_eq!(
            substring_pattern("boormachine").as_deref(),
            Some("%boormachine%")
        );
        assert_eq!(
            substring_pattern("boor machine").as_deref(),
            Some("%boor%machine%")
        );
        assert_eq!(substring_pattern("100%_x").as_deref(), Some("%100%x%"));
        assert_eq!(substring_pattern("%_"), None);
    }

    #[test]
    fn search_tsqueries_drops_tsquery_syntax() {
        let q = |s| search_tsqueries(s).map(|(p, w)| format!("{p} | {w}"));
        assert_eq!(
            q("a & !b | (c:*)").as_deref(),
            Some("a:* & b:* & c:* | a & b & c")
        );
        assert_eq!(q("'; DROP --").as_deref(), Some("drop:* | drop"));
        assert_eq!(q("&|!():*'\""), None);
        assert_eq!(q(""), None);
    }

    /// DB-backed tests: require PostgreSQL via TEST_DATABASE_URL; skipped without it
    async fn test_pool() -> Option<sqlx::PgPool> {
        let url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres@localhost:5432/rcommerce_scratch".to_string());
        let pool = match sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
        {
            Ok(pool) => pool,
            Err(e) => {
                eprintln!("Skipping DB test: cannot connect to {url}: {e}");
                return None;
            }
        };
        if let Err(e) = crate::Migrator::new(pool.clone()).migrate().await {
            eprintln!("Skipping DB test: migration failed: {e}");
            return None;
        }
        Some(pool)
    }

    /// A product with a base (English) row and translations: (locale, title, slug, description)
    async fn insert_product(
        pool: &sqlx::PgPool,
        base: (&str, &str, &str),
        translations: &[(&str, &str, &str, &str)],
    ) -> uuid::Uuid {
        let id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO products (id, title, slug, description, price) VALUES ($1, $2, $3, $4, 10.00)",
        )
        .bind(id)
        .bind(base.0)
        .bind(base.1)
        .bind(base.2)
        .execute(pool)
        .await
        .unwrap();
        for (locale, title, slug, description) in translations {
            sqlx::query(
                "INSERT INTO product_translations (product_id, locale, title, slug, description)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(id)
            .bind(locale)
            .bind(title)
            .bind(slug)
            .bind(description)
            .execute(pool)
            .await
            .unwrap();
        }
        id
    }

    #[tokio::test]
    async fn translations_and_search_against_postgres() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = super::CatalogI18nRepository::new(pool.clone());
        // A word unique to this run, so other test data never matches
        let tag = format!("t{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);

        let drill = insert_product(
            &pool,
            (
                &format!("Cordless drill {tag}"),
                &format!("cordless-drill-{tag}"),
                "Available for click & collect.",
            ),
            &[
                (
                    "fr",
                    &format!("Perceuse sans fil {tag}"),
                    &format!("perceuse-sans-fil-{tag}"),
                    "Disponible en click & collect.",
                ),
                (
                    "nl",
                    &format!("Accuboormachine {tag}"),
                    &format!("accuboormachine-{tag}"),
                    "Beschikbaar via click & collect.",
                ),
            ],
        )
        .await;
        let glue = insert_product(
            &pool,
            (
                &format!("Wood glue {tag}"),
                &format!("wood-glue-{tag}"),
                "Strong.",
            ),
            &[(
                "fr",
                &format!("Colle à bois {tag}"),
                &format!("colle-a-bois-{tag}"),
                "Solide.",
            )],
        )
        .await;

        // Prefix of a title word, in the translation's language
        let hits = repo
            .search_products(&format!("perc {tag}"), Some("fr"), None, 24, 0)
            .await
            .unwrap();
        assert_eq!(hits.ids, vec![drill]);
        // "colle" must not match "collect" in the drill's description
        let hits = repo
            .search_products(&format!("colle {tag}"), Some("fr"), None, 24, 0)
            .await
            .unwrap();
        assert_eq!((hits.ids, hits.total), (vec![glue], 1));
        // Dutch compound: no word starts with "boormachine", the substring fallback finds it
        let hits = repo
            .search_products(&format!("boormachine {tag}"), Some("nl"), None, 24, 0)
            .await
            .unwrap();
        assert_eq!(hits.ids, vec![drill]);
        // Without a translation in the locale, the base text is searched
        let hits = repo
            .search_products(&format!("glue {tag}"), Some("nl"), None, 24, 0)
            .await
            .unwrap();
        assert_eq!(hits.ids, vec![glue]);

        // Slugs: the requested locale, the base slug, or another locale's slug
        for (slug, locale) in [
            (format!("perceuse-sans-fil-{tag}"), Some("fr")),
            (format!("cordless-drill-{tag}"), Some("fr")),
            (format!("accuboormachine-{tag}"), Some("fr")),
            (format!("accuboormachine-{tag}"), None),
        ] {
            assert_eq!(
                repo.product_id_by_slug(&slug, locale).await.unwrap(),
                Some(drill),
                "{slug}"
            );
        }
        let (product, translations) = repo
            .product_with_translations(None, &format!("perceuse-sans-fil-{tag}"), Some("fr"))
            .await
            .unwrap()
            .expect("product by translated slug");
        assert_eq!(product.id, drill);
        let locales: Vec<&str> = translations.iter().map(|t| t.locale.as_str()).collect();
        assert_eq!(locales, ["fr", "nl"]);

        // A slug is unique per locale: reusing the drill's French slug is a validation error
        let duplicate = repo
            .upsert_product_translation(&super::ProductTranslation {
                product_id: glue,
                locale: "fr".to_string(),
                title: "Colle".to_string(),
                slug: format!("perceuse-sans-fil-{tag}"),
                description: None,
                seo_title: None,
                seo_description: None,
            })
            .await;
        assert!(matches!(duplicate, Err(crate::Error::Validation(_))));

        sqlx::query("DELETE FROM products WHERE id = ANY($1)")
            .bind([drill, glue].as_slice())
            .execute(&pool)
            .await
            .unwrap();
    }
}

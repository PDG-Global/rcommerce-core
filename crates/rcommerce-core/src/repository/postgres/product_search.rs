//! Product listing filters, search and ordering, shared by the page and count queries.
//!
//! Search splits the query into words; every word must appear in the title, the
//! description, the SKU or a variant SKU. Both sides go through the SQL function
//! `catalog_search_text` (migration 030): lower case, accents and spaces removed,
//! decimal comma read as a dot. So "25kg" finds "25 kg", "1.2mm" finds "1,2 mm",
//! "beton" finds "Béton" and "bm-0005" finds the SKU "BM-0005".

use sqlx::{Postgres, QueryBuilder};

use crate::{models::ProductFilter, models::ProductStatus, SortDirection, SortParams};

/// At most this many search words are used; the rest are ignored
pub const MAX_SEARCH_TERMS: usize = 8;

/// Columns a listing may be sorted by
const SORTABLE: [&str; 7] = [
    "id",
    "title",
    "slug",
    "price",
    "created_at",
    "updated_at",
    "inventory_quantity",
];

/// Escape user input for use inside a LIKE pattern matched with ESCAPE '\'.
/// Without this, '%' and '_' in the query would act as wildcards and a
/// backslash could break out of the escaping scheme.
pub fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// The words of a search query, escaped for LIKE (empty when there is nothing to search)
pub fn search_terms(query: Option<&str>) -> Vec<String> {
    query
        .unwrap_or_default()
        .split_whitespace()
        .take(MAX_SEARCH_TERMS)
        .map(escape_like)
        .collect()
}

/// Append the `WHERE` conditions of `filter` (the query already selects from products)
pub fn push_filter(qb: &mut QueryBuilder<'_, Postgres>, filter: &ProductFilter) {
    qb.push(" WHERE 1=1");
    match filter.status {
        Some(ProductStatus::Active) => {
            qb.push(" AND is_active = true");
        }
        Some(ProductStatus::Draft) => {
            qb.push(" AND is_active = false AND published_at IS NULL");
        }
        Some(ProductStatus::Archived) | None => {}
    }
    if let Some(category_id) = filter.category_id {
        qb.push(
            " AND id IN (SELECT product_id FROM product_category_relations WHERE category_id = ",
        )
        .push_bind(category_id)
        .push(")");
    }
    if let Some(min) = filter.price_min {
        qb.push(" AND price >= ").push_bind(min);
    }
    if let Some(max) = filter.price_max {
        qb.push(" AND price <= ").push_bind(max);
    }
    for term in search_terms(filter.query.as_deref()) {
        push_term(qb, term);
    }
}

/// One search word: it must match the title, description, SKU or a variant SKU
fn push_term(qb: &mut QueryBuilder<'_, Postgres>, term: String) {
    qb.push(" AND (SELECT bool_or(catalog_search_text(field) LIKE '%' || catalog_search_text(")
        .push_bind(term)
        .push(
            ") || '%' ESCAPE '\\') FROM (SELECT title AS field UNION ALL SELECT description \
             UNION ALL SELECT sku UNION ALL SELECT v.sku FROM product_variants v \
             WHERE v.product_id = products.id) AS fields)",
        );
}

/// Append `ORDER BY`: an exact SKU match first when searching, then `sort`
pub fn push_order(
    qb: &mut QueryBuilder<'_, Postgres>,
    filter: &ProductFilter,
    sort: Option<&SortParams>,
) -> crate::Result<()> {
    qb.push(" ORDER BY ");
    if let Some(query) = filter
        .query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
    {
        qb.push("(lower(coalesce(sku, '')) = lower(")
            .push_bind(query.to_string())
            .push(")) DESC, ");
    }
    match sort {
        Some(sort) => {
            if !SORTABLE.contains(&sort.field.as_str()) {
                return Err(crate::Error::validation("Invalid sort field"));
            }
            let direction = match sort.direction {
                SortDirection::Asc => "ASC",
                SortDirection::Desc => "DESC",
            };
            // Safe: the column name was checked against the whitelist
            qb.push(format!("{} {}, id", sort.field, direction));
        }
        None => {
            qb.push("created_at DESC, id");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_are_split_escaped_and_capped() {
        assert_eq!(
            search_terms(Some("  ciment   25kg ")),
            vec!["ciment", "25kg"]
        );
        assert_eq!(search_terms(Some("50%_off")), vec!["50\\%\\_off"]);
        assert_eq!(search_terms(Some("a\\b")), vec!["a\\\\b"]);
        assert!(search_terms(Some("   ")).is_empty());
        assert!(search_terms(None).is_empty());
        let many = "w ".repeat(20);
        assert_eq!(search_terms(Some(&many)).len(), MAX_SEARCH_TERMS);
    }

    #[test]
    fn filters_bind_every_value() {
        let filter = ProductFilter {
            status: Some(ProductStatus::Active),
            price_min: Some(rust_decimal::Decimal::ONE),
            query: Some("ciment 25kg".into()),
            ..Default::default()
        };
        let mut qb = QueryBuilder::<Postgres>::new("SELECT * FROM products");
        push_filter(&mut qb, &filter);
        push_order(&mut qb, &filter, None).unwrap();
        let sql = qb.sql();
        assert!(sql.contains("is_active = true"));
        assert!(sql.contains("price >= $1"));
        assert_eq!(sql.matches("catalog_search_text(").count(), 4);
        assert!(sql.contains("lower($4)"));
        assert!(sql.ends_with("created_at DESC, id"));
    }

    #[test]
    fn unknown_sort_columns_are_refused() {
        let sort = SortParams {
            field: "password_hash".into(),
            direction: SortDirection::Asc,
        };
        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
        assert!(push_order(&mut qb, &ProductFilter::default(), Some(&sort)).is_err());
    }
}

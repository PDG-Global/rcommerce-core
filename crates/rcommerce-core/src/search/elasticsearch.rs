//! Elasticsearch catalogue index.
//!
//! One document per product, with a title and description field per configured
//! locale (translation, or the base text when there is none), each analysed
//! for its language. Search returns product ids; product cards still come from
//! PostgreSQL, so prices and stock are never stale.
//!
//! Full reindexes build a new index and swap the alias, so search keeps working
//! while they run. Single products are re-indexed through the alias.

use std::collections::HashMap;

use rust_decimal::prelude::ToPrimitive;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::SearchConfig;
use crate::repository::{normalize_locale, SearchPage};
use crate::{Error, Result};

const BULK_BATCH: i64 = 1000;

#[derive(Deserialize)]
struct LocalizedText {
    title: String,
    description: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ProductDocRow {
    id: Uuid,
    title: String,
    description: Option<String>,
    sku: Option<String>,
    price: rust_decimal::Decimal,
    is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    translations: sqlx::types::Json<HashMap<String, LocalizedText>>,
    category_ids: Vec<Uuid>,
}

const DOC_SELECT: &str = "SELECT p.id, p.title, p.description, p.sku, p.price, p.is_active, p.created_at,
       COALESCE((SELECT json_object_agg(t.locale, json_build_object('title', t.title, 'description', t.description))
                 FROM product_translations t WHERE t.product_id = p.id), '{}') AS translations,
       COALESCE((SELECT array_agg(r.category_id) FROM product_category_relations r WHERE r.product_id = p.id),
                '{}') AS category_ids
FROM products p";

/// Elasticsearch analyzer for a locale (built-in language analysis is extended
/// with ASCII folding, so "echelle" finds "échelle")
fn analyzer_for(locale: &str) -> &'static str {
    match locale {
        "fr" => "rc_fr",
        "nl" => "rc_nl",
        "en" => "rc_en",
        "de" => "rc_de",
        _ => "rc_simple",
    }
}

/// Language analyzers (stemming, stop words, elision, accent folding) and a
/// trigram analyzer for matching inside compound words
fn analysis_settings() -> Value {
    json!({
        "filter": {
            "rc_fr_elision": { "type": "elision", "articles_case": true,
                "articles": ["l", "m", "t", "qu", "n", "s", "j", "d", "c", "jusqu", "quoiqu", "lorsqu", "puisqu"] },
            "rc_fr_stop": { "type": "stop", "stopwords": "_french_" },
            "rc_fr_stem": { "type": "stemmer", "language": "light_french" },
            "rc_nl_stop": { "type": "stop", "stopwords": "_dutch_" },
            "rc_nl_stem": { "type": "stemmer", "language": "dutch" },
            "rc_en_stop": { "type": "stop", "stopwords": "_english_" },
            "rc_en_stem": { "type": "stemmer", "language": "light_english" },
            "rc_de_stop": { "type": "stop", "stopwords": "_german_" },
            "rc_de_stem": { "type": "stemmer", "language": "light_german" }
        },
        "tokenizer": {
            "rc_trigram": { "type": "ngram", "min_gram": 3, "max_gram": 3, "token_chars": ["letter", "digit"] }
        },
        "analyzer": {
            "rc_fr": { "tokenizer": "standard", "filter": ["rc_fr_elision", "lowercase", "asciifolding", "rc_fr_stop", "rc_fr_stem"] },
            "rc_nl": { "tokenizer": "standard", "filter": ["lowercase", "asciifolding", "rc_nl_stop", "rc_nl_stem"] },
            "rc_en": { "tokenizer": "standard", "filter": ["lowercase", "asciifolding", "rc_en_stop", "rc_en_stem"] },
            "rc_de": { "tokenizer": "standard", "filter": ["lowercase", "asciifolding", "rc_de_stop", "rc_de_stem"] },
            "rc_simple": { "tokenizer": "standard", "filter": ["lowercase", "asciifolding"] },
            "rc_trigram": { "tokenizer": "rc_trigram", "filter": ["lowercase", "asciifolding"] }
        }
    })
}

/// Catalogue search through an Elasticsearch index, read and written through
/// the alias `search.index`
pub struct ElasticsearchCatalog {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    alias: String,
    locales: Vec<String>,
    pool: PgPool,
}

impl ElasticsearchCatalog {
    /// A client for the configured server and languages; fails when
    /// `search.elasticsearch_url` or `search.locales` is missing
    pub fn new(config: &SearchConfig, pool: PgPool) -> Result<Self> {
        let base_url = config
            .elasticsearch_url
            .clone()
            .ok_or_else(|| Error::Config("search.elasticsearch_url is required".to_string()))?
            .trim_end_matches('/')
            .to_string();
        let mut locales: Vec<String> = Vec::new();
        for locale in config.locales.iter().filter_map(|l| normalize_locale(l)) {
            if !locales.contains(&locale) {
                locales.push(locale);
            }
        }
        if locales.is_empty() {
            return Err(Error::Config(
                "search.locales must list at least one language".to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| Error::Config(format!("Elasticsearch client: {}", e)))?;
        Ok(Self {
            client,
            base_url,
            api_key: config.elasticsearch_api_key.clone(),
            alias: config.index.clone(),
            locales,
            pool,
        })
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let builder = self
            .client
            .request(method, format!("{}/{}", self.base_url, path));
        match &self.api_key {
            Some(key) => builder.header("Authorization", format!("ApiKey {}", key)),
            None => builder,
        }
    }

    async fn send(&self, builder: reqwest::RequestBuilder) -> Result<Value> {
        let response = builder
            .send()
            .await
            .map_err(|e| Error::network(format!("Elasticsearch: {}", e)))?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|e| Error::network(format!("Elasticsearch response: {}", e)))?;
        if !status.is_success() {
            return Err(Error::network(format!(
                "Elasticsearch {}: {}",
                status, body
            )));
        }
        Ok(body)
    }

    /// Whether the alias exists (an index has been built)
    pub async fn is_indexed(&self) -> Result<bool> {
        let response = self
            .request(reqwest::Method::HEAD, &format!("_alias/{}", self.alias))
            .send()
            .await
            .map_err(|e| Error::network(format!("Elasticsearch: {}", e)))?;
        Ok(response.status().is_success())
    }

    /// Settings and mappings of a new index
    fn index_definition(&self) -> Value {
        json!({
            "settings": {
                "number_of_shards": 1,
                "number_of_replicas": 0,
                "analysis": analysis_settings()
            },
            "mappings": self.mappings()
        })
    }

    /// Strict mappings, with the title and description in every configured language
    fn mappings(&self) -> Value {
        let mut title = serde_json::Map::new();
        let mut description = serde_json::Map::new();
        for locale in &self.locales {
            let analyzer = analyzer_for(locale);
            title.insert(
                locale.clone(),
                json!({
                    "type": "text",
                    "analyzer": analyzer,
                    // Infix matching: compound words ("boormachine" in "accuboormachine")
                    "fields": { "trigram": { "type": "text", "analyzer": "rc_trigram" } }
                }),
            );
            description.insert(
                locale.clone(),
                json!({ "type": "text", "analyzer": analyzer }),
            );
        }
        json!({
            "dynamic": "strict",
            "properties": {
                "product_id": { "type": "keyword" },
                "sku": { "type": "keyword" },
                "price": { "type": "scaled_float", "scaling_factor": 100 },
                "is_active": { "type": "boolean" },
                "created_at": { "type": "date" },
                "category_ids": { "type": "keyword" },
                "title": { "properties": title },
                "description": { "properties": description }
            }
        })
    }

    fn document(&self, row: &ProductDocRow) -> Value {
        let mut title = serde_json::Map::new();
        let mut description = serde_json::Map::new();
        for locale in &self.locales {
            let t = row.translations.0.get(locale);
            title.insert(locale.clone(), json!(t.map_or(&row.title, |t| &t.title)));
            let d = t.map_or(row.description.as_ref(), |t| t.description.as_ref());
            description.insert(locale.clone(), json!(d));
        }
        json!({
            "product_id": row.id,
            "sku": row.sku,
            "price": row.price.to_f64(),
            "is_active": row.is_active,
            "created_at": row.created_at,
            "category_ids": row.category_ids,
            "title": title,
            "description": description,
        })
    }

    async fn bulk(&self, index: &str, rows: &[ProductDocRow], deleted: &[Uuid]) -> Result<()> {
        if rows.is_empty() && deleted.is_empty() {
            return Ok(());
        }
        let mut body = String::new();
        for row in rows {
            body.push_str(&json!({ "index": { "_index": index, "_id": row.id } }).to_string());
            body.push('\n');
            body.push_str(&self.document(row).to_string());
            body.push('\n');
        }
        for id in deleted {
            body.push_str(&json!({ "delete": { "_index": index, "_id": id } }).to_string());
            body.push('\n');
        }
        let result = self
            .send(
                self.request(reqwest::Method::POST, "_bulk")
                    .header("Content-Type", "application/x-ndjson")
                    .body(body),
            )
            .await?;
        if result["errors"].as_bool() == Some(true) {
            let first = result["items"]
                .as_array()
                .and_then(|items| {
                    items.iter().find_map(|i| {
                        let op = i.get("index").or_else(|| i.get("delete"))?;
                        // Deleting a document that was never indexed is fine
                        (op["status"].as_u64() != Some(404))
                            .then(|| op.get("error").cloned())
                            .flatten()
                    })
                })
                .unwrap_or(Value::Null);
            if !first.is_null() {
                return Err(Error::network(format!(
                    "Elasticsearch bulk error: {}",
                    first
                )));
            }
        }
        Ok(())
    }

    /// Build a fresh index from PostgreSQL and point the alias at it.
    /// Returns the number of products indexed.
    pub async fn reindex_all(&self) -> Result<usize> {
        let index = format!("{}-{}", self.alias, chrono::Utc::now().timestamp_millis());
        self.send(
            self.request(reqwest::Method::PUT, &index)
                .json(&self.index_definition()),
        )
        .await?;
        let total = self.fill_index(&index).await?;
        self.send(self.request(reqwest::Method::POST, &format!("{}/_refresh", index)))
            .await?;
        self.point_alias_at(&index).await?;
        Ok(total)
    }

    /// Index every product, in batches; returns the number indexed
    async fn fill_index(&self, index: &str) -> Result<usize> {
        let mut total = 0usize;
        let mut after = Uuid::nil();
        loop {
            let rows = sqlx::query_as::<_, ProductDocRow>(&format!(
                "{DOC_SELECT} WHERE p.id > $1 ORDER BY p.id LIMIT $2"
            ))
            .bind(after)
            .bind(BULK_BATCH)
            .fetch_all(&self.pool)
            .await?;
            let Some(last) = rows.last() else { break };
            after = last.id;
            total += rows.len();
            self.bulk(index, &rows, &[]).await?;
        }
        Ok(total)
    }

    /// Swap the alias to `index` atomically, then drop the indices it pointed to before
    async fn point_alias_at(&self, index: &str) -> Result<()> {
        let previous: Vec<String> = match self
            .send(self.request(reqwest::Method::GET, &format!("_alias/{}", self.alias)))
            .await
        {
            Ok(Value::Object(map)) => map.keys().cloned().collect(),
            _ => Vec::new(),
        };
        let mut actions: Vec<Value> = previous
            .iter()
            .map(|old| json!({ "remove": { "index": old, "alias": self.alias } }))
            .collect();
        actions.push(json!({ "add": { "index": index, "alias": self.alias } }));
        self.send(
            self.request(reqwest::Method::POST, "_aliases")
                .json(&json!({ "actions": actions })),
        )
        .await?;
        for old in previous {
            if let Err(e) = self.send(self.request(reqwest::Method::DELETE, &old)).await {
                tracing::warn!("Could not delete old search index {}: {}", old, e);
            }
        }
        Ok(())
    }

    /// Re-index these products (deleted ones are removed from the index)
    pub async fn index_products(&self, ids: &[Uuid]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let rows =
            sqlx::query_as::<_, ProductDocRow>(&format!("{DOC_SELECT} WHERE p.id = ANY($1)"))
                .bind(ids)
                .fetch_all(&self.pool)
                .await?;
        let deleted: Vec<Uuid> = ids
            .iter()
            .filter(|id| !rows.iter().any(|r| r.id == **id))
            .copied()
            .collect();
        self.bulk(&self.alias, &rows, &deleted).await
    }

    /// Active products matching `query` in `locale`, best match first.
    /// Every word must match: stemmed, with typo tolerance (one edit from 5
    /// letters, two from 8), as a word prefix, or inside a compound word.
    pub async fn search(
        &self,
        query: &str,
        locale: Option<&str>,
        category_id: Option<Uuid>,
        limit: i64,
        offset: i64,
    ) -> Result<SearchPage> {
        let query: String = query.chars().take(200).collect();
        let locale = locale
            .filter(|l| self.locales.iter().any(|known| known.as_str() == *l))
            .unwrap_or(self.locales[0].as_str());
        let title = format!("title.{locale}");
        let mut filter = vec![json!({ "term": { "is_active": true } })];
        if let Some(category_id) = category_id {
            filter.push(json!({ "term": { "category_ids": category_id } }));
        }
        let body = json!({
            "from": offset,
            "size": limit,
            "track_total_hits": true,
            "_source": false,
            "query": { "bool": {
                "filter": filter,
                "should": [
                    { "match": { &title: { "query": query, "operator": "and", "fuzziness": "AUTO:5,8",
                                             "prefix_length": 1, "boost": 3 } } },
                    { "match_bool_prefix": { &title: { "query": query, "operator": "and", "boost": 2 } } },
                    { "match": { format!("description.{locale}"): { "query": query, "operator": "and" } } },
                    { "match": { format!("{title}.trigram"): { "query": query, "operator": "and", "boost": 0.5 } } }
                ],
                "minimum_should_match": 1
            }},
            "sort": ["_score", { "created_at": "desc" }, { "product_id": "asc" }]
        });
        let result = self
            .send(
                self.request(reqwest::Method::POST, &format!("{}/_search", self.alias))
                    .json(&body),
            )
            .await?;
        Ok(search_page(&result))
    }
}

/// Product ids (in rank order) and total hit count of a search response
fn search_page(result: &Value) -> SearchPage {
    let ids = result["hits"]["hits"]
        .as_array()
        .map(|hits| {
            hits.iter()
                .filter_map(|h| h["_id"].as_str().and_then(|id| Uuid::parse_str(id).ok()))
                .collect()
        })
        .unwrap_or_default();
    SearchPage {
        ids,
        total: result["hits"]["total"]["value"].as_i64().unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(locales: &[&str]) -> ElasticsearchCatalog {
        let config = SearchConfig {
            engine: "elasticsearch".to_string(),
            elasticsearch_url: Some("http://localhost:9200/".to_string()),
            elasticsearch_api_key: None,
            index: "rc-test".to_string(),
            locales: locales.iter().map(|l| l.to_string()).collect(),
        };
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/unused")
            .unwrap();
        ElasticsearchCatalog::new(&config, pool).unwrap()
    }

    #[test]
    fn each_known_language_has_its_analyzer() {
        assert_eq!(analyzer_for("fr"), "rc_fr");
        assert_eq!(analyzer_for("nl"), "rc_nl");
        assert_eq!(analyzer_for("en"), "rc_en");
        assert_eq!(analyzer_for("xx"), "rc_simple");
    }

    #[tokio::test]
    async fn locales_are_normalised_and_required() {
        let catalog = catalog(&["fr-BE", "NL", "fr"]);
        assert_eq!(catalog.locales, ["fr", "nl"]);
        assert_eq!(catalog.base_url, "http://localhost:9200");
        let config = SearchConfig {
            locales: vec!["not a locale".to_string()],
            elasticsearch_url: Some("http://localhost:9200".to_string()),
            ..SearchConfig::default()
        };
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/unused")
            .unwrap();
        assert!(ElasticsearchCatalog::new(&config, pool).is_err());
    }

    #[tokio::test]
    async fn documents_fall_back_to_the_base_text() {
        let catalog = catalog(&["en", "fr", "nl"]);
        let mut translations = HashMap::new();
        translations.insert(
            "fr".to_string(),
            LocalizedText {
                title: "Perceuse".to_string(),
                description: None,
            },
        );
        let row = ProductDocRow {
            id: Uuid::new_v4(),
            title: "Drill".to_string(),
            description: Some("Cordless".to_string()),
            sku: Some("P1".to_string()),
            price: rust_decimal::Decimal::new(2129, 2),
            is_active: true,
            created_at: chrono::Utc::now(),
            translations: sqlx::types::Json(translations),
            category_ids: vec![],
        };
        let doc = catalog.document(&row);
        assert_eq!(doc["title"]["fr"], "Perceuse");
        assert_eq!(doc["title"]["nl"], "Drill");
        assert_eq!(doc["title"]["en"], "Drill");
        assert_eq!(doc["description"]["fr"], serde_json::Value::Null);
        assert_eq!(doc["description"]["nl"], "Cordless");
        assert_eq!(doc["price"], 21.29);
    }

    /// Live test: requires PostgreSQL (TEST_DATABASE_URL) and Elasticsearch
    /// (TEST_ELASTICSEARCH_URL); skipped unless both are set
    #[tokio::test]
    async fn index_and_search_against_elasticsearch() {
        let (Ok(db_url), Ok(es_url)) = (
            std::env::var("TEST_DATABASE_URL"),
            std::env::var("TEST_ELASTICSEARCH_URL"),
        ) else {
            eprintln!(
                "Skipping Elasticsearch test: set TEST_DATABASE_URL and TEST_ELASTICSEARCH_URL"
            );
            return;
        };
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&db_url)
            .await
            .unwrap();
        crate::Migrator::new(pool.clone()).migrate().await.unwrap();

        let tag = format!("t{}", &Uuid::new_v4().simple().to_string()[..10]);
        let mut ids = Vec::new();
        for (en, fr, nl) in [
            ("Cordless drill", "Perceuse sans fil", "Accuboormachine"),
            ("Sliding ladder", "Échelle coulissante", "Schuifladder"),
        ] {
            let id = Uuid::new_v4();
            sqlx::query("INSERT INTO products (id, title, slug, price) VALUES ($1, $2, $3, 49.90)")
                .bind(id)
                .bind(format!("{en} {tag}"))
                .bind(format!("en-{id}"))
                .execute(&pool)
                .await
                .unwrap();
            for (locale, title) in [("fr", fr), ("nl", nl)] {
                sqlx::query(
                    "INSERT INTO product_translations (product_id, locale, title, slug)
                     VALUES ($1, $2, $3, $4)",
                )
                .bind(id)
                .bind(locale)
                .bind(format!("{title} {tag}"))
                .bind(format!("{locale}-{id}"))
                .execute(&pool)
                .await
                .unwrap();
            }
            ids.push(id);
        }
        let (drill, ladder) = (ids[0], ids[1]);

        let alias = format!("rc-test-{tag}");
        let catalog = ElasticsearchCatalog::new(
            &SearchConfig {
                engine: "elasticsearch".to_string(),
                elasticsearch_url: Some(es_url),
                elasticsearch_api_key: None,
                index: alias.clone(),
                locales: vec!["en".to_string(), "fr".to_string(), "nl".to_string()],
            },
            pool.clone(),
        )
        .unwrap();
        assert!(!catalog.is_indexed().await.unwrap());
        assert!(catalog.reindex_all().await.unwrap() >= 1);
        assert!(catalog.is_indexed().await.unwrap());

        let found = |query: String, locale: &'static str| {
            let catalog = &catalog;
            async move {
                catalog
                    .search(&query, Some(locale), None, 10, 0)
                    .await
                    .unwrap()
                    .ids
            }
        };
        // Typo tolerance, prefix, accent folding, Dutch compound, base language
        assert_eq!(found(format!("perseuse {tag}"), "fr").await, vec![drill]);
        assert_eq!(found(format!("perc {tag}"), "fr").await, vec![drill]);
        assert_eq!(found(format!("echelle {tag}"), "fr").await, vec![ladder]);
        assert_eq!(found(format!("boormachine {tag}"), "nl").await, vec![drill]);
        assert_eq!(found(format!("drill {tag}"), "en").await, vec![drill]);
        assert!(found(format!("hammer {tag}"), "en").await.is_empty());

        // Deleted products leave the index on re-index
        sqlx::query("DELETE FROM products WHERE id = ANY($1)")
            .bind(ids.as_slice())
            .execute(&pool)
            .await
            .unwrap();
        catalog.index_products(&ids).await.unwrap();
        catalog
            .send(catalog.request(reqwest::Method::POST, &format!("{alias}/_refresh")))
            .await
            .unwrap();
        assert!(found(format!("perc {tag}"), "fr").await.is_empty());

        let indices = catalog
            .send(catalog.request(reqwest::Method::GET, &format!("_alias/{alias}")))
            .await
            .unwrap();
        for index in indices.as_object().unwrap().keys() {
            catalog
                .send(catalog.request(reqwest::Method::DELETE, index))
                .await
                .unwrap();
        }
    }
}

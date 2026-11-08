//! Several stores in one process.
//!
//! Each `[stores.<slug>]` table of the configuration describes one store:
//! its hostnames, its role, and any configuration section it overrides
//! (`[stores.<slug>.database]`, `[stores.<slug>.security.jwt]`...). The table
//! is merged onto the base configuration, so a store only states what differs.
//!
//! Stores are isolated: each has its own database, signing secret and token
//! audience and upload directory. With no `[stores]` table,
//! nothing changes: the base configuration is the one store.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{Config, Error, Result};

/// Connections per store unless the store sets `database.pool_size`
pub const DEFAULT_STORE_POOL_SIZE: u32 = 10;

/// What a store is for
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreRole {
    /// A shop: catalogue, carts, checkout, customers
    #[default]
    Merchant,
    /// A catalogue published to other stores (no checkout)
    Catalogue,
}

/// The identity of one store
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreDefinition {
    pub slug: String,
    /// Host names served by this store, lower case, without port
    pub hostnames: Vec<String>,
    pub role: StoreRole,
}

/// One store: its identity and its full configuration
#[derive(Debug, Clone)]
pub struct StoreConfig {
    pub definition: StoreDefinition,
    pub config: Config,
}

/// Merge `overlay` into `base`: tables merge key by key, any other value
/// (including arrays) replaces the base value
fn deep_merge(base: &mut toml::Value, overlay: &toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base), toml::Value::Table(overlay)) => {
            for (key, value) in overlay {
                match base.get_mut(key) {
                    Some(existing) => deep_merge(existing, value),
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, overlay) => *base = overlay.clone(),
    }
}

/// Whether the store table sets a value at this dotted path
fn sets(table: &toml::Value, path: &[&str]) -> bool {
    let mut current = table;
    for key in path {
        match current.get(key) {
            Some(next) => current = next,
            None => return false,
        }
    }
    true
}

/// Store names: lower-case letters and digits separated by single hyphens
fn valid_store_slug(slug: &str) -> bool {
    (1..=50).contains(&slug.len())
        && slug.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// A host name as stores are matched: lower case, no port, no trailing dot
pub fn normalize_hostname(host: &str) -> String {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    // Bracketed IPv6 literals keep their colons
    if let Some(rest) = host.strip_prefix('[') {
        return match rest.split_once(']') {
            Some((ip, _)) => format!("[{ip}]"),
            None => host,
        };
    }
    match host.rsplit_once(':') {
        Some((name, port)) if port.bytes().all(|b| b.is_ascii_digit()) => name.to_string(),
        _ => host,
    }
}

impl Config {
    /// The stores this configuration defines, each with its merged
    /// configuration; empty when there is no `[stores]` table
    pub fn expand_stores(&self) -> Result<Vec<StoreConfig>> {
        if self.stores.is_empty() {
            return Ok(Vec::new());
        }
        let mut base = toml::Value::try_from(self)
            .map_err(|e| Error::Config(format!("Cannot prepare store configuration: {e}")))?;
        if let Some(table) = base.as_table_mut() {
            table.remove("stores");
            table.remove("store");
        }

        let mut stores = Vec::with_capacity(self.stores.len());
        for (slug, raw) in &self.stores {
            stores.push(self.expand_store(&base, slug, raw)?);
        }
        check_isolation(&stores)?;
        Ok(stores)
    }

    /// One store: the base configuration with the store's table merged over it
    fn expand_store(
        &self,
        base: &toml::Value,
        slug: &str,
        raw: &toml::Value,
    ) -> Result<StoreConfig> {
        if !valid_store_slug(slug) {
            return Err(store_error(
                slug,
                "the store name must be lower-case letters and digits separated by hyphens",
            ));
        }
        let mut overlay = raw.clone();
        let table = overlay
            .as_table_mut()
            .ok_or_else(|| store_error(slug, "must be a table"))?;
        let hostnames = take_hostnames(table, slug)?;
        let role = take_role(table, slug)?;

        let mut merged = base.clone();
        deep_merge(&mut merged, &overlay);
        let mut config: Config = merged
            .try_into()
            .map_err(|e| store_error(slug, format!("is not a valid configuration: {e}")))?;
        self.apply_isolation_defaults(&mut config, &overlay, slug);

        let definition = StoreDefinition {
            slug: slug.to_string(),
            hostnames,
            role,
        };
        config.stores = BTreeMap::new();
        config.store = Some(definition.clone());
        config
            .validate()
            .map_err(|e| store_error(slug, format!("is not valid: {e}")))?;
        Ok(StoreConfig { definition, config })
    }

    /// Give a store its own token audience, upload directory and a bounded
    /// connection pool, unless it sets them itself
    fn apply_isolation_defaults(&self, config: &mut Config, overlay: &toml::Value, slug: &str) {
        if !sets(overlay, &["security", "jwt", "audience"]) {
            config.security.jwt.audience = format!("{}:{slug}", self.security.jwt.audience);
        }
        if !sets(overlay, &["media", "local_path"]) {
            let base_path = self.media.local_path.as_deref().unwrap_or("./uploads");
            config.media.local_path = Some(format!("{}/{slug}", base_path.trim_end_matches('/')));
        }
        if !sets(overlay, &["database", "pool_size"]) {
            config.database.pool_size = config.database.pool_size.min(DEFAULT_STORE_POOL_SIZE);
        }
    }
}

/// A configuration error about one store
fn store_error(slug: &str, message: impl std::fmt::Display) -> Error {
    Error::Config(format!("[stores.{slug}] {message}"))
}

/// Remove and check a store's `hostnames` (normalized, at least one, no scheme or path)
fn take_hostnames(table: &mut toml::Table, slug: &str) -> Result<Vec<String>> {
    let hostnames: Vec<String> = match table.remove("hostnames") {
        Some(toml::Value::Array(values)) => values
            .iter()
            .map(|v| v.as_str().map(normalize_hostname))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| store_error(slug, "hostnames must be strings"))?,
        Some(_) => return Err(store_error(slug, "hostnames must be a list")),
        None => Vec::new(),
    };
    if hostnames.is_empty() || hostnames.iter().any(|h| h.is_empty() || h.contains('/')) {
        return Err(store_error(
            slug,
            "needs hostnames = [\"shop.example.com\", ...] without scheme or path",
        ));
    }
    Ok(hostnames)
}

/// Remove and parse a store's `role` (merchant when absent)
fn take_role(table: &mut toml::Table, slug: &str) -> Result<StoreRole> {
    match table.remove("role") {
        None => Ok(StoreRole::Merchant),
        Some(value) => value
            .try_into()
            .map_err(|_| store_error(slug, "role must be \"merchant\" or \"catalogue\"")),
    }
}

/// Stores must not share a host name, a database or a token secret
fn check_isolation(stores: &[StoreConfig]) -> Result<()> {
    let mut hosts = HashSet::new();
    let mut databases = HashSet::new();
    let mut secrets = HashSet::new();
    for store in stores {
        let slug = &store.definition.slug;
        for host in &store.definition.hostnames {
            if !hosts.insert(host.clone()) {
                return Err(Error::Config(format!(
                    "[stores.{slug}] host name {host} is used by another store"
                )));
            }
        }
        let db = &store.config.database;
        if !databases.insert((db.host.clone(), db.port, db.database.clone())) {
            return Err(Error::Config(format!(
                "[stores.{slug}] shares its database with another store; give each store its own database"
            )));
        }
        if !secrets.insert(store.config.security.jwt.secret.clone()) {
            return Err(Error::Config(format!(
                "[stores.{slug}] shares security.jwt.secret with another store; give each store its own secret"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"
[database]
host = "postgres"
database = "rcommerce"
username = "postgres"
password = "secret"
pool_size = 20

[security.jwt]
secret = "base-secret-0123456789abcdefghijklmnopqrstuvwxyz"

[media]
local_path = "/var/lib/rcommerce/uploads/"
"#;

    fn with_stores(stores: &str) -> Config {
        toml::from_str(&format!("{BASE}\n{stores}")).expect("config parses")
    }

    const TWO_STORES: &str = r#"
[stores.shop-a]
hostnames = ["Shop-A.example:443", "www.shop-a.example."]

[stores.shop-a.database]
database = "rc_shop_a"

[stores.shop-a.security.jwt]
secret = "shop-a-secret-0123456789abcdefghijklmnopqrstuvwxyz"

[stores.network]
hostnames = ["catalogue.example"]
role = "catalogue"

[stores.network.database]
database = "rc_network"
pool_size = 4

[stores.network.security.jwt]
secret = "network-secret-0123456789abcdefghijklmnopqrstuvw"
audience = "network-catalogue"

"#;

    #[test]
    fn no_stores_means_single_store() {
        let config = with_stores("");
        assert!(config.expand_stores().unwrap().is_empty());
        assert!(config.store.is_none());
        assert_eq!(config.security.jwt.audience, "rcommerce-api");
    }

    #[test]
    fn stores_merge_onto_the_base_with_isolation_defaults() {
        let stores = with_stores(TWO_STORES).expand_stores().unwrap();
        assert_eq!(stores.len(), 2);
        let network = &stores[0];
        let shop = &stores[1];

        assert_eq!(shop.definition.slug, "shop-a");
        assert_eq!(shop.definition.role, StoreRole::Merchant);
        assert_eq!(
            shop.definition.hostnames,
            vec!["shop-a.example", "www.shop-a.example"]
        );
        // Overridden
        assert_eq!(shop.config.database.database, "rc_shop_a");
        // Inherited from the base
        assert_eq!(shop.config.database.host, "postgres");
        assert_eq!(shop.config.database.username, "postgres");
        // Isolation defaults
        assert_eq!(shop.config.security.jwt.audience, "rcommerce-api:shop-a");
        assert_eq!(
            shop.config.media.local_path.as_deref(),
            Some("/var/lib/rcommerce/uploads/shop-a")
        );
        assert_eq!(shop.config.database.pool_size, DEFAULT_STORE_POOL_SIZE);
        assert_eq!(shop.config.store.as_ref(), Some(&shop.definition));
        assert!(shop.config.stores.is_empty());

        // What a store sets itself wins over the defaults
        assert_eq!(network.definition.role, StoreRole::Catalogue);
        assert_eq!(network.config.security.jwt.audience, "network-catalogue");
        assert_eq!(network.config.database.pool_size, 4);
    }

    #[test]
    fn stores_must_be_isolated() {
        // Same host name
        let shared_host = TWO_STORES.replace("catalogue.example", "shop-a.example");
        assert!(with_stores(&shared_host).expand_stores().is_err());
        // Same database
        let shared_db = TWO_STORES.replace("rc_network", "rc_shop_a");
        assert!(with_stores(&shared_db).expand_stores().is_err());
        // Same secret (both inherit the base one)
        let shared_secret = r#"
[stores.a]
hostnames = ["a.example.com"]
[stores.a.database]
database = "rc_a"
[stores.b]
hostnames = ["b.example.com"]
[stores.b.database]
database = "rc_b"
"#;
        let err = with_stores(shared_secret).expand_stores().unwrap_err();
        assert!(err.to_string().contains("secret"), "{err}");
    }

    #[test]
    fn store_definitions_are_checked() {
        for bad in [
            "[stores.Shop-A]\nhostnames = [\"a.example.com\"]",
            "[stores.shop-a]\nhostnames = []",
            "[stores.shop-a]\nhostnames = \"a.example.com\"",
            "[stores.shop-a]\nhostnames = [\"https://a.example.com/\"]",
            "[stores.shop-a]\nhostnames = [\"a.example.com\"]\nrole = \"warehouse\"",
        ] {
            assert!(with_stores(bad).expand_stores().is_err(), "{bad}");
        }
    }

    #[test]
    fn hostnames_are_normalized() {
        assert_eq!(
            normalize_hostname("Shop.Example.COM:8443"),
            "shop.example.com"
        );
        assert_eq!(normalize_hostname("shop.example.com."), "shop.example.com");
        assert_eq!(normalize_hostname("[::1]:8080"), "[::1]");
        assert_eq!(normalize_hostname("localhost"), "localhost");
    }
}

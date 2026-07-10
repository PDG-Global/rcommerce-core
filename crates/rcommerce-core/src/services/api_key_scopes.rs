//! API Key Scope and Permission System
//!
//! This module provides granular permission control for API keys.
//! Scopes follow the format: `resource:action`
//!
//! Examples:
//! - `products:read` - Read access to products
//! - `products:write` - Create/update/delete products
//! - `orders:read` - Read access to orders
//! - `orders:write` - Create/update orders
//! - `customers:read` - Read customer data
//! - `customers:write` - Create/update customers
//! - `admin` - Full administrative access
//! - `read` - Read access to all resources (wildcard)
//! - `write` - Write access to all resources (wildcard)

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Resource types that can be accessed via API
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    Products,
    Orders,
    Customers,
    Carts,
    Coupons,
    Payments,
    Inventory,
    Webhooks,
    Users,
    Settings,
    Reports,
    Imports,
    Exports,
}

impl Resource {
    /// Get all available resources
    pub fn all() -> Vec<Resource> {
        vec![
            Resource::Products,
            Resource::Orders,
            Resource::Customers,
            Resource::Carts,
            Resource::Coupons,
            Resource::Payments,
            Resource::Inventory,
            Resource::Webhooks,
            Resource::Users,
            Resource::Settings,
            Resource::Reports,
            Resource::Imports,
            Resource::Exports,
        ]
    }

    /// Convert resource to string
    pub fn as_str(&self) -> &'static str {
        match self {
            Resource::Products => "products",
            Resource::Orders => "orders",
            Resource::Customers => "customers",
            Resource::Carts => "carts",
            Resource::Coupons => "coupons",
            Resource::Payments => "payments",
            Resource::Inventory => "inventory",
            Resource::Webhooks => "webhooks",
            Resource::Users => "users",
            Resource::Settings => "settings",
            Resource::Reports => "reports",
            Resource::Imports => "imports",
            Resource::Exports => "exports",
        }
    }
}

impl std::str::FromStr for Resource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "products" => Ok(Resource::Products),
            "orders" => Ok(Resource::Orders),
            "customers" => Ok(Resource::Customers),
            "carts" => Ok(Resource::Carts),
            "coupons" => Ok(Resource::Coupons),
            "payments" => Ok(Resource::Payments),
            "inventory" => Ok(Resource::Inventory),
            "webhooks" => Ok(Resource::Webhooks),
            "users" => Ok(Resource::Users),
            "settings" => Ok(Resource::Settings),
            "reports" => Ok(Resource::Reports),
            "imports" => Ok(Resource::Imports),
            "exports" => Ok(Resource::Exports),
            _ => Err(format!("Unknown resource: {}", s)),
        }
    }
}

/// Action types that can be performed on resources
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Read,  // GET operations
    Write, // POST, PUT, PATCH, DELETE operations
    Admin, // Administrative operations
}

impl Action {
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::Read => "read",
            Action::Write => "write",
            Action::Admin => "admin",
        }
    }
}

impl std::str::FromStr for Action {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "read" => Ok(Action::Read),
            "write" => Ok(Action::Write),
            "admin" => Ok(Action::Admin),
            _ => Err(format!("Unknown action: {}", s)),
        }
    }
}

/// A scope represents a permission in the format `resource:action`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Scope {
    pub resource: Option<Resource>, // None means wildcard (all resources)
    pub action: Action,
}

impl Scope {
    /// Create a new scope
    pub fn new(resource: Option<Resource>, action: Action) -> Self {
        Self { resource, action }
    }

    /// Parse a scope string like "products:read" or "read"
    pub fn parse(scope: &str) -> Result<Self, String> {
        let parts: Vec<&str> = scope.split(':').collect();

        match parts.len() {
            1 => {
                // Global scope like "read", "write", "admin"
                let action = parts[0].parse()?;
                Ok(Self::new(None, action))
            }
            2 => {
                // Resource-specific scope like "products:read"
                let resource = Some(parts[0].parse()?);
                let action = parts[1].parse()?;
                Ok(Self::new(resource, action))
            }
            _ => Err(format!("Invalid scope format: {}", scope)),
        }
    }

    /// Convert scope to string representation
    pub fn as_scope_string(&self) -> String {
        match &self.resource {
            Some(r) => format!("{}:{}", r.as_str(), self.action.as_str()),
            None => self.action.as_str().to_string(),
        }
    }

    /// Check if this scope grants permission for a given resource and action
    pub fn allows(&self, resource: Resource, action: Action) -> bool {
        // Check if resource matches (or scope is a wildcard)
        let resource_matches = match &self.resource {
            Some(r) => *r == resource,
            None => true, // Wildcard - applies to all resources
        };

        if !resource_matches {
            return false;
        }

        // Check if action is permitted
        // Admin action includes write and read
        // Write action includes read
        matches!(
            (self.action, action),
            (Action::Admin, _)
                | (Action::Write, Action::Read)
                | (Action::Write, Action::Write)
                | (Action::Read, Action::Read)
        )
    }
}

/// Scope checker for validating permissions
#[derive(Debug, Clone)]
pub struct ScopeChecker {
    scopes: HashSet<Scope>,
}

impl ScopeChecker {
    /// Create a new scope checker from a list of scope strings
    pub fn new(scope_strings: &[String]) -> Result<Self, String> {
        let mut scopes = HashSet::new();

        for scope_str in scope_strings {
            let scope = Scope::parse(scope_str)?;
            scopes.insert(scope);
        }

        Ok(Self { scopes })
    }

    /// Check if the given scopes allow a specific action on a resource
    pub fn can(&self, resource: Resource, action: Action) -> bool {
        // Check for admin wildcard first
        for scope in &self.scopes {
            if scope.resource.is_none() && scope.action == Action::Admin {
                return true;
            }
        }

        // Check specific resource scopes
        for scope in &self.scopes {
            if scope.allows(resource, action) {
                return true;
            }
        }

        false
    }

    /// Check if can read a resource
    pub fn can_read(&self, resource: Resource) -> bool {
        self.can(resource, Action::Read)
    }

    /// Check if can write to a resource
    pub fn can_write(&self, resource: Resource) -> bool {
        self.can(resource, Action::Write)
    }

    /// Check if has admin access
    pub fn is_admin(&self) -> bool {
        self.scopes.iter().any(|s| s.action == Action::Admin)
    }

    /// Get all scopes as strings
    pub fn to_scope_strings(&self) -> Vec<String> {
        self.scopes.iter().map(|s| s.as_scope_string()).collect()
    }
}

/// Predefined scope sets for common use cases
pub mod presets {

    /// Read-only access to all resources
    pub fn read_only() -> Vec<String> {
        vec!["read".to_string()]
    }

    /// Read-write access to all resources
    pub fn read_write() -> Vec<String> {
        vec!["read".to_string(), "write".to_string()]
    }

    /// Admin access to all resources
    pub fn admin() -> Vec<String> {
        vec!["admin".to_string()]
    }

    /// Read-only access to products
    pub fn products_read_only() -> Vec<String> {
        vec!["products:read".to_string()]
    }

    /// Full access to products
    pub fn products_full() -> Vec<String> {
        vec!["products:read".to_string(), "products:write".to_string()]
    }

    /// Read-only access to orders
    pub fn orders_read_only() -> Vec<String> {
        vec!["orders:read".to_string()]
    }

    /// Full access to orders
    pub fn orders_full() -> Vec<String> {
        vec!["orders:read".to_string(), "orders:write".to_string()]
    }

    /// Customer-facing access (read products, read/write own orders and cart)
    pub fn customer() -> Vec<String> {
        vec![
            "products:read".to_string(),
            "orders:read".to_string(),
            "orders:write".to_string(),
            "carts:read".to_string(),
            "carts:write".to_string(),
            "customers:read".to_string(),
            "customers:write".to_string(),
            "payments:read".to_string(),
            "payments:write".to_string(),
        ]
    }

    /// Webhook handler access
    pub fn webhook_handler() -> Vec<String> {
        vec![
            "webhooks:write".to_string(),
            "orders:read".to_string(),
            "orders:write".to_string(),
            "payments:read".to_string(),
            "payments:write".to_string(),
        ]
    }

    /// Inventory management access
    pub fn inventory_manager() -> Vec<String> {
        vec![
            "inventory:read".to_string(),
            "inventory:write".to_string(),
            "products:read".to_string(),
            "orders:read".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_parsing() {
        let scope = Scope::parse("products:read").unwrap();
        assert_eq!(scope.resource, Some(Resource::Products));
        assert_eq!(scope.action, Action::Read);

        let scope = Scope::parse("read").unwrap();
        assert_eq!(scope.resource, None);
        assert_eq!(scope.action, Action::Read);

        let scope = Scope::parse("admin").unwrap();
        assert_eq!(scope.resource, None);
        assert_eq!(scope.action, Action::Admin);
    }

    #[test]
    fn test_scope_allows() {
        // Global read scope
        let read_scope = Scope::parse("read").unwrap();
        assert!(read_scope.allows(Resource::Products, Action::Read));
        assert!(!read_scope.allows(Resource::Products, Action::Write));

        // Resource-specific read scope
        let product_read = Scope::parse("products:read").unwrap();
        assert!(product_read.allows(Resource::Products, Action::Read));
        assert!(!product_read.allows(Resource::Products, Action::Write));
        assert!(!product_read.allows(Resource::Orders, Action::Read));

        // Write scope includes read
        let product_write = Scope::parse("products:write").unwrap();
        assert!(product_write.allows(Resource::Products, Action::Read));
        assert!(product_write.allows(Resource::Products, Action::Write));

        // Admin scope includes everything
        let admin_scope = Scope::parse("admin").unwrap();
        assert!(admin_scope.allows(Resource::Products, Action::Read));
        assert!(admin_scope.allows(Resource::Products, Action::Write));
        assert!(admin_scope.allows(Resource::Orders, Action::Admin));
    }

    #[test]
    fn test_scope_checker() {
        let scopes = vec!["products:read".to_string(), "orders:write".to_string()];

        let checker = ScopeChecker::new(&scopes).unwrap();

        assert!(checker.can_read(Resource::Products));
        assert!(!checker.can_write(Resource::Products));

        assert!(checker.can_read(Resource::Orders)); // write includes read
        assert!(checker.can_write(Resource::Orders));

        assert!(!checker.can_read(Resource::Customers));
        assert!(!checker.is_admin());
    }

    #[test]
    fn test_admin_checker() {
        let scopes = vec!["admin".to_string()];
        let checker = ScopeChecker::new(&scopes).unwrap();

        assert!(checker.is_admin());
        assert!(checker.can_read(Resource::Products));
        assert!(checker.can_write(Resource::Products));
        assert!(checker.can_read(Resource::Orders));
        assert!(checker.can_write(Resource::Orders));
    }

    #[test]
    fn test_presets() {
        let checker = ScopeChecker::new(&presets::read_only()).unwrap();
        assert!(checker.can_read(Resource::Products));
        assert!(!checker.can_write(Resource::Products));

        let checker = ScopeChecker::new(&presets::products_read_only()).unwrap();
        assert!(checker.can_read(Resource::Products));
        assert!(!checker.can_read(Resource::Orders));

        let checker = ScopeChecker::new(&presets::customer()).unwrap();
        assert!(checker.can_read(Resource::Products));
        assert!(checker.can_write(Resource::Orders));
        assert!(!checker.can_write(Resource::Products));
    }
}

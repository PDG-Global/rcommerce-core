//! Database access utilities
//!
//! Pools are created from the configuration and passed to the services that
//! need them (`repository::create_pool`); there is no process-wide pool, so
//! several stores in one process never share a database by accident.

pub mod migrate;

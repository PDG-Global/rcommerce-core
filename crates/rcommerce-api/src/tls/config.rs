// Re-export TLS configuration types from rcommerce_core for consistency
pub use rcommerce_core::config::{
    HstsConfig, HstsConfig as TlsHstsConfig, LetsEncryptConfig,
    LetsEncryptConfig as TlsLetsEncryptConfig, TlsConfig, TlsVersion,
};

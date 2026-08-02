//! High-level subscription handling for IronPass.
//!
//! This crate provides the public API used by the CLI to fetch and parse
//! VPN subscription content. The main entry point is [`SubscriptionService`], which
//! composes a fetcher and parser behind a small, convenient facade.
//!
//! For low-level control over fetching (custom HTTP clients, mock HWID providers,
//! retry options, etc.), use [`HttpSubscriptionFetcher`] directly.

mod fetcher;
mod parser;

pub use fetcher::{FetchOptions, HttpSubscriptionFetcher, PlaceholderPolicy, is_placeholder_node};
pub use parser::SubscriptionParser;

pub mod service;
pub use service::SubscriptionService;

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use ironpass_core::models::{Protocol, SubscriptionFormat};

    #[test]
    fn service_detect_format_base64() {
        let svc = SubscriptionService::new();
        let raw = "vless://uuid@example.com:443?encryption=none#T";
        let encoded = STANDARD.encode(raw.as_bytes());
        assert_eq!(
            svc.detect_format(&encoded),
            SubscriptionFormat::Base64VlessList
        );
    }

    #[test]
    fn service_detect_format_raw() {
        let svc = SubscriptionService::new();
        assert_eq!(
            svc.detect_format("vless://uuid@example.com:443?encryption=none#T"),
            SubscriptionFormat::RawUriList
        );
    }

    #[test]
    fn service_parse_raw_delegates() {
        let svc = SubscriptionService::new();
        let raw = "vless://550e8400-e29b-41d4-a716-446655440000@example.com:443?encryption=none#T";
        let nodes = svc.parse_raw(raw).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].protocol, Protocol::Vless);
    }
}

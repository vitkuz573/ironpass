//! OpenAPI 3.1 documentation generation.

use crate::routes::{config, hwid, nodes, proxy, split_tunnel, subscriptions};
use ironpass_api_client::models as api_models;
use ironpass_backend::BackendCapabilities;
use ironpass_config::AppConfig;
use ironpass_core::models::{
    HwidInfo, Protocol, ProxyNode, RoutingMode, Security, SplitTunnelAction, SplitTunnelRule,
    SplitTunnelTarget, SubscriptionMetadata, Transport, XhttpExtra,
};
use utoipa::OpenApi;

/// Central OpenAPI document describing the IronPass public REST API.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "IronPass API",
        version = "0.1.0",
        description = "REST API for managing subscriptions, proxy nodes, split tunnel rules and the local proxy daemon.",
        license(name = "MIT"),
    ),
    paths(
        crate::routes::health,
        crate::routes::backend_capabilities,
        config::get_config,
        config::put_config,
        hwid::get_hwid,
        hwid::regenerate_hwid,
        subscriptions::list,
        subscriptions::add,
        subscriptions::get,
        subscriptions::delete,
        subscriptions::fetch,
        nodes::list,
        nodes::select,
        proxy::status,
        proxy::start,
        proxy::stop,
        split_tunnel::list,
        split_tunnel::add,
        split_tunnel::get,
        split_tunnel::update,
        split_tunnel::delete,
    ),
    components(schemas(
        api_models::AddSplitTunnelRuleRequest,
        api_models::AddSubscriptionRequest,
        api_models::BackendCapabilities,
        api_models::BackendCapability,
        api_models::ConfigResponse,
        api_models::HealthResponse,
        api_models::HwidResponse,
        api_models::NodeWithSubscription,
        api_models::ProxyStatus,
        api_models::StartProxyRequest,
        api_models::StoredSubscription,
        api_models::SubscriptionDetail,
        api_models::UpdateSplitTunnelRuleRequest,
        AppConfig,
        BackendCapabilities,
        HwidInfo,
        Protocol,
        ProxyNode,
        RoutingMode,
        Security,
        SplitTunnelAction,
        SplitTunnelRule,
        SplitTunnelTarget,
        SubscriptionMetadata,
        Transport,
        XhttpExtra,
        ironpass_config::AppConfig,
        ironpass_config::GeneralConfig,
        ironpass_config::SubscriptionConfig,
        ironpass_config::HwidConfig,
        ironpass_config::LoggingConfig,
    )),
    tags(
        (name = "Subscriptions", description = "Manage VPN subscription URLs and refresh their nodes"),
        (name = "Nodes", description = "List and select proxy nodes"),
        (name = "Proxy", description = "Start, stop and inspect the local proxy"),
        (name = "Split Tunnel", description = "Configure selective routing rules"),
        (name = "Backend", description = "Inspect installed proxy core capabilities"),
        (name = "System", description = "Health and configuration"),
        (name = "Auth", description = "HWID and device identity"),
    ),
)]
pub struct ApiDoc;

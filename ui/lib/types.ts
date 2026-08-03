export type {
  AddSplitTunnelRuleRequest,
  AddSubscriptionRequest,
  AppConfig,
  BackendCapabilities,
  BackendCapability,
  BackendType,
  ConfigResponse,
  HealthResponse,
  HwidInfo,
  HwidResponse,
  NodeWithSubscription,
  ProxyNode,
  ProxyStatus,
  RoutingMode,
  SplitTunnelAction,
  SplitTunnelRule,
  SplitTunnelTarget,
  StartProxyRequest,
  StoredSubscription,
  SubscriptionDetail,
  SubscriptionMetadata,
  UpdateSplitTunnelRuleRequest,
} from "./api";

export interface ApiErrorResponse {
  error?: string;
}

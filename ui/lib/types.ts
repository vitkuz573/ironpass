export type BackendType = "auto" | "sing-box" | "xray";

export type SplitTunnelTarget = "domain" | "ip" | "cidr" | "app";

export type SplitTunnelAction = "direct" | "proxy";

export interface HwidInfo {
  machine_id?: string | null;
  os?: string | null;
  hostname?: string | null;
}

export interface ProxyNode {
  name: string;
  protocol?: string | null;
  address?: string | null;
  port?: number | null;
  [key: string]: unknown;
}

export interface SubscriptionMetadata {
  upload?: number | null;
  download?: number | null;
  total?: number | null;
  expire?: number | null;
  [key: string]: unknown;
}

export interface AddSplitTunnelRuleRequest {
  target: SplitTunnelTarget;
  value: string;
  action: SplitTunnelAction;
  node_id?: string | null;
}

export interface UpdateSplitTunnelRuleRequest {
  target: SplitTunnelTarget;
  value: string;
  action: SplitTunnelAction;
  node_id?: string | null;
}

export interface AddSubscriptionRequest {
  url: string;
  name?: string | null;
  hwid?: string | null;
}

export interface StoredSubscription {
  id: string;
  url: string;
  name?: string | null;
  hwid?: string | null;
  added_at: string;
  last_updated?: string | null;
  is_active: boolean;
  metadata: SubscriptionMetadata;
  traffic_used?: number | null;
  traffic_total?: number | null;
  expires_at?: string | null;
}

export interface NodeWithSubscription {
  id: string;
  subscription_id: string;
  subscription_name?: string | null;
  selected: boolean;
  node: ProxyNode;
}

export interface StartProxyRequest {
  node_id?: string | null;
  socks_port?: number | null;
  http_port?: number | null;
  mixed_port?: number | null;
  backend?: BackendType | null;
}

export interface ProxyStatus {
  running: boolean;
  selected_node?: NodeWithSubscription | null;
  socks_port?: number | null;
  http_port?: number | null;
  mixed_port?: number | null;
  pid?: number | null;
  uptime_secs?: number | null;
  last_error?: string | null;
  backend?: BackendType | null;
}

export interface HealthResponse {
  status: string;
  version: string;
  uptime_secs: number;
  hwid: string;
}

export interface ConfigResponse {
  config: AppConfig;
}

export interface HwidResponse {
  hwid: string;
  info: HwidInfo;
}

export interface SubscriptionDetail {
  subscription: StoredSubscription;
  nodes: NodeWithSubscription[];
}

export interface SplitTunnelRule {
  id: string;
  target: SplitTunnelTarget;
  value: string;
  action: SplitTunnelAction;
  node_id?: string | null;
}

export interface AppConfig {
  api_port?: number | null;
  data_dir?: string | null;
  config_path?: string | null;
  log_level?: string | null;
  [key: string]: unknown;
}

export interface ApiErrorResponse {
  error?: string;
}

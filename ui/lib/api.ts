import { api } from "@/api/client";
import type { components } from "@/api/schema";

export type AddSubscriptionRequest =
  components["schemas"]["AddSubscriptionRequest"];
export type AddSplitTunnelRuleRequest =
  components["schemas"]["AddSplitTunnelRuleRequest"];
export type AppConfig = components["schemas"]["AppConfig"];
export type BackendCapabilities =
  components["schemas"]["BackendCapabilities"];
export type BackendCapability = components["schemas"]["BackendCapability"];
export type BackendType = components["schemas"]["BackendType"];
export type ConfigResponse = components["schemas"]["ConfigResponse"];
export type HealthResponse = components["schemas"]["HealthResponse"];
export type HwidInfo = components["schemas"]["HwidInfo"];
export type HwidResponse = components["schemas"]["HwidResponse"];
export type NodeWithSubscription = components["schemas"]["NodeWithSubscription"];
export type ProxyNode = components["schemas"]["ProxyNode"];
export type ProxyStatus = components["schemas"]["ProxyStatus"];
export type RoutingMode = components["schemas"]["RoutingMode"];
export type SplitTunnelAction = components["schemas"]["SplitTunnelAction"];
export type SplitTunnelRule = components["schemas"]["SplitTunnelRule"];
export type SplitTunnelTarget = components["schemas"]["SplitTunnelTarget"];
export type StartProxyRequest = components["schemas"]["StartProxyRequest"];
export type StoredSubscription = components["schemas"]["StoredSubscription"];
export type SubscriptionDetail = components["schemas"]["SubscriptionDetail"];
export type SubscriptionMetadata =
  components["schemas"]["SubscriptionMetadata"];
export type UpdateSplitTunnelRuleRequest =
  components["schemas"]["UpdateSplitTunnelRuleRequest"];

class ApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
    this.name = "ApiError";
  }
}

async function unwrap<T>(
  call: Promise<{ data?: T; error?: unknown }>
): Promise<T> {
  const { data, error } = await call;
  if (error) {
    if (error instanceof Response) {
      let message = "Unknown API error";
      try {
        const body = (await error.json()) as { error?: string };
        if (body.error) message = body.error;
      } catch {
        // ignore parse error
      }
      throw new ApiError(error.status, message);
    }
    if (error instanceof Error) {
      throw new ApiError(0, error.message);
    }
    throw new ApiError(0, String(error));
  }
  if (data === undefined) {
    throw new ApiError(204, "No content");
  }
  return data;
}

export class IronpassApi {
  static health(): Promise<HealthResponse> {
    return unwrap(api.GET("/api/v1/health"));
  }

  static backendCapabilities(): Promise<BackendCapabilities> {
    return unwrap(api.GET("/api/v1/backend/capabilities"));
  }

  static getConfig(): Promise<AppConfig> {
    return unwrap(
      api.GET("/api/v1/config").then((r) => ({
        data: r.data?.config,
        error: r.error,
      }))
    );
  }

  static putConfig(config: AppConfig): Promise<AppConfig> {
    return unwrap(
      api.PUT("/api/v1/config", { body: config }).then((r) => ({
        data: r.data?.config,
        error: r.error,
      }))
    );
  }

  static getHwid(): Promise<HwidResponse> {
    return unwrap(api.GET("/api/v1/hwid"));
  }

  static regenerateHwid(): Promise<HwidResponse> {
    return unwrap(api.PUT("/api/v1/hwid/regenerate"));
  }

  static listSubscriptions(): Promise<StoredSubscription[]> {
    return unwrap(api.GET("/api/v1/subscriptions"));
  }

  static addSubscription(
    url: string,
    name?: string | null,
    hwid?: string | null
  ): Promise<StoredSubscription> {
    const body: AddSubscriptionRequest = { url, name, hwid };
    return unwrap(api.POST("/api/v1/subscriptions", { body }));
  }

  static getSubscription(id: string): Promise<SubscriptionDetail> {
    return unwrap(api.GET("/api/v1/subscriptions/{id}", { params: { path: { id } } }));
  }

  static deleteSubscription(id: string): Promise<unknown> {
    return unwrap(api.DELETE("/api/v1/subscriptions/{id}", { params: { path: { id } } }));
  }

  static fetchSubscription(
    id: string,
    hwid?: string | null
  ): Promise<SubscriptionDetail> {
    const params = hwid ? { path: { id }, query: { hwid } } : { path: { id } };
    return unwrap(api.PUT("/api/v1/subscriptions/{id}/fetch", { params }));
  }

  static listNodes(subscriptionId?: string | null): Promise<NodeWithSubscription[]> {
    const params = subscriptionId ? { query: { subscription: subscriptionId } } : {};
    return unwrap(api.GET("/api/v1/nodes", { params }));
  }

  static selectNode(id: string): Promise<NodeWithSubscription> {
    return unwrap(api.PUT("/api/v1/nodes/{id}/select", { params: { path: { id } } }));
  }

  static proxyStatus(): Promise<ProxyStatus> {
    return unwrap(api.GET("/api/v1/proxy/status"));
  }

  static startProxy(req: StartProxyRequest): Promise<ProxyStatus> {
    return unwrap(api.POST("/api/v1/proxy/start", { body: req }));
  }

  static stopProxy(): Promise<ProxyStatus> {
    return unwrap(api.POST("/api/v1/proxy/stop"));
  }

  static listSplitTunnelRules(nodeId?: string | null): Promise<SplitTunnelRule[]> {
    const params = nodeId ? { query: { node: nodeId } } : {};
    return unwrap(api.GET("/api/v1/split-tunnel", { params }));
  }

  static addSplitTunnelRule(
    target: SplitTunnelTarget,
    value: string,
    action: SplitTunnelAction,
    nodeId?: string | null
  ): Promise<SplitTunnelRule> {
    const body: AddSplitTunnelRuleRequest = { target, value, action, node_id: nodeId };
    return unwrap(api.POST("/api/v1/split-tunnel", { body }));
  }

  static updateSplitTunnelRule(
    id: string,
    target: SplitTunnelTarget,
    value: string,
    action: SplitTunnelAction,
    nodeId?: string | null
  ): Promise<SplitTunnelRule> {
    const body: UpdateSplitTunnelRuleRequest = {
      target,
      value,
      action,
      node_id: nodeId,
    };
    return unwrap(
      api.PUT("/api/v1/split-tunnel/{id}", { params: { path: { id } }, body })
    );
  }

  static deleteSplitTunnelRule(id: string): Promise<unknown> {
    return unwrap(api.DELETE("/api/v1/split-tunnel/{id}", { params: { path: { id } } }));
  }
}

export const backendOptions: { value: BackendType; label: string }[] = [
  { value: "auto", label: "Auto" },
  { value: "sing_box", label: "sing-box" },
  { value: "xray", label: "Xray" },
];

export const splitTunnelTargetOptions: { value: SplitTunnelTarget; label: string }[] = [
  { value: "domain", label: "Domain" },
  { value: "ip", label: "IP" },
  { value: "cidr", label: "CIDR" },
  { value: "app", label: "App" },
];

export const splitTunnelActionOptions: { value: SplitTunnelAction; label: string }[] = [
  { value: "direct", label: "Direct" },
  { value: "proxy", label: "Proxy" },
];

export const routingModeOptions: { value: RoutingMode; label: string }[] = [
  { value: "proxy_all_except_bypass", label: "Proxy all except bypass rules" },
  { value: "proxy_only_listed", label: "Proxy only listed rules" },
];

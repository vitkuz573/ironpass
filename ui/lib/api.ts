import {
  type AddSubscriptionRequest,
  type AddSplitTunnelRuleRequest,
  type AppConfig,
  type BackendCapabilities,
  type BackendType,
  type ConfigResponse,
  type HealthResponse,
  type HwidResponse,
  type NodeWithSubscription,
  type ProxyStatus,
  type SplitTunnelAction,
  type SplitTunnelRule,
  type SplitTunnelTarget,
  type StartProxyRequest,
  type StoredSubscription,
  type SubscriptionDetail,
  type UpdateSplitTunnelRuleRequest,
} from "./types";

export const API_BASE_URL =
  typeof process !== "undefined" && process.env.NEXT_PUBLIC_API_URL
    ? process.env.NEXT_PUBLIC_API_URL
    : "http://127.0.0.1:3001";

class ApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
    this.name = "ApiError";
  }
}

async function parseJson<T>(response: Response): Promise<T> {
  if (response.status === 204) {
    return undefined as T;
  }
  if (response.ok) {
    return (await response.json()) as T;
  }
  let message = "Unknown API error";
  try {
    const data = (await response.json()) as { error?: string };
    if (data.error) message = data.error;
  } catch {
    // ignore parse error
  }
  throw new ApiError(response.status, message);
}

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${API_BASE_URL}${path}`, {
    method: "GET",
    headers: { Accept: "application/json" },
  });
  return parseJson<T>(res);
}

async function post<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${API_BASE_URL}${path}`, {
    method: "POST",
    headers: {
      Accept: "application/json",
      "Content-Type": "application/json",
    },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  return parseJson<T>(res);
}

async function put<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${API_BASE_URL}${path}`, {
    method: "PUT",
    headers: {
      Accept: "application/json",
      "Content-Type": "application/json",
    },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  return parseJson<T>(res);
}

async function del<T>(path: string): Promise<T> {
  const res = await fetch(`${API_BASE_URL}${path}`, {
    method: "DELETE",
    headers: { Accept: "application/json" },
  });
  return parseJson<T>(res);
}

export class IronpassApi {
  static health(): Promise<HealthResponse> {
    return get("/api/v1/health");
  }

  static backendCapabilities(): Promise<BackendCapabilities> {
    return get("/api/v1/backend/capabilities");
  }

  static getConfig(): Promise<AppConfig> {
    return get<ConfigResponse>("/api/v1/config").then((r) => r.config);
  }

  static putConfig(config: AppConfig): Promise<AppConfig> {
    return put<ConfigResponse>("/api/v1/config", config).then((r) => r.config);
  }

  static getHwid(): Promise<HwidResponse> {
    return get("/api/v1/hwid");
  }

  static regenerateHwid(): Promise<HwidResponse> {
    return put("/api/v1/hwid/regenerate");
  }

  static listSubscriptions(): Promise<StoredSubscription[]> {
    return get("/api/v1/subscriptions");
  }

  static addSubscription(
    url: string,
    name?: string | null,
    hwid?: string | null
  ): Promise<StoredSubscription> {
    const body: AddSubscriptionRequest = { url, name, hwid };
    return post("/api/v1/subscriptions", body);
  }

  static getSubscription(id: string): Promise<SubscriptionDetail> {
    return get(`/api/v1/subscriptions/${encodeURIComponent(id)}`);
  }

  static deleteSubscription(id: string): Promise<unknown> {
    return del(`/api/v1/subscriptions/${encodeURIComponent(id)}`);
  }

  static fetchSubscription(
    id: string,
    hwid?: string | null
  ): Promise<SubscriptionDetail> {
    const params = new URLSearchParams();
    if (hwid) params.set("hwid", hwid);
    const query = params.toString();
    return put(
      `/api/v1/subscriptions/${encodeURIComponent(id)}/fetch${
        query ? `?${query}` : ""
      }`
    );
  }

  static listNodes(subscriptionId?: string | null): Promise<NodeWithSubscription[]> {
    const path = subscriptionId
      ? `/api/v1/nodes?subscription=${encodeURIComponent(subscriptionId)}`
      : "/api/v1/nodes";
    return get(path);
  }

  static selectNode(id: string): Promise<NodeWithSubscription> {
    return put(`/api/v1/nodes/${encodeURIComponent(id)}/select`);
  }

  static proxyStatus(): Promise<ProxyStatus> {
    return get("/api/v1/proxy/status");
  }

  static startProxy(req: StartProxyRequest): Promise<ProxyStatus> {
    return post("/api/v1/proxy/start", req);
  }

  static stopProxy(): Promise<ProxyStatus> {
    return post("/api/v1/proxy/stop");
  }

  static listSplitTunnelRules(nodeId?: string | null): Promise<SplitTunnelRule[]> {
    const path = nodeId
      ? `/api/v1/split-tunnel?node=${encodeURIComponent(nodeId)}`
      : "/api/v1/split-tunnel";
    return get(path);
  }

  static addSplitTunnelRule(
    target: SplitTunnelTarget,
    value: string,
    action: SplitTunnelAction,
    nodeId?: string | null
  ): Promise<SplitTunnelRule> {
    const body: AddSplitTunnelRuleRequest = { target, value, action, node_id: nodeId };
    return post("/api/v1/split-tunnel", body);
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
    return put(`/api/v1/split-tunnel/${encodeURIComponent(id)}`, body);
  }

  static deleteSplitTunnelRule(id: string): Promise<unknown> {
    return del(`/api/v1/split-tunnel/${encodeURIComponent(id)}`);
  }
}

export const backendOptions: { value: BackendType; label: string }[] = [
  { value: "auto", label: "Auto" },
  { value: "sing-box", label: "sing-box" },
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

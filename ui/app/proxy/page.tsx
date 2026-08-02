"use client";

import { useEffect, useState, useCallback } from "react";
import { IronpassApi, backendOptions } from "@/lib/api";
import type { BackendType, NodeWithSubscription, ProxyStatus } from "@/lib/types";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Play, Square, Server } from "lucide-react";
import { toast } from "sonner";

export default function ProxyPage() {
  const [status, setStatus] = useState<ProxyStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState(false);

  const [nodeId] = useState<string>("");
  const [socksPort, setSocksPort] = useState<string>("");
  const [httpPort, setHttpPort] = useState<string>("");
  const [mixedPort, setMixedPort] = useState<string>("");
  const [backend, setBackend] = useState<BackendType>("auto");

  const fetchStatus = useCallback(async () => {
    setLoading(true);
    try {
      const s = await IronpassApi.proxyStatus();
      setStatus(s);
      setSocksPort(s.socks_port?.toString() ?? "");
      setHttpPort(s.http_port?.toString() ?? "");
      setMixedPort(s.mixed_port?.toString() ?? "");
      setBackend(s.backend ?? "auto");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to load proxy status");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchStatus();
    const interval = setInterval(() => {
      IronpassApi.proxyStatus()
        .then((s) => {
          setStatus(s);
          setSocksPort(s.socks_port?.toString() ?? "");
          setHttpPort(s.http_port?.toString() ?? "");
          setMixedPort(s.mixed_port?.toString() ?? "");
          setBackend(s.backend ?? "auto");
        })
        .catch((err) => {
          toast.error(err instanceof Error ? err.message : "Failed to refresh proxy status");
        });
    }, 3000);
    return () => clearInterval(interval);
  }, [fetchStatus]);

  async function handleToggle() {
    if (!status) return;
    setActionLoading(true);
    try {
      const next = status.running
        ? await IronpassApi.stopProxy()
        : await IronpassApi.startProxy({
            node_id: nodeId || status.selected_node?.id || null,
            socks_port: parsePort(socksPort),
            http_port: parsePort(httpPort),
            mixed_port: parsePort(mixedPort),
            backend,
          });
      setStatus(next);
      toast.success(next.running ? "Proxy started" : "Proxy stopped");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Proxy action failed");
    } finally {
      setActionLoading(false);
    }
  }

  async function handleStartWithSettings(e: React.FormEvent) {
    e.preventDefault();
    setActionLoading(true);
    try {
      const next = await IronpassApi.startProxy({
        node_id: nodeId || status?.selected_node?.id || null,
        socks_port: parsePort(socksPort),
        http_port: parsePort(httpPort),
        mixed_port: parsePort(mixedPort),
        backend,
      });
      setStatus(next);
      toast.success("Proxy started");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to start proxy");
    } finally {
      setActionLoading(false);
    }
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Proxy</h1>
        <p className="text-muted-foreground">Control the local proxy service.</p>
      </div>

      {loading ? (
        <Skeleton className="h-40 w-full" />
      ) : status ? (
        <StatusCard status={status} />
      ) : null}

      <div className="grid gap-6 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Control</CardTitle>
            <CardDescription>Start or stop the proxy service.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex items-center gap-4">
              <div
                className={`flex h-12 w-12 items-center justify-center rounded-full ${
                  status?.running
                    ? "bg-green-100 text-green-600 dark:bg-green-900 dark:text-green-100"
                    : "bg-muted text-muted-foreground"
                }`}
              >
                <Server className="size-6" />
              </div>
              <div>
                <div className="text-lg font-semibold">
                  {status?.running ? "Running" : "Stopped"}
                </div>
                <div className="text-sm text-muted-foreground">
                  {status?.selected_node?.node.name ?? "No node selected"}
                </div>
              </div>
            </div>
            <Button
              size="lg"
              className="w-full"
              variant={status?.running ? "destructive" : "default"}
              onClick={handleToggle}
              disabled={actionLoading}
            >
              {status?.running ? (
                <>
                  <Square className="mr-2 size-4" /> Stop proxy
                </>
              ) : (
                <>
                  <Play className="mr-2 size-4" /> Start proxy
                </>
              )}
            </Button>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Settings</CardTitle>
            <CardDescription>
              Configure ports and backend before starting.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <form onSubmit={handleStartWithSettings} className="space-y-4">
              <div className="grid gap-4 sm:grid-cols-2">
                <div className="grid gap-2">
                  <Label htmlFor="socks-port">SOCKS port</Label>
                  <Input
                    id="socks-port"
                    type="number"
                    placeholder="1080"
                    value={socksPort}
                    onChange={(e) => setSocksPort(e.target.value)}
                  />
                </div>
                <div className="grid gap-2">
                  <Label htmlFor="http-port">HTTP port</Label>
                  <Input
                    id="http-port"
                    type="number"
                    placeholder="8080"
                    value={httpPort}
                    onChange={(e) => setHttpPort(e.target.value)}
                  />
                </div>
                <div className="grid gap-2">
                  <Label htmlFor="mixed-port">Mixed port</Label>
                  <Input
                    id="mixed-port"
                    type="number"
                    placeholder="7890"
                    value={mixedPort}
                    onChange={(e) => setMixedPort(e.target.value)}
                  />
                </div>
                <div className="grid gap-2">
                  <Label htmlFor="backend">Backend</Label>
                  <Select value={backend} onValueChange={(v) => setBackend(v as BackendType)}>
                    <SelectTrigger id="backend">
                      <SelectValue placeholder="Select backend" />
                    </SelectTrigger>
                    <SelectContent>
                      {backendOptions.map((opt) => (
                        <SelectItem key={opt.value} value={opt.value}>
                          {opt.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </div>
              <Button type="submit" className="w-full" disabled={actionLoading}>
                Apply & Start
              </Button>
            </form>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

function StatusCard({ status }: { status: ProxyStatus }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Live status</CardTitle>
        <CardDescription>Current proxy runtime information.</CardDescription>
      </CardHeader>
      <CardContent>
        <dl className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          <div>
            <dt className="text-xs font-medium text-muted-foreground">State</dt>
            <dd className="text-sm font-semibold">{status.running ? "Running" : "Stopped"}</dd>
          </div>
          <div>
            <dt className="text-xs font-medium text-muted-foreground">PID</dt>
            <dd className="text-sm font-semibold">{status.pid ?? "—"}</dd>
          </div>
          <div>
            <dt className="text-xs font-medium text-muted-foreground">Uptime</dt>
            <dd className="text-sm font-semibold">
              {status.uptime_secs ? formatDuration(status.uptime_secs) : "—"}
            </dd>
          </div>
          <div>
            <dt className="text-xs font-medium text-muted-foreground">SOCKS</dt>
            <dd className="text-sm font-semibold">{status.socks_port ?? "—"}</dd>
          </div>
          <div>
            <dt className="text-xs font-medium text-muted-foreground">HTTP</dt>
            <dd className="text-sm font-semibold">{status.http_port ?? "—"}</dd>
          </div>
          <div>
            <dt className="text-xs font-medium text-muted-foreground">Mixed</dt>
            <dd className="text-sm font-semibold">{status.mixed_port ?? "—"}</dd>
          </div>
          <div>
            <dt className="text-xs font-medium text-muted-foreground">Backend</dt>
            <dd className="text-sm font-semibold">{status.backend ?? "auto"}</dd>
          </div>
          <div className="sm:col-span-2 lg:col-span-2">
            <dt className="text-xs font-medium text-muted-foreground">Selected node</dt>
            <dd className="text-sm font-semibold">
              {status.selected_node ? (
                <NodeDescription node={status.selected_node} />
              ) : (
                "—"
              )}
            </dd>
          </div>
          {status.last_error && (
            <div className="sm:col-span-2 lg:col-span-3">
              <dt className="text-xs font-medium text-destructive">Last error</dt>
              <dd className="text-sm text-destructive">{status.last_error}</dd>
            </div>
          )}
        </dl>
      </CardContent>
    </Card>
  );
}

function NodeDescription({ node }: { node: NodeWithSubscription }) {
  return (
    <span>
      {node.node.name}
      {node.subscription_name ? ` · ${node.subscription_name}` : ""}
    </span>
  );
}

function parsePort(value: string): number | null {
  const n = parseInt(value, 10);
  if (Number.isNaN(n) || n < 1 || n > 65535) return null;
  return n;
}

function formatDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) return `${h}h ${m}m ${s}s`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

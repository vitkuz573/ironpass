"use client";

import { useEffect, useState, useCallback } from "react";
import Link from "next/link";
import { IronpassApi } from "@/lib/api";
import type { HealthResponse, ProxyStatus, StoredSubscription } from "@/lib/types";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  CardFooter,
} from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Activity,
  Play,
  Square,
  Plus,
  Server,
  ArrowRight,
  AlertCircle,
} from "lucide-react";

export default function DashboardPage() {
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [proxy, setProxy] = useState<ProxyStatus | null>(null);
  const [subscriptions, setSubscriptions] = useState<StoredSubscription[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [addOpen, setAddOpen] = useState(false);
  const [addUrl, setAddUrl] = useState("");
  const [addName, setAddName] = useState("");
  const [addLoading, setAddLoading] = useState(false);

  const fetchAll = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [h, p, subs] = await Promise.all([
        IronpassApi.health().catch(() => null),
        IronpassApi.proxyStatus().catch(() => null),
        IronpassApi.listSubscriptions().catch(() => []),
      ]);
      setHealth(h);
      setProxy(p);
      setSubscriptions(subs);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load dashboard");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchAll();
    const interval = setInterval(() => {
      IronpassApi.proxyStatus().then(setProxy).catch(console.error);
    }, 3000);
    return () => clearInterval(interval);
  }, [fetchAll]);

  async function toggleProxy() {
    if (!proxy) return;
    try {
      const next = proxy.running
        ? await IronpassApi.stopProxy()
        : await IronpassApi.startProxy({});
      setProxy(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Proxy action failed");
    }
  }

  async function handleAddSubscription(e: React.FormEvent) {
    e.preventDefault();
    if (!addUrl.trim()) return;
    setAddLoading(true);
    try {
      await IronpassApi.addSubscription(addUrl.trim(), addName.trim() || null, null);
      setAddUrl("");
      setAddName("");
      setAddOpen(false);
      await fetchAll();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to add subscription");
    } finally {
      setAddLoading(false);
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Dashboard</h1>
          <p className="text-muted-foreground">Overview of IronPass status and quick actions.</p>
        </div>
        <div className="flex items-center gap-2">
          <Dialog open={addOpen} onOpenChange={setAddOpen}>
            <DialogTrigger asChild>
              <Button variant="outline">
                <Plus className="mr-2 size-4" />
                Add subscription
              </Button>
            </DialogTrigger>
            <DialogContent>
              <form onSubmit={handleAddSubscription}>
                <DialogHeader>
                  <DialogTitle>Add subscription</DialogTitle>
                  <DialogDescription>
                    Enter the subscription URL and an optional name.
                  </DialogDescription>
                </DialogHeader>
                <div className="grid gap-4 py-4">
                  <div className="grid gap-2">
                    <Label htmlFor="sub-url">URL</Label>
                    <Input
                      id="sub-url"
                      type="url"
                      placeholder="https://example.com/sub"
                      value={addUrl}
                      onChange={(e) => setAddUrl(e.target.value)}
                      required
                    />
                  </div>
                  <div className="grid gap-2">
                    <Label htmlFor="sub-name">Name (optional)</Label>
                    <Input
                      id="sub-name"
                      placeholder="My subscription"
                      value={addName}
                      onChange={(e) => setAddName(e.target.value)}
                    />
                  </div>
                </div>
                <DialogFooter>
                  <Button type="submit" disabled={addLoading}>
                    {addLoading ? "Adding..." : "Add"}
                  </Button>
                </DialogFooter>
              </form>
            </DialogContent>
          </Dialog>
          <Button onClick={fetchAll} variant="ghost">
            Refresh
          </Button>
        </div>
      </div>

      {error && (
        <div className="flex items-center gap-2 rounded-md border border-destructive/50 bg-destructive/10 p-3 text-sm text-destructive">
          <AlertCircle className="size-4" />
          {error}
        </div>
      )}

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium">Health</CardTitle>
            <Activity className="size-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {loading ? (
              <Skeleton className="h-8 w-24" />
            ) : health ? (
              <>
                <div className="text-2xl font-bold">{health.status}</div>
                <p className="text-xs text-muted-foreground">
                  v{health.version} · uptime {formatDuration(health.uptime_secs)}
                </p>
              </>
            ) : (
              <div className="text-sm text-muted-foreground">Unavailable</div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium">Proxy</CardTitle>
            <Server className="size-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {loading ? (
              <Skeleton className="h-8 w-24" />
            ) : proxy ? (
              <>
                <div
                  className={`text-2xl font-bold ${
                    proxy.running ? "text-green-600" : "text-muted-foreground"
                  }`}
                >
                  {proxy.running ? "Running" : "Stopped"}
                </div>
                <p className="text-xs text-muted-foreground">
                  {proxy.selected_node?.node.name ?? "No node selected"}
                </p>
              </>
            ) : (
              <div className="text-sm text-muted-foreground">Unavailable</div>
            )}
          </CardContent>
          <CardFooter className="pt-0">
            <Button
              size="sm"
              variant={proxy?.running ? "destructive" : "default"}
              onClick={toggleProxy}
              disabled={!proxy}
              className="w-full"
            >
              {proxy?.running ? (
                <>
                  <Square className="mr-2 size-4" /> Stop
                </>
              ) : (
                <>
                  <Play className="mr-2 size-4" /> Start
                </>
              )}
            </Button>
          </CardFooter>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium">Subscriptions</CardTitle>
            <Server className="size-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {loading ? (
              <Skeleton className="h-8 w-16" />
            ) : (
              <>
                <div className="text-2xl font-bold">{subscriptions.length}</div>
                <p className="text-xs text-muted-foreground">
                  {subscriptions.reduce((acc, s) => acc + (s.metadata?.expire ? 1 : 0), 0)}{" "}
                  with expiry info
                </p>
              </>
            )}
          </CardContent>
          <CardFooter className="pt-0">
            <Button size="sm" variant="outline" className="w-full" asChild>
              <Link href="/subscriptions">
                Manage <ArrowRight className="ml-2 size-4" />
              </Link>
            </Button>
          </CardFooter>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Quick links</CardTitle>
          <CardDescription>Navigate to common sections.</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
          <Button variant="outline" asChild>
            <Link href="/subscriptions">Subscriptions</Link>
          </Button>
          <Button variant="outline" asChild>
            <Link href="/nodes">Nodes</Link>
          </Button>
          <Button variant="outline" asChild>
            <Link href="/proxy">Proxy control</Link>
          </Button>
          <Button variant="outline" asChild>
            <Link href="/split-tunnel">Split tunnel</Link>
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}

function formatDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

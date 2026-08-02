"use client";

import { useEffect, useState, useCallback } from "react";
import { IronpassApi } from "@/lib/api";
import type { HwidResponse } from "@/lib/types";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { AlertCircle, Fingerprint, RefreshCw } from "lucide-react";

export default function HwidPage() {
  const [hwid, setHwid] = useState<HwidResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [regenerating, setRegenerating] = useState(false);

  const fetchHwid = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await IronpassApi.getHwid();
      setHwid(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load HWID");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchHwid();
  }, [fetchHwid]);

  async function handleRegenerate() {
    if (!confirm("Regenerate HWID? This may invalidate existing subscriptions.")) return;
    setRegenerating(true);
    setError(null);
    try {
      const data = await IronpassApi.regenerateHwid();
      setHwid(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to regenerate HWID");
    } finally {
      setRegenerating(false);
    }
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">HWID</h1>
        <p className="text-muted-foreground">Hardware identifier and device info.</p>
      </div>

      {error && (
        <div className="flex items-center gap-2 rounded-md border border-destructive/50 bg-destructive/10 p-3 text-sm text-destructive">
          <AlertCircle className="size-4" />
          {error}
        </div>
      )}

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Fingerprint className="size-5" />
            Hardware ID
          </CardTitle>
          <CardDescription>
            This identifier is used for subscription binding.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {loading ? (
            <Skeleton className="h-8 w-full" />
          ) : hwid ? (
            <>
              <div className="rounded-md bg-muted p-3 font-mono text-sm break-all">
                {hwid.hwid}
              </div>
              <div className="grid gap-4 sm:grid-cols-3">
                <div>
                  <div className="text-xs font-medium text-muted-foreground">Machine ID</div>
                  <div className="text-sm font-semibold">
                    {hwid.info.machine_id ?? "—"}
                  </div>
                </div>
                <div>
                  <div className="text-xs font-medium text-muted-foreground">OS</div>
                  <div className="text-sm font-semibold">{hwid.info.os ?? "—"}</div>
                </div>
                <div>
                  <div className="text-xs font-medium text-muted-foreground">Hostname</div>
                  <div className="text-sm font-semibold">
                    {hwid.info.hostname ?? "—"}
                  </div>
                </div>
              </div>
            </>
          ) : (
            <div className="text-sm text-muted-foreground">Unable to load HWID.</div>
          )}
          <div className="flex gap-2">
            <Button onClick={handleRegenerate} disabled={regenerating || loading}>
              <RefreshCw className="mr-2 size-4" />
              {regenerating ? "Regenerating..." : "Regenerate HWID"}
            </Button>
            <Button variant="outline" onClick={fetchHwid} disabled={loading}>
              Refresh
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

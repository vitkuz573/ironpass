"use client";

import { useEffect, useState, useCallback } from "react";
import { IronpassApi } from "@/lib/api";
import type { AppConfig } from "@/lib/types";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "sonner";

export default function ConfigPage() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [loading, setLoading] = useState(true);

  const fetchConfig = useCallback(async () => {
    setLoading(true);
    try {
      const data = await IronpassApi.getConfig();
      setConfig(data);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to load config");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchConfig();
  }, [fetchConfig]);

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Config</h1>
        <p className="text-muted-foreground">Current application configuration.</p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Config path</CardTitle>
          </CardHeader>
          <CardContent>
            <code className="rounded bg-muted px-2 py-1 text-sm break-all">
              {config?.config_path ?? "—"}
            </code>
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle>Data directory</CardTitle>
          </CardHeader>
          <CardContent>
            <code className="rounded bg-muted px-2 py-1 text-sm break-all">
              {config?.data_dir ?? "—"}
            </code>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Configuration JSON</CardTitle>
          <CardDescription>Read-only view of the active config.</CardDescription>
        </CardHeader>
        <CardContent>
          {loading ? (
            <Skeleton className="h-64 w-full" />
          ) : config ? (
            <pre className="max-h-[600px] overflow-auto rounded-md bg-muted p-4 text-sm font-mono">
              {JSON.stringify(config, null, 2)}
            </pre>
          ) : (
            <div className="text-sm text-muted-foreground">Unable to load config.</div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

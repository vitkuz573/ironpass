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
import { Fingerprint, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/confirm-dialog";

export default function HwidPage() {
  const [hwid, setHwid] = useState<HwidResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [regenerating, setRegenerating] = useState(false);
  const [regenerateOpen, setRegenerateOpen] = useState(false);

  const fetchHwid = useCallback(async () => {
    setLoading(true);
    try {
      const data = await IronpassApi.getHwid();
      setHwid(data);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to load HWID");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchHwid();
  }, [fetchHwid]);

  async function handleRegenerate() {
    setRegenerating(true);
    try {
      const data = await IronpassApi.regenerateHwid();
      setHwid(data);
      setRegenerateOpen(false);
      toast.success("HWID regenerated");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to regenerate HWID");
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
            <Button onClick={() => setRegenerateOpen(true)} disabled={regenerating || loading}>
              <RefreshCw className="mr-2 size-4" />
              {regenerating ? "Regenerating..." : "Regenerate HWID"}
            </Button>
            <Button variant="outline" onClick={fetchHwid} disabled={loading}>
              Refresh
            </Button>
          </div>
        </CardContent>
      </Card>

      <ConfirmDialog
        open={regenerateOpen}
        onOpenChange={setRegenerateOpen}
        title="Regenerate HWID"
        description="Regenerating your HWID may invalidate existing subscriptions. Are you sure?"
        confirmText="Regenerate"
        cancelText="Cancel"
        onConfirm={handleRegenerate}
        variant="destructive"
      />
    </div>
  );
}

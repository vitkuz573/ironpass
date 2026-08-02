"use client";

import { useEffect, useState, useCallback } from "react";
import { useParams, useRouter } from "next/navigation";
import Link from "next/link";
import { IronpassApi } from "@/lib/api";
import type { NodeWithSubscription, SubscriptionDetail } from "@/lib/types";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Skeleton } from "@/components/ui/skeleton";
import { ArrowLeft, Check, Play, RefreshCw, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/confirm-dialog";

export default function SubscriptionDetailPage() {
  const params = useParams();
  const router = useRouter();
  const id = typeof params.id === "string" ? params.id : "";

  const [detail, setDetail] = useState<SubscriptionDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [selectingId, setSelectingId] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [deleteOpen, setDeleteOpen] = useState(false);

  const fetchDetail = useCallback(async () => {
    if (!id) return;
    setLoading(true);
    try {
      const data = await IronpassApi.getSubscription(id);
      setDetail(data);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to load subscription");
    } finally {
      setLoading(false);
    }
  }, [id]);

  useEffect(() => {
    fetchDetail();
  }, [fetchDetail]);

  async function handleRefresh() {
    if (!id) return;
    try {
      await IronpassApi.fetchSubscription(id, null);
      toast.success("Subscription refreshed");
      await fetchDetail();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to refresh subscription");
    }
  }

  async function handleDelete() {
    if (!id) return;
    try {
      await IronpassApi.deleteSubscription(id);
      toast.success("Subscription deleted");
      router.push("/subscriptions");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to delete subscription");
    } finally {
      setDeleteOpen(false);
    }
  }

  async function handleSelectNode(nodeId: string) {
    setSelectingId(nodeId);
    try {
      await IronpassApi.selectNode(nodeId);
      setSelectedId(nodeId);
      toast.success("Node selected");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to select node");
    } finally {
      setSelectingId(null);
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-center gap-2">
          <Button variant="ghost" size="icon" asChild>
            <Link href="/subscriptions">
              <ArrowLeft className="size-4" />
            </Link>
          </Button>
          <div>
            <h1 className="text-2xl font-bold tracking-tight">
              {detail?.subscription.name ?? "Subscription"}
            </h1>
            <p className="text-muted-foreground">{detail?.subscription.url}</p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" onClick={handleRefresh}>
            <RefreshCw className="mr-2 size-4" />
            Refresh
          </Button>
          <Button variant="destructive" onClick={() => setDeleteOpen(true)}>
            <Trash2 className="mr-2 size-4" />
            Delete
          </Button>
        </div>
      </div>

      {loading ? (
        <div className="space-y-4">
          <Skeleton className="h-32 w-full" />
          <Skeleton className="h-64 w-full" />
        </div>
      ) : detail ? (
        <>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm font-medium">Status</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-xl font-bold">
                  {detail.subscription.is_active ? "Active" : "Inactive"}
                </div>
              </CardContent>
            </Card>
            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm font-medium">Added</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-xl font-bold">
                  {formatDate(detail.subscription.added_at)}
                </div>
              </CardContent>
            </Card>
            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm font-medium">Last updated</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-xl font-bold">
                  {detail.subscription.last_updated
                    ? formatDate(detail.subscription.last_updated)
                    : "Never"}
                </div>
              </CardContent>
            </Card>
            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm font-medium">Nodes</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-xl font-bold">{detail.nodes.length}</div>
              </CardContent>
            </Card>
          </div>

          <Card>
            <CardHeader>
              <CardTitle>Nodes</CardTitle>
              <CardDescription>
                Select a node to use for proxying.
              </CardDescription>
            </CardHeader>
            <CardContent>
              {detail.nodes.length === 0 ? (
                <div className="text-sm text-muted-foreground">
                  No nodes found. Try refreshing the subscription.
                </div>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Name</TableHead>
                      <TableHead>Protocol</TableHead>
                      <TableHead>Address</TableHead>
                      <TableHead className="w-32" />
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {detail.nodes.map((node) => (
                      <NodeRow
                        key={node.id}
                        node={node}
                        isSelected={selectedId === node.id}
                        isSelecting={selectingId === node.id}
                        onSelect={() => handleSelectNode(node.id)}
                      />
                    ))}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </Card>
        </>
      ) : (
        <div className="text-sm text-muted-foreground">Subscription not found.</div>
      )}

      <ConfirmDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        title="Delete subscription"
        description="Are you sure you want to delete this subscription? This action cannot be undone."
        confirmText="Delete"
        cancelText="Cancel"
        onConfirm={handleDelete}
        variant="destructive"
      />
    </div>
  );
}

function NodeRow({
  node,
  isSelected,
  isSelecting,
  onSelect,
}: {
  node: NodeWithSubscription;
  isSelected: boolean;
  isSelecting: boolean;
  onSelect: () => void;
}) {
  return (
    <TableRow>
      <TableCell className="font-medium">{node.node.name}</TableCell>
      <TableCell className="text-muted-foreground">
        {node.node.protocol ?? "—"}
      </TableCell>
      <TableCell className="text-muted-foreground">
        {node.node.address ? `${node.node.address}:${node.node.port ?? ""}` : "—"}
      </TableCell>
      <TableCell>
        <Button
          size="sm"
          variant={isSelected ? "secondary" : "outline"}
          onClick={onSelect}
          disabled={isSelecting}
        >
          {isSelecting ? (
            "..."
          ) : isSelected ? (
            <>
              <Check className="mr-1 size-4" /> Selected
            </>
          ) : (
            <>
              <Play className="mr-1 size-4" /> Select
            </>
          )}
        </Button>
      </TableCell>
    </TableRow>
  );
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleString();
}

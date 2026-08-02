"use client";

import { useEffect, useState, useCallback } from "react";
import Link from "next/link";
import { IronpassApi } from "@/lib/api";
import type { StoredSubscription } from "@/lib/types";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
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
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Skeleton } from "@/components/ui/skeleton";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Plus, MoreHorizontal, RefreshCw, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/confirm-dialog";

export default function SubscriptionsPage() {
  const [subscriptions, setSubscriptions] = useState<StoredSubscription[]>([]);
  const [loading, setLoading] = useState(true);

  const [addOpen, setAddOpen] = useState(false);
  const [deleteId, setDeleteId] = useState<string | null>(null);
  const [addUrl, setAddUrl] = useState("");
  const [addName, setAddName] = useState("");
  const [addLoading, setAddLoading] = useState(false);

  const fetchSubscriptions = useCallback(async () => {
    setLoading(true);
    try {
      const data = await IronpassApi.listSubscriptions();
      setSubscriptions(data);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to load subscriptions");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchSubscriptions();
  }, [fetchSubscriptions]);

  async function handleAdd(e: React.FormEvent) {
    e.preventDefault();
    if (!addUrl.trim()) return;
    setAddLoading(true);
    try {
      await IronpassApi.addSubscription(addUrl.trim(), addName.trim() || null, null);
      setAddUrl("");
      setAddName("");
      setAddOpen(false);
      toast.success("Subscription added");
      await fetchSubscriptions();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to add subscription");
    } finally {
      setAddLoading(false);
    }
  }

  async function handleRefresh(id: string) {
    try {
      await IronpassApi.fetchSubscription(id, null);
      toast.success("Subscription refreshed");
      await fetchSubscriptions();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to refresh subscription");
    }
  }

  async function handleDelete() {
    if (!deleteId) return;
    try {
      await IronpassApi.deleteSubscription(deleteId);
      setDeleteId(null);
      toast.success("Subscription deleted");
      await fetchSubscriptions();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to delete subscription");
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Subscriptions</h1>
          <p className="text-muted-foreground">Manage your proxy subscriptions.</p>
        </div>
        <div className="flex items-center gap-2">
          <Dialog open={addOpen} onOpenChange={setAddOpen}>
            <DialogTrigger asChild>
              <Button>
                <Plus className="mr-2 size-4" />
                Add subscription
              </Button>
            </DialogTrigger>
            <DialogContent>
              <form onSubmit={handleAdd}>
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
          <Button variant="outline" onClick={fetchSubscriptions}>
            Refresh
          </Button>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>All subscriptions</CardTitle>
          <CardDescription>
            Click a subscription to view its nodes and details.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {loading ? (
            <div className="space-y-2">
              <Skeleton className="h-10 w-full" />
              <Skeleton className="h-10 w-full" />
              <Skeleton className="h-10 w-full" />
            </div>
          ) : subscriptions.length === 0 ? (
            <div className="text-sm text-muted-foreground">
              No subscriptions yet. Add one to get started.
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>URL</TableHead>
                  <TableHead>Last updated</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead className="w-12" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {subscriptions.map((sub) => (
                  <TableRow key={sub.id}>
                    <TableCell className="font-medium">
                      <Link
                        href={`/subscriptions/${sub.id}`}
                        className="hover:underline"
                      >
                        {sub.name ?? "Unnamed"}
                      </Link>
                    </TableCell>
                    <TableCell className="max-w-xs truncate" title={sub.url}>
                      {sub.url}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {sub.last_updated ? formatDate(sub.last_updated) : "Never"}
                    </TableCell>
                    <TableCell>
                      {sub.is_active ? (
                        <span className="inline-flex items-center rounded-full bg-green-100 px-2 py-0.5 text-xs font-medium text-green-800 dark:bg-green-900 dark:text-green-100">
                          Active
                        </span>
                      ) : (
                        <span className="inline-flex items-center rounded-full bg-muted px-2 py-0.5 text-xs font-medium">
                          Inactive
                        </span>
                      )}
                    </TableCell>
                    <TableCell>
                      <DropdownMenu>
                        <DropdownMenuTrigger asChild>
                          <Button variant="ghost" size="icon">
                            <MoreHorizontal className="size-4" />
                          </Button>
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                          <DropdownMenuItem asChild>
                            <Link href={`/subscriptions/${sub.id}`}>View details</Link>
                          </DropdownMenuItem>
                          <DropdownMenuItem onClick={() => handleRefresh(sub.id)}>
                            <RefreshCw className="mr-2 size-4" /> Refresh
                          </DropdownMenuItem>
                          <DropdownMenuItem
                            onClick={() => setDeleteId(sub.id)}
                            className="text-destructive focus:text-destructive"
                          >
                            <Trash2 className="mr-2 size-4" /> Delete
                          </DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(open) => !open && setDeleteId(null)}
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

function formatDate(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleString();
}

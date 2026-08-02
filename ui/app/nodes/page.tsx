"use client";

import { useEffect, useMemo, useState, useCallback } from "react";
import { IronpassApi } from "@/lib/api";
import type { NodeWithSubscription, StoredSubscription } from "@/lib/types";
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
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Skeleton } from "@/components/ui/skeleton";
import { AlertCircle, Check, Play } from "lucide-react";

export default function NodesPage() {
  const [nodes, setNodes] = useState<NodeWithSubscription[]>([]);
  const [subscriptions, setSubscriptions] = useState<StoredSubscription[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedFilter, setSelectedFilter] = useState<string>("all");
  const [search, setSearch] = useState("");
  const [selectingId, setSelectingId] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [n, subs] = await Promise.all([
        IronpassApi.listNodes(),
        IronpassApi.listSubscriptions(),
      ]);
      setNodes(n);
      setSubscriptions(subs);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load nodes");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const filteredNodes = useMemo(() => {
    let result = nodes;
    if (selectedFilter !== "all") {
      result = result.filter((n) => n.subscription_id === selectedFilter);
    }
    const term = search.trim().toLowerCase();
    if (term) {
      result = result.filter(
        (n) =>
          n.node.name.toLowerCase().includes(term) ||
          (n.node.protocol?.toLowerCase() ?? "").includes(term) ||
          (n.node.address?.toLowerCase() ?? "").includes(term)
      );
    }
    return result;
  }, [nodes, selectedFilter, search]);

  async function handleSelectNode(id: string) {
    setSelectingId(id);
    try {
      await IronpassApi.selectNode(id);
      setSelectedId(id);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to select node");
    } finally {
      setSelectingId(null);
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Nodes</h1>
          <p className="text-muted-foreground">Browse and select proxy nodes.</p>
        </div>
        <Button variant="outline" onClick={fetchData}>
          Refresh
        </Button>
      </div>

      {error && (
        <div className="flex items-center gap-2 rounded-md border border-destructive/50 bg-destructive/10 p-3 text-sm text-destructive">
          <AlertCircle className="size-4" />
          {error}
        </div>
      )}

      <Card>
        <CardHeader>
          <CardTitle>All nodes</CardTitle>
          <CardDescription>
            Filter by subscription or search by name, protocol, or address.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-col gap-4 sm:flex-row">
            <div className="grid gap-2 sm:w-64">
              <Label htmlFor="sub-filter">Subscription</Label>
              <Select value={selectedFilter} onValueChange={setSelectedFilter}>
                <SelectTrigger id="sub-filter">
                  <SelectValue placeholder="All subscriptions" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All subscriptions</SelectItem>
                  {subscriptions.map((sub) => (
                    <SelectItem key={sub.id} value={sub.id}>
                      {sub.name ?? sub.url}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="grid gap-2 flex-1">
              <Label htmlFor="search">Search</Label>
              <Input
                id="search"
                placeholder="Search nodes..."
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
            </div>
          </div>

          {loading ? (
            <div className="space-y-2">
              <Skeleton className="h-10 w-full" />
              <Skeleton className="h-10 w-full" />
              <Skeleton className="h-10 w-full" />
            </div>
          ) : filteredNodes.length === 0 ? (
            <div className="text-sm text-muted-foreground">No nodes found.</div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Subscription</TableHead>
                  <TableHead>Protocol</TableHead>
                  <TableHead>Address</TableHead>
                  <TableHead className="w-32" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {filteredNodes.map((node) => (
                  <TableRow key={node.id}>
                    <TableCell className="font-medium">{node.node.name}</TableCell>
                    <TableCell className="text-muted-foreground">
                      {node.subscription_name ?? "—"}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {node.node.protocol ?? "—"}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {node.node.address
                        ? `${node.node.address}:${node.node.port ?? ""}`
                        : "—"}
                    </TableCell>
                    <TableCell>
                      <Button
                        size="sm"
                        variant={selectedId === node.id ? "secondary" : "outline"}
                        onClick={() => handleSelectNode(node.id)}
                        disabled={selectingId === node.id}
                      >
                        {selectingId === node.id ? (
                          "..."
                        ) : selectedId === node.id ? (
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
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

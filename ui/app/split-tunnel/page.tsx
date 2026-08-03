"use client";

import { useEffect, useState, useCallback } from "react";
import { IronpassApi, splitTunnelTargetOptions, splitTunnelActionOptions } from "@/lib/api";
import type {
  NodeWithSubscription,
  SplitTunnelAction,
  SplitTunnelRule,
  SplitTunnelTarget,
} from "@/lib/types";
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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Pencil, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/confirm-dialog";

export default function SplitTunnelPage() {
  const [rules, setRules] = useState<SplitTunnelRule[]>([]);
  const [nodes, setNodes] = useState<NodeWithSubscription[]>([]);
  const [loading, setLoading] = useState(true);

  const [target, setTarget] = useState<SplitTunnelTarget>("domain");
  const [value, setValue] = useState("");
  const [action, setAction] = useState<SplitTunnelAction>("direct");
  const [nodeId, setNodeId] = useState<string>("");
  const [submitting, setSubmitting] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const [editing, setEditing] = useState<SplitTunnelRule | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  function validateValue(): string | null {
    if (!value.trim()) return "Value is required";
    if (target === "ip") {
      const parts = value.trim().split(".");
      if (parts.length !== 4 || parts.some((p) => Number.isNaN(Number(p)) || Number(p) < 0 || Number(p) > 255)) {
        return "Invalid IPv4 address";
      }
    }
    if (target === "cidr") {
      const [ip, prefix] = value.trim().split("/");
      if (!ip || !prefix) return "Invalid CIDR (expected e.g. 10.0.0.0/8)";
      const parts = ip.split(".");
      if (parts.length !== 4 || parts.some((p) => Number.isNaN(Number(p)) || Number(p) < 0 || Number(p) > 255)) {
        return "Invalid CIDR base address";
      }
      const n = Number(prefix);
      if (Number.isNaN(n) || n < 0 || n > 32) return "Invalid CIDR prefix";
    }
    return null;
  }

  const fetchData = useCallback(async () => {
    setLoading(true);
    try {
      const [r, n] = await Promise.all([
        IronpassApi.listSplitTunnelRules(),
        IronpassApi.listNodes(),
      ]);
      setRules(r);
      setNodes(n);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to load split tunnel rules");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  function resetForm() {
    setTarget("domain");
    setValue("");
    setAction("direct");
    setNodeId("");
    setFormError(null);
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const error = validateValue();
    if (error) {
      setFormError(error);
      return;
    }
    setFormError(null);
    setSubmitting(true);
    try {
      await IronpassApi.addSplitTunnelRule(
        target,
        value.trim(),
        action,
        nodeId || null
      );
      resetForm();
      toast.success("Rule added");
      await fetchData();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to add rule");
    } finally {
      setSubmitting(false);
    }
  }

  function validateEditingValue(): string | null {
    if (!editing) return null;
    if (!editing.value.trim()) return "Value is required";
    if (editing.target === "ip") {
      const parts = editing.value.trim().split(".");
      if (parts.length !== 4 || parts.some((p) => Number.isNaN(Number(p)) || Number(p) < 0 || Number(p) > 255)) {
        return "Invalid IPv4 address";
      }
    }
    if (editing.target === "cidr") {
      const [ip, prefix] = editing.value.trim().split("/");
      if (!ip || !prefix) return "Invalid CIDR (expected e.g. 10.0.0.0/8)";
      const parts = ip.split(".");
      if (parts.length !== 4 || parts.some((p) => Number.isNaN(Number(p)) || Number(p) < 0 || Number(p) > 255)) {
        return "Invalid CIDR base address";
      }
      const n = Number(prefix);
      if (Number.isNaN(n) || n < 0 || n > 32) return "Invalid CIDR prefix";
    }
    return null;
  }

  async function handleUpdate(e: React.FormEvent) {
    e.preventDefault();
    if (!editing) return;
    const error = validateEditingValue();
    if (error) {
      toast.error(error);
      return;
    }
    setSubmitting(true);
    try {
      await IronpassApi.updateSplitTunnelRule(
        editing.id,
        editing.target,
        editing.value,
        editing.action,
        editing.node_id || null
      );
      setEditing(null);
      toast.success("Rule updated");
      await fetchData();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to update rule");
    } finally {
      setSubmitting(false);
    }
  }

  async function handleDelete() {
    if (!deleteId) return;
    try {
      await IronpassApi.deleteSplitTunnelRule(deleteId);
      setDeleteId(null);
      toast.success("Rule deleted");
      await fetchData();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to delete rule");
    }
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Split Tunnel</h1>
        <p className="text-muted-foreground">Configure per-target routing rules.</p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Add rule</CardTitle>
          <CardDescription>
            Create a new split tunnel rule for domains, IPs, CIDRs, or apps.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
              <div className="grid gap-2">
                <Label htmlFor="target-type">Target type</Label>
                <Select
                  value={target}
                  onValueChange={(v) => {
                    setTarget(v as SplitTunnelTarget);
                    setValue("");
                    if (formError) setFormError(null);
                  }}
                >
                  <SelectTrigger id="target-type">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {splitTunnelTargetOptions.map((opt) => (
                      <SelectItem key={opt.value} value={opt.value}>
                        {opt.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="grid gap-2">
                <Label htmlFor="target-value">Value</Label>
                <Input
                  id="target-value"
                  placeholder={target === "ip" ? "1.2.3.4" : target === "cidr" ? "10.0.0.0/8" : "example.com"}
                  value={value}
                  onChange={(e) => {
                    setValue(e.target.value);
                    if (formError) setFormError(null);
                  }}
                  required
                />
              </div>
              <div className="grid gap-2">
                <Label htmlFor="action">Action</Label>
                <Select
                  value={action}
                  onValueChange={(v) => setAction(v as SplitTunnelAction)}
                >
                  <SelectTrigger id="action">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {splitTunnelActionOptions.map((opt) => (
                      <SelectItem key={opt.value} value={opt.value}>
                        {opt.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="grid gap-2">
                <Label htmlFor="node-scope">Node scope (optional)</Label>
                <Select value={nodeId} onValueChange={setNodeId}>
                  <SelectTrigger id="node-scope">
                    <SelectValue placeholder="Global" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="">Global</SelectItem>
                    {nodes.map((node) => (
                      <SelectItem key={node.id} value={node.id}>
                        {node.node.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </div>
            {formError && (
              <p className="text-sm text-destructive">{formError}</p>
            )}
            <Button type="submit" disabled={submitting}>
              Add rule
            </Button>
          </form>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Rules</CardTitle>
          <CardDescription>Existing split tunnel rules.</CardDescription>
        </CardHeader>
        <CardContent>
          {loading ? (
            <div className="space-y-2">
              <Skeleton className="h-10 w-full" />
              <Skeleton className="h-10 w-full" />
              <Skeleton className="h-10 w-full" />
            </div>
          ) : rules.length === 0 ? (
            <div className="text-sm text-muted-foreground">No rules yet.</div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Target</TableHead>
                  <TableHead>Value</TableHead>
                  <TableHead>Action</TableHead>
                  <TableHead>Node scope</TableHead>
                  <TableHead className="w-24" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {rules.map((rule) => (
                  <TableRow key={rule.id}>
                    <TableCell className="font-medium capitalize">{rule.target}</TableCell>
                    <TableCell>{rule.value}</TableCell>
                    <TableCell className="capitalize">{rule.action}</TableCell>
                    <TableCell className="text-muted-foreground">
                      {nodes.find((n) => n.id === rule.node_id)?.node.name ?? "Global"}
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center gap-1">
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() => setEditing(rule)}
                        >
                          <Pencil className="size-4" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() => setDeleteId(rule.id)}
                        >
                          <Trash2 className="size-4 text-destructive" />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      <Dialog open={!!editing} onOpenChange={(open) => !open && setEditing(null)}>
        <DialogContent>
          {editing && (
            <form onSubmit={handleUpdate}>
              <DialogHeader>
                <DialogTitle>Edit rule</DialogTitle>
                <DialogDescription>Update the split tunnel rule.</DialogDescription>
              </DialogHeader>
              <div className="grid gap-4 py-4">
              <div className="grid gap-2">
                <Label htmlFor="edit-target">Target type</Label>
                <Select
                  value={editing.target}
                  onValueChange={(v) =>
                    setEditing({ ...editing, target: v as SplitTunnelTarget, value: "" })
                  }
                >
                    <SelectTrigger id="edit-target">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {splitTunnelTargetOptions.map((opt) => (
                        <SelectItem key={opt.value} value={opt.value}>
                          {opt.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <div className="grid gap-2">
                  <Label htmlFor="edit-value">Value</Label>
                  <Input
                    id="edit-value"
                    placeholder={editing.target === "ip" ? "1.2.3.4" : editing.target === "cidr" ? "10.0.0.0/8" : "example.com"}
                    value={editing.value}
                    onChange={(e) =>
                      setEditing({ ...editing, value: e.target.value })
                    }
                    required
                  />
                </div>
                <div className="grid gap-2">
                  <Label htmlFor="edit-action">Action</Label>
                  <Select
                    value={editing.action}
                    onValueChange={(v) =>
                      setEditing({ ...editing, action: v as SplitTunnelAction })
                    }
                  >
                    <SelectTrigger id="edit-action">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {splitTunnelActionOptions.map((opt) => (
                        <SelectItem key={opt.value} value={opt.value}>
                          {opt.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <div className="grid gap-2">
                  <Label htmlFor="edit-node">Node scope (optional)</Label>
                  <Select
                    value={editing.node_id ?? ""}
                    onValueChange={(v) =>
                      setEditing({ ...editing, node_id: v || null })
                    }
                  >
                    <SelectTrigger id="edit-node">
                      <SelectValue placeholder="Global" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="">Global</SelectItem>
                      {nodes.map((node) => (
                        <SelectItem key={node.id} value={node.id}>
                          {node.node.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </div>
              <DialogFooter>
                <Button type="button" variant="outline" onClick={() => setEditing(null)}>
                  Cancel
                </Button>
                <Button type="submit" disabled={submitting}>
                  Save changes
                </Button>
              </DialogFooter>
            </form>
          )}
        </DialogContent>
      </Dialog>

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(open) => !open && setDeleteId(null)}
        title="Delete rule"
        description="Are you sure you want to delete this split tunnel rule? This action cannot be undone."
        confirmText="Delete"
        cancelText="Cancel"
        onConfirm={handleDelete}
        variant="destructive"
      />
    </div>
  );
}

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Trash2Icon } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  createQueryWhitelistEntry,
  deleteQueryWhitelistEntry,
  listQueryWhitelist,
} from "@/api";
import { openDialog, useDialog } from "@/hooks/useDialog";
import type { QueryWhitelistEntry } from "@/types";

export function QueryParamSection() {
  const queryClient = useQueryClient();
  const [domain, setDomain] = useState("");
  const [param, setParam] = useState("");
  const addQueryWhitelistDialog = useDialog("add-query-whitelist");

  const entriesQuery = useQuery({
    queryKey: ["queryWhitelist"],
    queryFn: listQueryWhitelist,
  });
  const entries = entriesQuery.data ?? [];

  function invalidate() {
    queryClient.invalidateQueries({ queryKey: ["queryWhitelist"] });
  }

  const createMutation = useMutation({
    mutationFn: () => createQueryWhitelistEntry(domain.trim(), param.trim()),
    onSuccess: () => {
      setDomain("");
      setParam("");
      addQueryWhitelistDialog.setOpen(false);
      invalidate();
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (entry: QueryWhitelistEntry) =>
      deleteQueryWhitelistEntry(entry.id),
    onSuccess: invalidate,
  });

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!domain.trim() || !param.trim()) return;
    createMutation.mutate();
  }

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Kept URL parameters</CardTitle>
        <CardDescription>
          Every query-string parameter is stripped from tracked URLs by default.
          Whitelist one per domain to keep it — e.g. youtube.com's{" "}
          <span className="font-mono">v</span> so different videos become
          different activities instead of one continuous block.
        </CardDescription>
        <CardAction>
          <Button size="sm" onClick={() => openDialog("add-query-whitelist")}>
            Add
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        {!entriesQuery.isLoading && entries.length === 0 && (
          <p className="text-sm text-muted-foreground">
            Nothing whitelisted — query strings are always stripped.
          </p>
        )}

        <div className="flex flex-wrap items-center gap-1.5">
          {entries.map((entry) => (
            <Badge key={entry.id} variant="secondary" className="h-auto py-0.5">
              <span className="font-mono">{entry.domain}</span>
              <span className="text-muted-foreground">?</span>
              <span className="font-mono">{entry.param}</span>
              <button
                type="button"
                onClick={() => deleteMutation.mutate(entry)}
                aria-label={`Remove ${entry.domain} ?${entry.param}`}
                className="text-muted-foreground hover:text-destructive"
              >
                <Trash2Icon className="size-3" />
              </button>
            </Badge>
          ))}
        </div>
      </CardContent>

      <Dialog
        open={addQueryWhitelistDialog.open}
        onOpenChange={addQueryWhitelistDialog.setOpen}
      >
        <DialogContent>
          <form onSubmit={handleSubmit} className="flex flex-col gap-4">
            <DialogHeader>
              <DialogTitle>Keep a URL parameter</DialogTitle>
            </DialogHeader>
            <div className="flex flex-wrap items-center gap-1.5">
              <Input
                type="text"
                value={domain}
                onChange={(e) => setDomain(e.target.value)}
                placeholder="youtube.com"
                className="w-40 flex-1 font-mono"
                aria-label="Domain"
                autoFocus
              />
              <Input
                type="text"
                value={param}
                onChange={(e) => setParam(e.target.value)}
                placeholder="v"
                className="w-28 font-mono"
                aria-label="Query parameter"
              />
            </div>
            {createMutation.isError && (
              <p className="text-xs text-destructive">
                {String(createMutation.error)}
              </p>
            )}
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => addQueryWhitelistDialog.setOpen(false)}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                disabled={
                  !domain.trim() || !param.trim() || createMutation.isPending
                }
              >
                Add
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </Card>
  );
}

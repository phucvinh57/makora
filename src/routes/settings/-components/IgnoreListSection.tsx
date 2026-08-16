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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { createIgnoreRule, deleteIgnoreRule, listIgnoreRules } from "@/api";
import { openDialog, useDialog } from "@/hooks/useDialog";
import type { IgnoreKind, IgnoreRule, MatchKind } from "@/types";

const IGNORE_KIND_LABEL: Record<IgnoreKind, string> = {
  app: "App",
  site: "Website",
};

const MATCH_KIND_LABEL: Record<MatchKind, string> = {
  exact: "is",
  prefix: "starts with",
  substring: "contains",
  regex: "matches",
};

const MATCH_KIND_SELECT_LABEL: Record<MatchKind, string> = {
  exact: "is",
  prefix: "starts with",
  substring: "contains",
  regex: "matches (regex)",
};

function matchKindPlaceholder(kind: IgnoreKind, matchKind: MatchKind): string {
  if (matchKind === "regex") {
    return kind === "app" ? "^game\\d*\\.exe$" : "^.*\\.corp\\.example\\.com$";
  }
  return kind === "app" ? "steam.exe" : "youtube.com";
}

export function IgnoreListSection() {
  const queryClient = useQueryClient();
  const [kind, setKind] = useState<IgnoreKind>("app");
  const [matchKind, setMatchKind] = useState<MatchKind>("substring");
  const [pattern, setPattern] = useState("");
  const addIgnoreRuleDialog = useDialog("add-ignore-rule");

  const rulesQuery = useQuery({
    queryKey: ["ignoreRules"],
    queryFn: listIgnoreRules,
  });
  const rules = rulesQuery.data ?? [];

  function invalidate() {
    queryClient.invalidateQueries({ queryKey: ["ignoreRules"] });
  }

  const createMutation = useMutation({
    mutationFn: () => {
      const normalized =
        matchKind === "regex" ? pattern.trim() : pattern.trim().toLowerCase();
      return createIgnoreRule(kind, normalized, matchKind);
    },
    onSuccess: () => {
      setPattern("");
      addIgnoreRuleDialog.setOpen(false);
      invalidate();
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (rule: IgnoreRule) => deleteIgnoreRule(rule.id),
    onSuccess: invalidate,
  });

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!pattern.trim()) return;
    createMutation.mutate();
  }

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Ignored apps & websites</CardTitle>
        <CardDescription>
          Matching apps and sites are excluded from tracking entirely — no time
          is recorded for them, using the same is/contains/matches rules as
          categorization.
        </CardDescription>
        <CardAction>
          <Button size="sm" onClick={() => openDialog("add-ignore-rule")}>
            Add
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        {!rulesQuery.isLoading && rules.length === 0 && (
          <p className="text-sm text-muted-foreground">
            Nothing ignored — every app and site is tracked.
          </p>
        )}

        <div className="flex flex-wrap items-center gap-1.5">
          {rules.map((rule) => (
            <Badge key={rule.id} variant="secondary" className="h-auto py-0.5">
              <span className="text-muted-foreground">
                {IGNORE_KIND_LABEL[rule.kind]}{" "}
                {MATCH_KIND_LABEL[rule.match_kind]}
              </span>
              <span className="font-mono">{rule.pattern}</span>
              <button
                type="button"
                onClick={() => deleteMutation.mutate(rule)}
                aria-label={`Remove ignore rule ${rule.pattern}`}
                className="text-muted-foreground hover:text-destructive"
              >
                <Trash2Icon className="size-3" />
              </button>
            </Badge>
          ))}
        </div>
      </CardContent>

      <Dialog
        open={addIgnoreRuleDialog.open}
        onOpenChange={addIgnoreRuleDialog.setOpen}
      >
        <DialogContent>
          <form onSubmit={handleSubmit} className="flex flex-col gap-4">
            <DialogHeader>
              <DialogTitle>Ignore app or website</DialogTitle>
            </DialogHeader>
            <div className="flex flex-wrap items-center gap-1.5">
              <Select
                value={kind}
                onValueChange={(value) => setKind(value as IgnoreKind)}
                items={IGNORE_KIND_LABEL}
              >
                <SelectTrigger size="sm" className="w-28">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="app">App</SelectItem>
                  <SelectItem value="site">Website</SelectItem>
                </SelectContent>
              </Select>
              <Select
                value={matchKind}
                onValueChange={(value) => setMatchKind(value as MatchKind)}
                items={MATCH_KIND_SELECT_LABEL}
              >
                <SelectTrigger
                  size="sm"
                  className="w-32"
                  aria-label="Match kind"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="exact">is</SelectItem>
                  <SelectItem value="substring">contains</SelectItem>
                  <SelectItem value="regex">matches (regex)</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <Input
              type="text"
              value={pattern}
              onChange={(e) => setPattern(e.target.value)}
              placeholder={matchKindPlaceholder(kind, matchKind)}
              className="font-mono"
              autoFocus
            />
            {createMutation.isError && (
              <p className="text-xs text-destructive">
                {String(createMutation.error)}
              </p>
            )}
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => addIgnoreRuleDialog.setOpen(false)}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                disabled={!pattern.trim() || createMutation.isPending}
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

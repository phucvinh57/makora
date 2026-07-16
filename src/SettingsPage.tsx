import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Moon, Sun, Trash2Icon } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  createIgnoreRule,
  deleteIgnoreRule,
  isAutostartEnabled,
  listIgnoreRules,
  setAutostartEnabled,
} from "./api";
import { toggleTheme, useTheme } from "./theme";
import type { IgnoreKind, IgnoreRule, MatchKind } from "./types";

export default function SettingsPage() {
  return (
    <div className="flex h-full flex-col gap-6 overflow-y-auto subtle-scrollbar box-border">
      <header>
        <h1 className="m-0 text-lg font-semibold">Settings</h1>
        <p className="m-0 text-sm text-muted-foreground">
          App preferences and tracking exclusions
        </p>
      </header>
      <ThemeSection />
      <StartupSection />
      <IgnoreListSection />
    </div>
  );
}

function StartupSection() {
  const queryClient = useQueryClient();

  const autostartQuery = useQuery({
    queryKey: ["autostartEnabled"],
    queryFn: isAutostartEnabled,
  });

  const toggleMutation = useMutation({
    mutationFn: (enabled: boolean) => setAutostartEnabled(enabled),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["autostartEnabled"] });
    },
  });

  const enabled = autostartQuery.data ?? false;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Startup</CardTitle>
        <CardDescription>
          Launch Makora automatically when you sign in.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="flex w-fit items-center gap-2 text-sm">
          <Checkbox
            id="autostart-checkbox"
            checked={enabled}
            disabled={autostartQuery.isLoading || toggleMutation.isPending}
            onCheckedChange={(checked) =>
              toggleMutation.mutate(checked === true)
            }
          />
          <label htmlFor="autostart-checkbox">Run when system starts</label>
        </div>
        {toggleMutation.isError && (
          <p className="mt-2 text-xs text-destructive">
            {String(toggleMutation.error)}
          </p>
        )}
      </CardContent>
    </Card>
  );
}

function ThemeSection() {
  const theme = useTheme();

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Theme</CardTitle>
        <CardDescription>
          Switch between light and dark appearance.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex items-center gap-2">
        <Button
          variant={theme === "light" ? "default" : "outline"}
          size="sm"
          onClick={() => theme === "dark" && toggleTheme()}
        >
          <Sun className="size-4" />
          Light
        </Button>
        <Button
          variant={theme === "dark" ? "default" : "outline"}
          size="sm"
          onClick={() => theme === "light" && toggleTheme()}
        >
          <Moon className="size-4" />
          Dark
        </Button>
      </CardContent>
    </Card>
  );
}

const IGNORE_KIND_LABEL: Record<IgnoreKind, string> = {
  app: "App",
  site: "Website",
};

const MATCH_KIND_LABEL: Record<MatchKind, string> = {
  exact: "is",
  substring: "contains",
  regex: "matches",
};

function matchKindPlaceholder(kind: IgnoreKind, matchKind: MatchKind): string {
  if (matchKind === "regex") {
    return kind === "app" ? "^game\\d*\\.exe$" : "^.*\\.corp\\.example\\.com$";
  }
  return kind === "app" ? "steam.exe" : "youtube.com";
}

function IgnoreListSection() {
  const queryClient = useQueryClient();
  const [kind, setKind] = useState<IgnoreKind>("app");
  const [matchKind, setMatchKind] = useState<MatchKind>("substring");
  const [pattern, setPattern] = useState("");

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
      // Lowercasing canonicalizes exact/substring patterns (matching is
      // case-insensitive either way), but would corrupt case-sensitive
      // regex escapes like `\D`/`\S`/`\W`, so regex patterns are sent as-is —
      // same convention as `CategoriesPage`'s rule form.
      const normalized =
        matchKind === "regex" ? pattern.trim() : pattern.trim().toLowerCase();
      return createIgnoreRule(kind, normalized, matchKind);
    },
    onSuccess: () => {
      setPattern("");
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
      </CardHeader>
      <CardContent>
        <form
          onSubmit={handleSubmit}
          className="mb-3 flex flex-wrap items-center gap-1.5"
        >
          <Select
            value={kind}
            onValueChange={(value) => setKind(value as IgnoreKind)}
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
          >
            <SelectTrigger size="sm" className="w-32" aria-label="Match kind">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="exact">is</SelectItem>
              <SelectItem value="substring">contains</SelectItem>
              <SelectItem value="regex">matches (regex)</SelectItem>
            </SelectContent>
          </Select>
          <Input
            type="text"
            value={pattern}
            onChange={(e) => setPattern(e.target.value)}
            placeholder={matchKindPlaceholder(kind, matchKind)}
            className="h-7 min-w-40 flex-1 font-mono"
          />
          <Button
            type="submit"
            size="sm"
            variant="outline"
            disabled={!pattern.trim() || createMutation.isPending}
          >
            Add
          </Button>
        </form>

        {createMutation.isError && (
          <p className="mb-3 text-xs text-destructive">
            {String(createMutation.error)}
          </p>
        )}

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
    </Card>
  );
}

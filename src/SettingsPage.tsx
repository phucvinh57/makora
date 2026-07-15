import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Moon, Sun, Trash2Icon } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { createIgnoreRule, deleteIgnoreRule, listIgnoreRules } from "./api";
import { useTheme } from "./theme";
import type { IgnoreKind, IgnoreRule, MatchKind } from "./types";

export default function SettingsPage() {
  return (
    <div className="flex h-full flex-col gap-6 overflow-y-auto no-scrollbar box-border">
      <header>
        <p className="m-0 text-sm text-muted-foreground">
          App preferences and tracking exclusions
        </p>
      </header>
      <ThemeSection />
      <IgnoreListSection />
    </div>
  );
}

function ThemeSection() {
  const { theme, toggleTheme } = useTheme();

  return (
    <section className="rounded-md border p-4">
      <h2 className="m-0 mb-1 text-sm font-semibold">Theme</h2>
      <p className="m-0 mb-3 text-xs text-muted-foreground">
        Switch between light and dark appearance.
      </p>
      <div className="flex items-center gap-2">
        <Button
          variant={theme === "light" ? "secondary" : "outline"}
          size="sm"
          onClick={() => theme === "dark" && toggleTheme()}
        >
          <Sun className="size-4" />
          Light
        </Button>
        <Button
          variant={theme === "dark" ? "secondary" : "outline"}
          size="sm"
          onClick={() => theme === "light" && toggleTheme()}
        >
          <Moon className="size-4" />
          Dark
        </Button>
      </div>
    </section>
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
    <section className="rounded-md border p-4">
      <h2 className="m-0 mb-1 text-sm font-semibold">
        Ignored apps & websites
      </h2>
      <p className="m-0 mb-3 text-xs text-muted-foreground">
        Matching apps and sites are excluded from tracking entirely — no time is
        recorded for them, using the same is/contains/matches rules as
        categorization.
      </p>

      <form onSubmit={handleSubmit} className="mb-3 flex items-center gap-1.5">
        <select
          value={kind}
          onChange={(e) => setKind(e.target.value as IgnoreKind)}
          className="h-7 rounded-md border bg-transparent px-1.5 text-sm"
        >
          <option value="app">App</option>
          <option value="site">Website</option>
        </select>
        <select
          value={matchKind}
          onChange={(e) => setMatchKind(e.target.value as MatchKind)}
          className="h-7 rounded-md border bg-transparent px-1.5 text-sm"
          aria-label="Match kind"
        >
          <option value="exact">is</option>
          <option value="substring">contains</option>
          <option value="regex">matches (regex)</option>
        </select>
        <input
          type="text"
          value={pattern}
          onChange={(e) => setPattern(e.target.value)}
          placeholder={matchKindPlaceholder(kind, matchKind)}
          className="h-7 rounded-md border bg-transparent px-2 text-sm font-mono"
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
          <span
            key={rule.id}
            className={cn(
              "inline-flex items-center gap-1 rounded-md border bg-muted px-1.5 py-0.5 text-xs",
            )}
          >
            <span className="text-muted-foreground">
              {IGNORE_KIND_LABEL[rule.kind]} {MATCH_KIND_LABEL[rule.match_kind]}
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
          </span>
        ))}
      </div>
    </section>
  );
}

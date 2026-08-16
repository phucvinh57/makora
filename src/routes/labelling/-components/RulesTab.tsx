import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Trash2Icon } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
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
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  categoryOptions,
  categoryValueLabels,
} from "@/components/CategoryOptions";
import {
  createCategoryRule,
  deleteCategoryRule,
  listCategories,
  listCategoryRules,
} from "@/api";
import { categoryPath } from "@/categories";
import { openDialog, useDialog } from "@/hooks/useDialog";
import type { CategoryRule, KeyKind, MatchKind } from "@/types";

export const ALL_CATEGORIES = "all";

const KEY_KIND_LABEL: Record<KeyKind, string> = {
  exe: "App",
  domain: "Site",
  url: "URL",
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

function stripSchemeAndWww(pattern: string): string {
  let url: URL;
  try {
    url = new URL(pattern.includes("://") ? pattern : `http://${pattern}`);
  } catch {
    return pattern;
  }
  const host = url.hostname.startsWith("www.")
    ? url.hostname.slice(4)
    : url.hostname;
  const rest = url.pathname === "/" ? "" : url.pathname;
  return `${host}${rest}${url.search}`;
}

function matchKindPlaceholder(keyKind: KeyKind, matchKind: MatchKind): string {
  if (matchKind === "regex") {
    if (keyKind === "exe") return "^game\\d*\\.exe$";
    if (keyKind === "domain") return "^.*\\.corp\\.example\\.com$";
    return "^docs\\.google\\.com/document/d/.+$";
  }
  if (keyKind === "url") {
    return matchKind === "exact"
      ? "youtube.com/watch?v=XYZ"
      : "docs.google.com/document/d/ABC";
  }
  if (matchKind === "substring") {
    return keyKind === "exe" ? "steam" : "google.com";
  }
  return keyKind === "exe" ? "code.exe" : "github.com";
}

export function RulesTab({
  categoryFilter,
  onCategoryFilterChange,
}: {
  categoryFilter: number | typeof ALL_CATEGORIES;
  onCategoryFilterChange: (value: number | typeof ALL_CATEGORIES) => void;
}) {
  const queryClient = useQueryClient();
  const [keyKind, setKeyKind] = useState<KeyKind>("exe");
  const [matchKind, setMatchKind] = useState<MatchKind>("exact");
  const [pattern, setPattern] = useState("");
  const [newRuleCategoryId, setNewRuleCategoryId] = useState<number | null>(
    null,
  );
  const [textFilter, setTextFilter] = useState("");
  const addRuleDialog = useDialog("add-rule");

  const categoriesQuery = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });
  const rulesQuery = useQuery({
    queryKey: ["categoryRules"],
    queryFn: listCategoryRules,
  });
  const categories = categoriesQuery.data ?? [];
  const rules = rulesQuery.data ?? [];
  const categoryById = new Map(categories.map((c) => [c.id, c]));
  const selectedNewRuleCategoryId = newRuleCategoryId ?? categories[0]?.id;
  const categorySelectItems = categoryValueLabels(categories);

  function invalidateAll() {
    queryClient.invalidateQueries({ queryKey: ["categories"] });
    queryClient.invalidateQueries({ queryKey: ["categoryRules"] });
  }

  const addRuleMutation = useMutation({
    mutationFn: ({
      categoryId,
      keyKind,
      pattern,
      matchKind,
    }: {
      categoryId: number;
      keyKind: KeyKind;
      pattern: string;
      matchKind: MatchKind;
    }) => {
      const stripped =
        matchKind !== "regex" && (keyKind === "domain" || keyKind === "url")
          ? stripSchemeAndWww(pattern.trim())
          : pattern.trim();
      const normalized =
        matchKind === "regex" || keyKind === "url"
          ? stripped
          : stripped.toLowerCase();
      return createCategoryRule(keyKind, normalized, matchKind, categoryId);
    },
    onSuccess: () => {
      setPattern("");
      addRuleDialog.setOpen(false);
      invalidateAll();
    },
  });

  const deleteRuleMutation = useMutation({
    mutationFn: (rule: CategoryRule) => deleteCategoryRule(rule.id),
    onSuccess: invalidateAll,
  });

  function handleAddRule(e: React.FormEvent) {
    e.preventDefault();
    if (!pattern.trim() || !selectedNewRuleCategoryId) return;
    addRuleMutation.mutate({
      categoryId: selectedNewRuleCategoryId,
      keyKind,
      pattern,
      matchKind,
    });
  }

  const filteredRules = rules.filter((rule) => {
    if (
      categoryFilter !== ALL_CATEGORIES &&
      rule.category_id !== categoryFilter
    ) {
      return false;
    }
    const term = textFilter.trim().toLowerCase();
    if (term && !rule.pattern.toLowerCase().includes(term)) return false;
    return true;
  });

  return (
    <>
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <Input
          type="text"
          value={textFilter}
          onChange={(e) => setTextFilter(e.target.value)}
          placeholder="Search..."
          className="h-7 w-48"
        />
        <Select
          value={
            categoryFilter === ALL_CATEGORIES
              ? ALL_CATEGORIES
              : String(categoryFilter)
          }
          onValueChange={(value) =>
            onCategoryFilterChange(
              !value || value === ALL_CATEGORIES
                ? ALL_CATEGORIES
                : Number(value),
            )
          }
          items={{
            [ALL_CATEGORIES]: "All categories",
            ...categorySelectItems,
          }}
        >
          <SelectTrigger
            size="sm"
            className="w-40"
            aria-label="Filter by category"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={ALL_CATEGORIES}>All categories</SelectItem>
            {categoryOptions(categories)}
          </SelectContent>
        </Select>
        <Button
          size="sm"
          className="ml-auto"
          onClick={() => openDialog("add-rule")}
        >
          Add rule
        </Button>
      </div>

      <Card className="flex-1 overflow-hidden p-0">
        <div className="h-full overflow-y-auto subtle-scrollbar">
          <Table className="table-fixed w-full">
            <TableHeader className="sticky top-0 z-10 bg-card">
              <TableRow>
                <TableHead className="w-24">Target</TableHead>
                <TableHead className="w-32">Match</TableHead>
                <TableHead>Pattern</TableHead>
                <TableHead className="w-40">Category</TableHead>
                <TableHead className="w-10" />
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredRules.map((rule) => {
                const category = categoryById.get(rule.category_id);
                return (
                  <TableRow key={rule.id}>
                    <TableCell>
                      <Badge variant="secondary">
                        {KEY_KIND_LABEL[rule.key_kind]}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {MATCH_KIND_LABEL[rule.match_kind]}
                    </TableCell>
                    <TableCell className="truncate font-mono">
                      {rule.pattern}
                    </TableCell>
                    <TableCell className="truncate">
                      {category && (
                        <span
                          className="flex items-center gap-1.5"
                          title={categoryPath(category, categories)}
                        >
                          <span
                            className="size-2 shrink-0 rounded-full"
                            style={{ backgroundColor: category.color }}
                          />
                          <span className="truncate">
                            {categoryPath(category, categories)}
                          </span>
                        </span>
                      )}
                    </TableCell>
                    <TableCell>
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label={`Remove rule ${rule.pattern}`}
                        onClick={() => deleteRuleMutation.mutate(rule)}
                      >
                        <Trash2Icon size={14} />
                      </Button>
                    </TableCell>
                  </TableRow>
                );
              })}
              {filteredRules.length === 0 && (
                <TableRow>
                  <TableCell
                    colSpan={5}
                    className="py-8 text-center text-muted-foreground"
                  >
                    No rules match this filter.
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </div>
      </Card>

      <p className="mt-3 text-xs text-muted-foreground">
        When several rules match the same activity, the most specific one wins:
        a URL rule beats a site/app rule, then "is" beats "starts with" beats
        "contains" beats "matches", then the longer pattern wins.
      </p>

      <Dialog open={addRuleDialog.open} onOpenChange={addRuleDialog.setOpen}>
        <DialogContent>
          <form onSubmit={handleAddRule} className="flex flex-col gap-4">
            <DialogHeader>
              <DialogTitle>Add rule</DialogTitle>
            </DialogHeader>
            <div className="flex flex-wrap items-center gap-1.5">
              <Select
                value={keyKind}
                onValueChange={(value) => setKeyKind(value as KeyKind)}
                items={KEY_KIND_LABEL}
              >
                <SelectTrigger size="sm" className="w-28">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="exe">App</SelectItem>
                  <SelectItem value="domain">Site</SelectItem>
                  <SelectItem value="url">URL</SelectItem>
                </SelectContent>
              </Select>
              <Select
                value={matchKind}
                onValueChange={(value) => setMatchKind(value as MatchKind)}
                items={MATCH_KIND_SELECT_LABEL}
              >
                <SelectTrigger
                  size="sm"
                  className="w-36"
                  aria-label="Match kind"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="exact">is</SelectItem>
                  <SelectItem value="prefix">starts with</SelectItem>
                  <SelectItem value="substring">contains</SelectItem>
                  <SelectItem value="regex">matches (regex)</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <Input
              type="text"
              value={pattern}
              onChange={(e) => setPattern(e.target.value)}
              placeholder={matchKindPlaceholder(keyKind, matchKind)}
              className="font-mono"
              autoFocus
            />
            <Select
              value={
                selectedNewRuleCategoryId != null
                  ? String(selectedNewRuleCategoryId)
                  : ""
              }
              onValueChange={(value) =>
                value && setNewRuleCategoryId(Number(value))
              }
              items={categorySelectItems}
            >
              <SelectTrigger className="w-full" aria-label="Category">
                <SelectValue placeholder="Category…" />
              </SelectTrigger>
              <SelectContent>{categoryOptions(categories)}</SelectContent>
            </Select>
            {addRuleMutation.isError && (
              <p className="text-xs text-destructive">
                {String(addRuleMutation.error)}
              </p>
            )}
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => addRuleDialog.setOpen(false)}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                disabled={
                  !pattern.trim() ||
                  !selectedNewRuleCategoryId ||
                  addRuleMutation.isPending
                }
              >
                Add rule
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </>
  );
}

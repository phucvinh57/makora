import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, X } from "lucide-react";
import { useMemo, useState } from "react";
import { settings } from "@/api";
import { type Suggestion, SuggestInput } from "@/components/SuggestInput";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { formatSeconds } from "@/duration";
import {
  queryKeys,
  settingsQueryOptions,
  sourceSuggestionsQueryOptions,
} from "@/queries";
import {
  describeRule,
  PATTERN_FIELD_LABELS,
  PATTERN_FIELDS,
  parsePatternField,
  parseRuleKind,
  RULE_KIND_LABELS,
  RULE_KINDS,
  RULE_VALUE_PLACEHOLDERS,
} from "@/rules";
import type { PatternField, RuleKind } from "@/types";

const HINTS: Record<RuleKind, string> = {
  app: "Nothing done in this app is recorded, including its browser tabs if it is a browser.",
  site: "Every page of this website is skipped.",
  page: "This page and everything below it is skipped.",
  pattern:
    "* matches any text and ? matches one character, ignoring case. Tracked as the title or address when it starts.",
};

export function ExclusionsSection() {
  const queryClient = useQueryClient();
  const settingsQuery = useQuery(settingsQueryOptions);
  const sourcesQuery = useQuery(sourceSuggestionsQueryOptions);
  const excluded = settingsQuery.data?.tracking.excluded ?? [];

  const [kind, setKind] = useState<RuleKind>("app");
  const [field, setField] = useState<PatternField>("url");
  const [value, setValue] = useState("");

  const suggestions = useMemo<Suggestion[]>(() => {
    const wanted = kind === "app" ? "exe" : "domain";
    return (sourcesQuery.data ?? [])
      .filter((source) => source.kind === wanted)
      .map((source) => ({
        value: source.key,
        hint: formatSeconds(source.total_seconds),
        search: source.name,
      }));
  }, [sourcesQuery.data, kind]);

  const onSaved = (saved: Awaited<ReturnType<typeof settings.get>>) =>
    queryClient.setQueryData(queryKeys.settings, saved);

  const addMutation = useMutation({
    mutationFn: () =>
      settings.addExclusion(kind, kind === "pattern" ? field : null, value),
    onSuccess: (saved) => {
      onSaved(saved);
      setValue("");
    },
  });
  const removeMutation = useMutation({
    mutationFn: settings.removeExclusion,
    onSuccess: onSaved,
  });

  const canAdd = value.trim() !== "" && !addMutation.isPending;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Excluded from tracking</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <form
          className="flex flex-col gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            if (canAdd) addMutation.mutate();
          }}
        >
          <div className="flex items-center gap-2">
            <Select
              value={kind}
              onValueChange={(next) => {
                const parsed = parseRuleKind(next);
                if (!parsed) return;
                addMutation.reset();
                setKind(parsed);
              }}
              items={RULE_KIND_LABELS}
            >
              <SelectTrigger className="w-32" aria-label="What to exclude">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {RULE_KINDS.map((item) => (
                  <SelectItem key={item} value={item}>
                    {RULE_KIND_LABELS[item]}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {kind === "pattern" && (
              <Select
                value={field}
                onValueChange={(next) => {
                  const parsed = parsePatternField(next);
                  if (parsed) setField(parsed);
                }}
                items={PATTERN_FIELD_LABELS}
              >
                <SelectTrigger className="w-40" aria-label="Match against">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {PATTERN_FIELDS.map((item) => (
                    <SelectItem key={item} value={item}>
                      {PATTERN_FIELD_LABELS[item]}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
            <div className="min-w-0 flex-1">
              {kind === "app" || kind === "site" ? (
                <SuggestInput
                  value={value}
                  onValueChange={(next) => {
                    addMutation.reset();
                    setValue(next);
                  }}
                  suggestions={suggestions}
                  placeholder={RULE_VALUE_PLACEHOLDERS[kind]}
                  ariaLabel={kind === "app" ? "App" : "Website"}
                />
              ) : (
                <Input
                  type="text"
                  value={value}
                  onChange={(event) => {
                    addMutation.reset();
                    setValue(event.target.value);
                  }}
                  placeholder={RULE_VALUE_PLACEHOLDERS[kind]}
                  aria-label={kind === "page" ? "Page" : "Pattern"}
                  autoComplete="off"
                  spellCheck={false}
                />
              )}
            </div>
            <Button type="submit" size="sm" disabled={!canAdd}>
              <Plus />
              Exclude
            </Button>
          </div>
          <p className="m-0 text-xs text-muted-foreground">{HINTS[kind]}</p>
          {addMutation.isError && (
            <p className="m-0 text-xs text-destructive">
              {String(addMutation.error)}
            </p>
          )}
        </form>

        {excluded.length > 0 && (
          <ul className="m-0 flex list-none flex-col divide-y rounded-md border p-0">
            {excluded.map((exclusion) => (
              <li
                key={`${exclusion.kind}:${exclusion.field ?? ""}:${exclusion.value}`}
                className="flex items-center justify-between gap-2 px-3 py-1.5 text-sm"
              >
                <span className="truncate">
                  {describeRule(
                    exclusion.kind,
                    exclusion.field ?? null,
                    exclusion.value,
                  )}
                </span>
                <Button
                  type="button"
                  size="icon-xs"
                  variant="ghost"
                  aria-label={`Stop excluding ${exclusion.value}`}
                  title="Stop excluding"
                  disabled={removeMutation.isPending}
                  onClick={() => removeMutation.mutate(exclusion)}
                >
                  <X />
                </Button>
              </li>
            ))}
          </ul>
        )}
        <p className="m-0 text-xs text-muted-foreground">
          Matching activity is not recorded from the next time it starts.
          Already recorded history is kept.
        </p>
        {removeMutation.isError && (
          <p className="m-0 text-xs text-destructive">
            {String(removeMutation.error)}
          </p>
        )}
      </CardContent>
    </Card>
  );
}

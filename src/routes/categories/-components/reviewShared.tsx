import { Check, X } from "lucide-react";
import type { CategoryPicker } from "@/components/CategoryOptions";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { CreatableCategoryCombobox } from "@/components/CreatableCategoryCombobox";
import type { AiReviewGroup } from "@/types";

export function groupId(group: AiReviewGroup): string {
  return `${group.key_kind}:${group.key}:${group.category_id}`;
}

export function sourceName(group: AiReviewGroup): string {
  return group.key_kind === "domain" ? group.key : group.app_name;
}

/** What accepting, rejecting or recategorizing activities does, and whether one is in flight. */
export type Decide = {
  busy: boolean;
  accept: (ids: number[]) => void;
  setCategory: (ids: number[], categoryId: number | null) => void;
};

export function CategorySelect({
  picker,
  value,
  disabled,
  onChange,
}: {
  picker: CategoryPicker;
  value: number;
  disabled: boolean;
  onChange: (categoryId: number) => void;
}) {
  return (
    <CreatableCategoryCombobox
      picker={picker}
      value={String(value)}
      disabled={disabled}
      onValueChange={(next) => {
        if (Number(next) !== value) onChange(Number(next));
      }}
      size="sm"
      className="h-6 w-full px-1.5 text-xs"
    />
  );
}

export function DecisionButtons({
  ids,
  decide,
  noun,
}: {
  ids: number[];
  decide: Decide;
  noun: string;
}) {
  return (
    <div className="flex justify-end gap-1">
      <Button
        size="icon-sm"
        variant="ghost"
        disabled={decide.busy}
        aria-label={`Accept ${noun}`}
        title={`Accept ${noun}`}
        onClick={() => decide.accept(ids)}
      >
        <Check />
      </Button>
      <Button
        size="icon-sm"
        variant="ghost"
        disabled={decide.busy}
        aria-label={`Reject ${noun}`}
        title={`Reject ${noun}: leave uncategorized`}
        onClick={() => decide.setCategory(ids, null)}
      >
        <X />
      </Button>
    </div>
  );
}

export function ConfidenceBadge({ value }: { value: number | null }) {
  if (value === null) {
    return <span className="text-xs text-muted-foreground">—</span>;
  }
  return (
    <Badge variant={value < 0.6 ? "destructive" : "outline"}>
      {Math.round(value * 100)}%
    </Badge>
  );
}

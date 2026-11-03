import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import { toast } from "sonner";
import { settings } from "@/api";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useDialog } from "@/hooks/useDialog";
import { queryKeys } from "@/queries";
import type { ModelPrice } from "@/types";

export interface ModelPriceDialogData {
  provider: string;
  providerName: string;
  model: string;
  price: ModelPrice | undefined;
}

type Field = "input" | "cached_input" | "output";

const FIELDS: { field: Field; label: string; optional?: boolean }[] = [
  { field: "input", label: "Input" },
  { field: "cached_input", label: "Cached input", optional: true },
  { field: "output", label: "Output" },
];

function toText(value: number | null | undefined): string {
  return value === null || value === undefined ? "" : String(value);
}

/** A price as typed: `null` when the field is empty, `NaN` when it is not a price. */
function parsePrice(text: string): number | null {
  const trimmed = text.trim();
  if (trimmed === "") return null;
  const value = Number(trimmed);
  return Number.isFinite(value) && value >= 0 ? value : Number.NaN;
}

function PriceForm({
  data,
  saving,
  onSave,
  onRemove,
  onCancel,
}: {
  data: ModelPriceDialogData;
  saving: boolean;
  onSave: (price: ModelPrice) => void;
  onRemove: () => void;
  onCancel: () => void;
}) {
  const id = useId();
  const [text, setText] = useState<Record<Field, string>>({
    input: toText(data.price?.input),
    cached_input: toText(data.price?.cached_input),
    output: toText(data.price?.output),
  });
  const parsed = {
    input: parsePrice(text.input),
    cached_input: parsePrice(text.cached_input),
    output: parsePrice(text.output),
  };
  const invalid = (field: Field) => Number.isNaN(parsed[field]);
  const canSave =
    parsed.input !== null &&
    parsed.output !== null &&
    !FIELDS.some(({ field }) => invalid(field));

  function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!canSave || parsed.input === null || parsed.output === null) return;
    onSave({
      provider: data.provider,
      model: data.model,
      input: parsed.input,
      cached_input: parsed.cached_input,
      output: parsed.output,
    });
  }

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>Price of {data.model}</DialogTitle>
        <DialogDescription>
          What {data.providerName} charges, in US dollars per million tokens. It
          is only used to estimate what categorization costs.
        </DialogDescription>
      </DialogHeader>
      <div className="grid grid-cols-3 gap-3">
        {FIELDS.map(({ field, label, optional }) => (
          <div key={field} className="flex flex-col gap-1 text-xs">
            <label htmlFor={`${id}-${field}`} className="text-muted-foreground">
              {label}
            </label>
            <Input
              id={`${id}-${field}`}
              inputMode="decimal"
              value={text[field]}
              placeholder={optional ? "Same as input" : "0.00"}
              aria-invalid={invalid(field) || undefined}
              autoFocus={field === "input"}
              onChange={(event) =>
                setText((current) => ({
                  ...current,
                  [field]: event.target.value,
                }))
              }
            />
          </div>
        ))}
      </div>
      <p className="m-0 text-xs text-muted-foreground">
        Cached input is a prompt the provider read from its cache, usually
        billed at a discount.
      </p>
      <DialogFooter>
        {data.price && (
          <Button
            type="button"
            variant="ghost"
            className="mr-auto text-destructive"
            disabled={saving}
            onClick={onRemove}
          >
            Remove price
          </Button>
        )}
        <Button type="button" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="submit" disabled={!canSave || saving}>
          Save
        </Button>
      </DialogFooter>
    </form>
  );
}

export function ModelPriceDialog() {
  const queryClient = useQueryClient();
  const { open, setOpen, data } = useDialog("model-price");

  const saveMutation = useMutation({
    mutationFn: (price: ModelPrice | { provider: string; model: string }) =>
      "input" in price
        ? settings.setModelPrice(price)
        : settings.removeModelPrice(price.provider, price.model),
    onSuccess: (saved) => {
      queryClient.setQueryData(queryKeys.settings, saved);
      setOpen(false);
    },
    onError: (err) => toast.error(String(err)),
  });

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        {data && (
          <PriceForm
            key={`${data.provider}/${data.model}`}
            data={data}
            saving={saveMutation.isPending}
            onSave={(price) => saveMutation.mutate(price)}
            onRemove={() =>
              saveMutation.mutate({
                provider: data.provider,
                model: data.model,
              })
            }
            onCancel={() => setOpen(false)}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

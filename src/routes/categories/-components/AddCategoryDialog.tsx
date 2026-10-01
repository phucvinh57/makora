import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { createCategory } from "@/api";
import type { CategoryPicker } from "@/components/CategoryOptions";
import { Button } from "@/components/ui/button";
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
import { invalidateCategorizationQueries } from "@/queries";
import type { Category } from "@/types";
import { NO_PARENT } from "./CategoryCard";

const DEFAULT_COLOR = "#6366f1";

/**
 * The form is its own component because the dialog unmounts its content on close: the fields
 * and the mutation's error start fresh every time it opens, with nothing to reset by hand.
 */
function AddCategoryForm({
  parent,
  picker,
  onClose,
}: {
  parent: Category | null;
  picker: CategoryPicker;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const [color, setColor] = useState(parent?.color ?? DEFAULT_COLOR);
  const [description, setDescription] = useState("");
  const [parentId, setParentId] = useState<number | null>(parent?.id ?? null);

  const createMutation = useMutation({
    mutationFn: createCategory,
    onSuccess: () => {
      onClose();
      return invalidateCategorizationQueries(queryClient);
    },
  });

  const trimmedName = name.trim();
  const chosenParent =
    parentId !== null
      ? picker.categories.find((c) => c.id === parentId)
      : undefined;

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        if (!trimmedName) return;
        createMutation.mutate({
          name: trimmedName,
          color,
          description: description.trim() || null,
          parentId,
        });
      }}
      className="flex flex-col gap-4"
    >
      <DialogHeader>
        <DialogTitle>
          {chosenParent
            ? `Add subcategory of ${picker.pathById.get(chosenParent.id)}`
            : "Add category"}
        </DialogTitle>
      </DialogHeader>
      <div className="flex items-center gap-2">
        <input
          type="color"
          value={color}
          onChange={(e) => setColor(e.target.value)}
          className="h-8 w-10 shrink-0 cursor-pointer rounded-md border bg-transparent p-0.5"
          aria-label="New category color"
        />
        <Input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="New category name"
          className="flex-1"
          autoFocus
        />
      </div>
      <Input
        type="text"
        value={description}
        onChange={(e) => setDescription(e.target.value)}
        placeholder="Description (optional)"
        aria-describedby="category-description-help"
      />
      <p
        id="category-description-help"
        className="text-xs text-muted-foreground"
      >
        Describe what belongs here.
      </p>
      <Select
        value={parentId === null ? NO_PARENT : String(parentId)}
        onValueChange={(value) =>
          setParentId(!value || value === NO_PARENT ? null : Number(value))
        }
        items={{
          [NO_PARENT]: "No parent (top level)",
          ...picker.valueLabels,
        }}
      >
        <SelectTrigger className="w-full" aria-label="Parent category">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={NO_PARENT}>No parent (top level)</SelectItem>
          {picker.options}
        </SelectContent>
      </Select>
      {createMutation.isError && (
        <p className="text-xs text-destructive">
          {String(createMutation.error)}
        </p>
      )}
      <DialogFooter>
        <Button type="button" variant="outline" onClick={onClose}>
          Cancel
        </Button>
        <Button
          type="submit"
          disabled={!trimmedName || createMutation.isPending}
        >
          Add category
        </Button>
      </DialogFooter>
    </form>
  );
}

export function AddCategoryDialog({
  open,
  onOpenChange,
  parent,
  picker,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The category the new one goes under; `null` for a top-level one. */
  parent: Category | null;
  picker: CategoryPicker;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <AddCategoryForm
          parent={parent}
          picker={picker}
          onClose={() => onOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  );
}

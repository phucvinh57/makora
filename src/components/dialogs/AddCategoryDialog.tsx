import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { category } from "@/api";
import { CategoryCombobox } from "@/components/CategoryCombobox";
import {
  type CategoryChoice,
  useCategoryPicker,
} from "@/components/CategoryOptions";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useCategories } from "@/hooks/useCategories";
import { useDialog } from "@/hooks/useDialog";
import { invalidateCategorizationQueries } from "@/queries";
import type { Category } from "@/types";

const DEFAULT_COLOR = "#6366f1";
const NO_PARENT = "none";
const NO_PARENT_CHOICES: CategoryChoice[] = [
  { value: NO_PARENT, label: "No parent (top level)" },
];

/**
 * The form is its own component because the dialog unmounts its content on close: the fields
 * and the mutation's error start fresh every time it opens, with nothing to reset by hand.
 */
function AddCategoryForm({
  parent,
  onCreated,
  onClose,
}: {
  parent: Category | null;
  onCreated?: (category: Category) => void;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const { categories } = useCategories();
  const picker = useCategoryPicker(categories);
  const [name, setName] = useState("");
  const [color, setColor] = useState(parent?.color ?? DEFAULT_COLOR);
  const [description, setDescription] = useState("");
  const [parentId, setParentId] = useState<number | null>(parent?.id ?? null);

  const createMutation = useMutation({
    mutationFn: category.create,
    onSuccess: async (created) => {
      // The list has to know the new category before anyone can select it.
      await invalidateCategorizationQueries(queryClient);
      onCreated?.(created);
      onClose();
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
      <CategoryCombobox
        picker={picker}
        extraChoices={NO_PARENT_CHOICES}
        value={parentId === null ? NO_PARENT : String(parentId)}
        onValueChange={(value) =>
          setParentId(value === NO_PARENT ? null : Number(value))
        }
        ariaLabel="Parent category"
        className="w-full"
      />
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

export interface AddCategoryDialogData {
  /** The category the new one goes under; `null` for a top-level one. */
  parent: Category | null;
  /** Called once the new category is in the list, so the opener can select it. */
  onCreated?: (category: Category) => void;
}

export function AddCategoryDialog() {
  const { open, setOpen, data } =
    useDialog<AddCategoryDialogData>("add-category");

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        <AddCategoryForm
          parent={data?.parent ?? null}
          onCreated={data?.onCreated}
          onClose={() => setOpen(false)}
        />
      </DialogContent>
    </Dialog>
  );
}

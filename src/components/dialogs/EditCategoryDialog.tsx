import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { category as categoryApi, type CategoryDraft } from "@/api";
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
import { Textarea } from "@/components/ui/textarea";
import { categorySubtreeIds } from "@/categories";
import { useCategories } from "@/hooks/useCategories";
import { useDialog } from "@/hooks/useDialog";
import { invalidateCategorizationQueries } from "@/queries";
import type { Category } from "@/types";

const NO_PARENT = "none";
const NO_PARENT_CHOICES: CategoryChoice[] = [
  { value: NO_PARENT, label: "No parent (top level)" },
];

/**
 * The form is its own component because the dialog unmounts its content on close: the fields
 * and the mutation's error start from the category every time it opens.
 */
function EditCategoryForm({
  category,
  onClose,
}: {
  category: Category;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const { categories } = useCategories();
  const [name, setName] = useState(category.name);
  const [color, setColor] = useState(category.color);
  const [description, setDescription] = useState(category.description ?? "");
  const [parentId, setParentId] = useState<number | null>(category.parent_id);

  // Moving a category inside its own branch would cut that branch loose from every root,
  // so its own subtree is not on offer as a new parent.
  const parentChoices = useMemo(() => {
    const ownSubtree = categorySubtreeIds(category.id, categories);
    return categories.filter((c) => !ownSubtree.has(c.id));
  }, [category.id, categories]);
  const picker = useCategoryPicker(parentChoices);

  const saveMutation = useMutation({
    mutationFn: (draft: CategoryDraft) =>
      categoryApi.update(category.id, draft),
    onSuccess: async () => {
      await invalidateCategorizationQueries(queryClient);
      onClose();
    },
  });

  const trimmedName = name.trim();

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        if (!trimmedName) return;
        saveMutation.mutate({
          name: trimmedName,
          color,
          description: description.trim() || null,
          parentId,
        });
      }}
      className="flex flex-col gap-4"
    >
      <DialogHeader>
        <DialogTitle>Edit category</DialogTitle>
      </DialogHeader>
      <div className="flex items-center gap-2">
        <input
          type="color"
          value={color}
          onChange={(e) => setColor(e.target.value)}
          className="h-8 w-10 shrink-0 cursor-pointer rounded-md border bg-transparent p-0.5"
          aria-label="Category color"
        />
        <Input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Category name"
          aria-label="Category name"
          className="flex-1"
          autoFocus
        />
      </div>
      <Textarea
        value={description}
        onChange={(e) => setDescription(e.target.value)}
        placeholder="Description (optional)"
        aria-label="Category description"
        aria-describedby="edit-category-description-help"
      />
      <p
        id="edit-category-description-help"
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
      {saveMutation.isError && (
        <p className="text-xs text-destructive">{String(saveMutation.error)}</p>
      )}
      <DialogFooter>
        <Button type="button" variant="outline" onClick={onClose}>
          Cancel
        </Button>
        <Button type="submit" disabled={!trimmedName || saveMutation.isPending}>
          Save
        </Button>
      </DialogFooter>
    </form>
  );
}

export interface EditCategoryDialogData {
  category: Category;
}

export function EditCategoryDialog() {
  const { open, setOpen, data } =
    useDialog<EditCategoryDialogData>("edit-category");

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        {data && (
          <EditCategoryForm
            key={data.category.id}
            category={data.category}
            onClose={() => setOpen(false)}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

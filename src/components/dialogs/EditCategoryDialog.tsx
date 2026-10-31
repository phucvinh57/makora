import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { category as categoryApi, type CategoryDraft } from "@/api";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { categorySubtreeIds } from "@/categories";
import { useCategories } from "@/hooks/useCategories";
import { useDialog } from "@/hooks/useDialog";
import { invalidateCategorizationQueries } from "@/queries";
import type { Category } from "@/types";
import { CategoryForm } from "./CategoryForm";

function EditCategoryForm({
  category,
  onClose,
}: {
  category: Category;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const { categories } = useCategories();

  // Moving a category inside its own branch would cut that branch loose from every root,
  // so its own subtree is not on offer as a new parent.
  const parentChoices = useMemo(() => {
    const ownSubtree = categorySubtreeIds(category.id, categories);
    return categories.filter((c) => !ownSubtree.has(c.id));
  }, [category.id, categories]);

  const saveMutation = useMutation({
    mutationFn: (draft: CategoryDraft) =>
      categoryApi.update(category.id, draft),
    onSuccess: async () => {
      await invalidateCategorizationQueries(queryClient);
      onClose();
    },
  });

  return (
    <CategoryForm
      initial={{
        name: category.name,
        color: category.color,
        description: category.description,
        parentId: category.parent_id,
      }}
      parentChoices={parentChoices}
      title={() => "Edit category"}
      submitLabel="Save"
      pending={saveMutation.isPending}
      error={saveMutation.error}
      onSubmit={(draft) => saveMutation.mutate(draft)}
      onClose={onClose}
    />
  );
}

export interface EditCategoryDialogData {
  category: Category;
}

export function EditCategoryDialog() {
  const { open, setOpen, data } = useDialog("edit-category");

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

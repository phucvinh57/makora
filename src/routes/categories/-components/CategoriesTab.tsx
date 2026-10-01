import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { toast } from "sonner";
import { type CategoryDraft, deleteCategory, updateCategory } from "@/api";
import { useCategoryPicker } from "@/components/CategoryOptions";
import { Button } from "@/components/ui/button";
import { categorySubtreeIds, orderedCategories } from "@/categories";
import { useCategories } from "@/hooks/useCategories";
import { invalidateCategorizationQueries } from "@/queries";
import type { Category } from "@/types";
import { AddCategoryDialog } from "./AddCategoryDialog";
import { CategoryCard } from "./CategoryCard";
import { DeleteCategoryDialog } from "./DeleteCategoryDialog";

export function CategoriesTab() {
  const queryClient = useQueryClient();
  const { categories, isLoading, error } = useCategories();
  const picker = useCategoryPicker(categories);
  const tree = useMemo(() => orderedCategories(categories), [categories]);

  // `parent` outlives `open` so the dialog keeps its content while it fades out.
  const [add, setAdd] = useState<{ open: boolean; parent: Category | null }>({
    open: false,
    parent: null,
  });
  const [remove, setRemove] = useState<{
    open: boolean;
    category: Category | null;
  }>({ open: false, category: null });

  const saveCategoryMutation = useMutation({
    mutationFn: ({
      category,
      draft,
    }: {
      category: Category;
      draft: CategoryDraft;
    }) => updateCategory(category.id, draft),
    onSuccess: () => invalidateCategorizationQueries(queryClient),
    onError: (err) => toast.error(String(err)),
  });

  const deleteCategoryMutation = useMutation({
    mutationFn: (category: Category) => deleteCategory(category.id),
    onSuccess: () => invalidateCategorizationQueries(queryClient),
    onError: (err) => toast.error(String(err)),
  });

  return (
    <>
      {error && (
        <div className="mb-3 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {String(error)}
        </div>
      )}

      <div className="mb-4 flex justify-end">
        <Button size="sm" onClick={() => setAdd({ open: true, parent: null })}>
          Add category
        </Button>
      </div>

      {!isLoading && categories.length === 0 && (
        <div className="flex flex-col items-start gap-2 text-sm text-muted-foreground">
          <p>No categories yet. Use “Add category” above to begin.</p>
        </div>
      )}
      <div className="flex flex-col gap-3">
        {tree.map(({ category, depth, children }) => (
          <CategoryCard
            key={category.id}
            category={category}
            depth={depth}
            subcategoryCount={children.length}
            categories={categories}
            onSave={(cat, draft) =>
              saveCategoryMutation.mutate({ category: cat, draft })
            }
            onDelete={(cat) => setRemove({ open: true, category: cat })}
            onAddSubcategory={() => setAdd({ open: true, parent: category })}
          />
        ))}
      </div>

      <AddCategoryDialog
        open={add.open}
        onOpenChange={(open) => setAdd((prev) => ({ ...prev, open }))}
        parent={add.parent}
        picker={picker}
      />
      <DeleteCategoryDialog
        open={remove.open}
        onOpenChange={(open) => setRemove((prev) => ({ ...prev, open }))}
        category={remove.category}
        descendantCount={
          remove.category
            ? categorySubtreeIds(remove.category.id, categories).size - 1
            : 0
        }
        onConfirm={(category) => {
          deleteCategoryMutation.mutate(category);
          setRemove((prev) => ({ ...prev, open: false }));
        }}
      />
    </>
  );
}

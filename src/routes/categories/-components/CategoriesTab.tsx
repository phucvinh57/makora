import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { toast } from "sonner";
import { category as categoryApi, type CategoryDraft } from "@/api";
import type { AddCategoryDialogData } from "@/components/dialogs/AddCategoryDialog";
import type { DeleteCategoryDialogData } from "@/components/dialogs/DeleteCategoryDialog";
import { Button } from "@/components/ui/button";
import { orderedCategories } from "@/categories";
import { useCategories } from "@/hooks/useCategories";
import { openDialog } from "@/hooks/useDialog";
import { invalidateCategorizationQueries } from "@/queries";
import type { Category } from "@/types";
import { CategoryCard } from "./CategoryCard";

export function CategoriesTab() {
  const queryClient = useQueryClient();
  const { categories, isLoading, error } = useCategories();
  const tree = useMemo(() => orderedCategories(categories), [categories]);

  const saveCategoryMutation = useMutation({
    mutationFn: ({
      category,
      draft,
    }: {
      category: Category;
      draft: CategoryDraft;
    }) => categoryApi.update(category.id, draft),
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
        <Button
          size="sm"
          onClick={() =>
            openDialog<AddCategoryDialogData>("add-category", { parent: null })
          }
        >
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
            onDelete={(cat) =>
              openDialog<DeleteCategoryDialogData>("delete-category", {
                category: cat,
              })
            }
            onAddSubcategory={() =>
              openDialog<AddCategoryDialogData>("add-category", {
                parent: category,
              })
            }
          />
        ))}
      </div>
    </>
  );
}

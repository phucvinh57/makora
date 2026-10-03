import { useMemo } from "react";
import type { AddCategoryDialogData } from "@/components/dialogs/AddCategoryDialog";
import type { DeleteCategoryDialogData } from "@/components/dialogs/DeleteCategoryDialog";
import type { EditCategoryDialogData } from "@/components/dialogs/EditCategoryDialog";
import { Button } from "@/components/ui/button";
import { orderedCategories } from "@/categories";
import { useCategories } from "@/hooks/useCategories";
import { openDialog } from "@/hooks/useDialog";
import { CategoryCard } from "./CategoryCard";

export function CategoriesTab() {
  const { categories, isLoading, error } = useCategories();
  const tree = useMemo(() => orderedCategories(categories), [categories]);

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
          <p>
            No categories yet. Use “Add category” above to begin; the AI sorts
            your activities into them.
          </p>
        </div>
      )}
      <div className="flex flex-col gap-3">
        {tree.map(({ category, depth, children }) => (
          <CategoryCard
            key={category.id}
            category={category}
            depth={depth}
            subcategoryCount={children.length}
            onEdit={() =>
              openDialog<EditCategoryDialogData>("edit-category", { category })
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

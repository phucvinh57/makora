import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
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
  createCategory,
  deleteCategory,
  listCategories,
  listCategoryRules,
  updateCategory,
} from "@/api";
import { openDialog, useDialog } from "@/hooks/useDialog";
import type { Category } from "@/types";
import { CategoryCard } from "./CategoryCard";

const DEFAULT_COLOR = "#6366f1";

export function CategoriesTab({
  onShowRules,
}: {
  onShowRules: (categoryId: number) => void;
}) {
  const queryClient = useQueryClient();
  const [newName, setNewName] = useState("");
  const [newColor, setNewColor] = useState(DEFAULT_COLOR);
  const addCategoryDialog = useDialog("add-category");

  const categoriesQuery = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });
  const rulesQuery = useQuery({
    queryKey: ["categoryRules"],
    queryFn: listCategoryRules,
  });

  const categories = categoriesQuery.data ?? [];
  const rules = rulesQuery.data ?? [];
  const loading = categoriesQuery.isLoading || rulesQuery.isLoading;
  const error =
    (categoriesQuery.error && String(categoriesQuery.error)) ||
    (rulesQuery.error && String(rulesQuery.error)) ||
    null;

  function invalidateAll() {
    queryClient.invalidateQueries({ queryKey: ["categories"] });
    queryClient.invalidateQueries({ queryKey: ["categoryRules"] });
  }

  const createCategoryMutation = useMutation({
    mutationFn: () => createCategory(newName.trim(), newColor),
    onSuccess: () => {
      setNewName("");
      setNewColor(DEFAULT_COLOR);
      addCategoryDialog.setOpen(false);
      invalidateAll();
    },
  });

  const renameCategoryMutation = useMutation({
    mutationFn: ({
      category,
      name,
      color,
    }: {
      category: Category;
      name: string;
      color: string;
    }) => updateCategory(category.id, name, color),
    onSuccess: invalidateAll,
  });

  const deleteCategoryMutation = useMutation({
    mutationFn: (category: Category) => deleteCategory(category.id),
    onSuccess: invalidateAll,
  });

  function handleCreateCategory(e: React.FormEvent) {
    e.preventDefault();
    if (!newName.trim()) return;
    createCategoryMutation.mutate();
  }

  return (
    <>
      {error && (
        <div className="mb-3 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}

      <div className="mb-4 flex justify-end">
        <Button size="sm" onClick={() => openDialog("add-category")}>
          Add category
        </Button>
      </div>

      <div className="flex-1 overflow-y-auto subtle-scrollbar">
        {!loading && categories.length === 0 && (
          <p className="text-sm text-muted-foreground">No categories yet.</p>
        )}
        <div className="flex flex-col gap-3">
          {categories.map((category) => (
            <CategoryCard
              key={category.id}
              category={category}
              ruleCount={
                rules.filter((r) => r.category_id === category.id).length
              }
              onRename={(cat, name, color) =>
                renameCategoryMutation.mutate({ category: cat, name, color })
              }
              onDelete={(cat) => deleteCategoryMutation.mutate(cat)}
              onShowRules={() => onShowRules(category.id)}
            />
          ))}
        </div>
      </div>

      <Dialog
        open={addCategoryDialog.open}
        onOpenChange={addCategoryDialog.setOpen}
      >
        <DialogContent>
          <form onSubmit={handleCreateCategory} className="flex flex-col gap-4">
            <DialogHeader>
              <DialogTitle>Add category</DialogTitle>
            </DialogHeader>
            <div className="flex items-center gap-2">
              <input
                type="color"
                value={newColor}
                onChange={(e) => setNewColor(e.target.value)}
                className="h-8 w-10 shrink-0 cursor-pointer rounded-md border bg-transparent p-0.5"
                aria-label="New category color"
              />
              <Input
                type="text"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder="New category name"
                className="flex-1"
                autoFocus
              />
            </div>
            {createCategoryMutation.isError && (
              <p className="text-xs text-destructive">
                {String(createCategoryMutation.error)}
              </p>
            )}
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => addCategoryDialog.setOpen(false)}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                disabled={!newName.trim() || createCategoryMutation.isPending}
              >
                Add category
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </>
  );
}

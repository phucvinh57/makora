import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";
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
import {
  categoryOptions,
  categoryValueLabels,
} from "@/components/CategoryOptions";
import {
  applySeedTaxonomy,
  createCategory,
  deleteCategory,
  listCategories,
  updateCategory,
} from "@/api";
import {
  categoryPath,
  categorySubtreeIds,
  orderedCategories,
} from "@/categories";
import { openDialog, useDialog } from "@/hooks/useDialog";
import type { Category } from "@/types";
import { CategoryCard, NO_PARENT } from "./CategoryCard";

const DEFAULT_COLOR = "#6366f1";

export function CategoriesTab() {
  const queryClient = useQueryClient();
  const [newName, setNewName] = useState("");
  const [newColor, setNewColor] = useState(DEFAULT_COLOR);
  const [newDescription, setNewDescription] = useState("");
  const [newParentId, setNewParentId] = useState<number | null>(null);
  const addCategoryDialog = useDialog("add-category");

  const categoriesQuery = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });

  const categories = categoriesQuery.data ?? [];
  const tree = orderedCategories(categories);
  const loading = categoriesQuery.isLoading;
  const error = categoriesQuery.error ? String(categoriesQuery.error) : null;

  function invalidateAll() {
    queryClient.invalidateQueries({ queryKey: ["categories"] });
    // Deleting a category clears the pins that pointed at it, and a rename or recolour is denormalized into the rows these two queries hold.
    queryClient.invalidateQueries({ queryKey: ["sessions"] });
    queryClient.invalidateQueries({ queryKey: ["activityStats"] });
  }

  const createCategoryMutation = useMutation({
    mutationFn: () =>
      createCategory(
        newName.trim(),
        newColor,
        newDescription.trim() || null,
        newParentId,
      ),
    onSuccess: () => {
      setNewName("");
      setNewColor(DEFAULT_COLOR);
      setNewDescription("");
      setNewParentId(null);
      addCategoryDialog.setOpen(false);
      invalidateAll();
    },
  });

  const saveCategoryMutation = useMutation({
    mutationFn: ({
      category,
      name,
      color,
      parentId,
      description,
    }: {
      category: Category;
      name: string;
      color: string;
      description: string | null;
      parentId: number | null;
    }) => updateCategory(category.id, name, color, description, parentId),
    onSuccess: invalidateAll,
    onError: (err) => toast.error(String(err)),
  });

  const deleteCategoryMutation = useMutation({
    mutationFn: (category: Category) => deleteCategory(category.id),
    onSuccess: invalidateAll,
    onError: (err) => toast.error(String(err)),
  });

  const applySeedMutation = useMutation({
    mutationFn: applySeedTaxonomy,
    onSuccess: invalidateAll,
    onError: (err) => toast.error(String(err)),
  });

  function handleCreateCategory(e: React.FormEvent) {
    e.preventDefault();
    if (!newName.trim()) return;
    createCategoryMutation.mutate();
  }

  function startAddSubcategory(parent: Category) {
    setNewName("");
    setNewColor(parent.color);
    setNewDescription("");
    setNewParentId(parent.id);
    openDialog("add-category");
  }

  // Deleting a parent takes its whole branch, which is a bigger deal than deleting a leaf.
  function handleDelete(category: Category) {
    const descendants = categorySubtreeIds(category.id, categories).size - 1;
    if (descendants === 0) {
      deleteCategoryMutation.mutate(category);
      return;
    }
    toast(`Delete ${category.name} and everything under it?`, {
      description: `This also deletes ${descendants} subcategor${
        descendants === 1 ? "y" : "ies"
      } and their pins. This cannot be undone.`,
      action: {
        label: "Delete branch",
        onClick: () => deleteCategoryMutation.mutate(category),
      },
      duration: 10000,
    });
  }

  const newParent =
    newParentId !== null
      ? categories.find((c) => c.id === newParentId)
      : undefined;

  return (
    <>
      {error && (
        <div className="mb-3 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}

      <div className="mb-4 flex justify-end">
        <Button
          size="sm"
          variant="outline"
          className="mr-2"
          onClick={() => applySeedMutation.mutate()}
          disabled={applySeedMutation.isPending}
        >
          Apply starter taxonomy
        </Button>
        <Button
          size="sm"
          onClick={() => {
            setNewParentId(null);
            setNewColor(DEFAULT_COLOR);
            setNewDescription("");
            openDialog("add-category");
          }}
        >
          Add category
        </Button>
      </div>

      <div className="flex-1 overflow-y-auto subtle-scrollbar">
        {!loading && categories.length === 0 && (
          <div className="flex flex-col items-start gap-2 text-sm text-muted-foreground">
            <p>
              No categories yet. Use “Apply starter taxonomy” above to begin.
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
              categories={categories}
              onSave={(cat, name, color, description, parentId) =>
                saveCategoryMutation.mutate({
                  category: cat,
                  name,
                  color,
                  description,
                  parentId,
                })
              }
              onDelete={handleDelete}
              onAddSubcategory={() => startAddSubcategory(category)}
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
              <DialogTitle>
                {newParent
                  ? `Add subcategory of ${categoryPath(newParent, categories)}`
                  : "Add category"}
              </DialogTitle>
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
            <Input
              type="text"
              value={newDescription}
              onChange={(e) => setNewDescription(e.target.value)}
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
              value={newParentId === null ? NO_PARENT : String(newParentId)}
              onValueChange={(value) =>
                setNewParentId(
                  !value || value === NO_PARENT ? null : Number(value),
                )
              }
              items={{
                [NO_PARENT]: "No parent (top level)",
                ...categoryValueLabels(categories),
              }}
            >
              <SelectTrigger className="w-full" aria-label="Parent category">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={NO_PARENT}>No parent (top level)</SelectItem>
                {categoryOptions(categories)}
              </SelectContent>
            </Select>
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

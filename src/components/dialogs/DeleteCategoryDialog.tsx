import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { category as categoryApi } from "@/api";
import { categorySubtreeIds } from "@/categories";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useCategories } from "@/hooks/useCategories";
import { useDialog } from "@/hooks/useDialog";
import { invalidateCategorizationQueries } from "@/queries";
import type { Category } from "@/types";

export interface DeleteCategoryDialogData {
  category: Category;
}

export function DeleteCategoryDialog() {
  const queryClient = useQueryClient();
  const { open, setOpen, data } =
    useDialog<DeleteCategoryDialogData>("delete-category");
  const { categories } = useCategories();
  const category = data?.category;
  const descendantCount = category
    ? categorySubtreeIds(category.id, categories).size - 1
    : 0;
  const branch = descendantCount > 0;

  const deleteMutation = useMutation({
    mutationFn: (target: Category) => categoryApi.delete(target.id),
    onSuccess: () => invalidateCategorizationQueries(queryClient),
    onError: (err) => toast.error(String(err)),
  });

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {branch
              ? `Delete ${category?.name} and everything under it?`
              : `Delete ${category?.name}?`}
          </DialogTitle>
          <DialogDescription>
            {branch
              ? `This also deletes ${descendantCount} subcategor${
                  descendantCount === 1 ? "y" : "ies"
                } and their pins and rules. `
              : "Pins and rules that file under this category are removed. "}
            This cannot be undone.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={() => setOpen(false)}>
            Cancel
          </Button>
          <Button
            type="button"
            variant="destructive"
            onClick={() => {
              if (!category) return;
              deleteMutation.mutate(category);
              setOpen(false);
            }}
          >
            {branch ? "Delete branch" : "Delete"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

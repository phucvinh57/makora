import { useMutation, useQueryClient } from "@tanstack/react-query";
import { category } from "@/api";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { useCategories } from "@/hooks/useCategories";
import { useDialog } from "@/hooks/useDialog";
import { invalidateCategorizationQueries } from "@/queries";
import type { Category } from "@/types";
import { CategoryForm } from "./CategoryForm";

const DEFAULT_COLOR = "#6366f1";

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

  const createMutation = useMutation({
    mutationFn: category.create,
    onSuccess: async (created) => {
      // The list has to know the new category before anyone can select it.
      await invalidateCategorizationQueries(queryClient);
      onCreated?.(created);
      onClose();
    },
  });

  return (
    <CategoryForm
      initial={{
        name: "",
        color: parent?.color ?? DEFAULT_COLOR,
        description: null,
        parentId: parent?.id ?? null,
      }}
      parentChoices={categories}
      title={(parentPath) =>
        parentPath ? `Add subcategory of ${parentPath}` : "Add category"
      }
      submitLabel="Add category"
      pending={createMutation.isPending}
      error={createMutation.error}
      onSubmit={(draft) => createMutation.mutate(draft)}
      onClose={onClose}
    />
  );
}

export interface AddCategoryDialogData {
  parent: Category | null;
  onCreated?: (category: Category) => void;
}

export function AddCategoryDialog() {
  const { open, setOpen, data } = useDialog("add-category");

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

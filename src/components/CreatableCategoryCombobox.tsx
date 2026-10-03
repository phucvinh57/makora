import type { ComponentProps } from "react";
import { CategoryCombobox } from "@/components/CategoryCombobox";
import type { AddCategoryDialogData } from "@/components/dialogs/AddCategoryDialog";
import { openDialog } from "@/hooks/useDialog";

export function CreatableCategoryCombobox({
  picker,
  onValueChange,
  ...props
}: Omit<ComponentProps<typeof CategoryCombobox>, "onCreate">) {
  return (
    <CategoryCombobox
      picker={picker}
      onValueChange={onValueChange}
      onCreate={() =>
        openDialog<AddCategoryDialogData>("add-category", {
          parent: null,
          onCreated: (created) => onValueChange(String(created.id)),
        })
      }
      {...props}
    />
  );
}

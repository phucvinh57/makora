import type { ComponentProps } from "react";
import { CategoryCombobox } from "@/components/CategoryCombobox";
import type { AddCategoryDialogData } from "@/components/dialogs/AddCategoryDialog";
import { openDialog } from "@/hooks/useDialog";

/**
 * A category combobox that can also make the category you are looking for: its “New category”
 * button opens the add dialog, and the category it creates is picked straight away.
 */
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

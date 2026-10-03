import { AddCategoryDialog } from "./AddCategoryDialog";
import { AddModelDialog } from "./AddModelDialog";
import { DeleteCategoryDialog } from "./DeleteCategoryDialog";
import { EditCategoryDialog } from "./EditCategoryDialog";
import { ResetCategorizationDialog } from "./ResetCategorizationDialog";
import { RuleDialog } from "./RuleDialog";

export function Dialogs() {
  return (
    <>
      <AddCategoryDialog />
      <AddModelDialog />
      <DeleteCategoryDialog />
      <EditCategoryDialog />
      <ResetCategorizationDialog />
      <RuleDialog />
    </>
  );
}

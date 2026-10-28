import { AddCategoryDialog } from "./AddCategoryDialog";
import { AddModelDialog } from "./AddModelDialog";
import { DeleteCategoryDialog } from "./DeleteCategoryDialog";
import { EditCategoryDialog } from "./EditCategoryDialog";
import { ProviderDialog } from "./ProviderDialog";
import { RemoveProviderDialog } from "./RemoveProviderDialog";
import { ResetCategorizationDialog } from "./ResetCategorizationDialog";
import { RuleDialog } from "./RuleDialog";

export function Dialogs() {
  return (
    <>
      <AddCategoryDialog />
      <AddModelDialog />
      <DeleteCategoryDialog />
      <EditCategoryDialog />
      <ProviderDialog />
      <RemoveProviderDialog />
      <ResetCategorizationDialog />
      <RuleDialog />
    </>
  );
}

import { AddCategoryDialog } from "./AddCategoryDialog";
import { AddModelDialog } from "./AddModelDialog";
import { DeleteCategoryDialog } from "./DeleteCategoryDialog";
import { EditCategoryDialog } from "./EditCategoryDialog";
import { ModelPriceDialog } from "./ModelPriceDialog";
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
      <ModelPriceDialog />
      <ProviderDialog />
      <RemoveProviderDialog />
      <ResetCategorizationDialog />
      <RuleDialog />
    </>
  );
}

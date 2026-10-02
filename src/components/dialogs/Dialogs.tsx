import { AddCategoryDialog } from "./AddCategoryDialog";
import { AddModelDialog } from "./AddModelDialog";
import { DeleteCategoryDialog } from "./DeleteCategoryDialog";
import { ResetCategorizationDialog } from "./ResetCategorizationDialog";
import { RuleDialog } from "./RuleDialog";

/**
 * Every dialog in the app, mounted once. Each opens through `openDialog(name, data)` from
 * wherever it is needed, so the page that wants one never renders it.
 */
export function Dialogs() {
  return (
    <>
      <AddCategoryDialog />
      <AddModelDialog />
      <DeleteCategoryDialog />
      <ResetCategorizationDialog />
      <RuleDialog />
    </>
  );
}

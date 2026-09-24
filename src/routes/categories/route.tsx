import { createFileRoute } from "@tanstack/react-router";
import { CategoriesTab } from "./-components/CategoriesTab";

export const Route = createFileRoute("/categories")({
  component: CategoriesPage,
});

function CategoriesPage() {
  return (
    <div className="flex h-full flex-col box-border">
      <header className="mb-4">
        <h1 className="m-0 text-lg font-semibold">Categories</h1>
        <p className="m-0 text-sm text-muted-foreground">
          Organize activity with categories
        </p>
      </header>
      <CategoriesTab />
    </div>
  );
}

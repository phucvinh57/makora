import { useState } from "react";
import { Button } from "@/components/ui/button";
import CategoriesPage from "./CategoriesPage";
import HistoryPage from "./HistoryPage";

type Tab = "history" | "categories";

export default function App() {
  const [tab, setTab] = useState<Tab>("history");

  return (
    <div className="flex h-screen flex-col box-border">
      <nav className="flex items-center gap-1 border-b px-6 py-3">
        <h1 className="m-0 mr-4 text-xl font-semibold">makora</h1>
        <Button
          variant={tab === "history" ? "secondary" : "ghost"}
          size="sm"
          onClick={() => setTab("history")}
        >
          History
        </Button>
        <Button
          variant={tab === "categories" ? "secondary" : "ghost"}
          size="sm"
          onClick={() => setTab("categories")}
        >
          Categories
        </Button>
      </nav>
      <div className="min-h-0 flex-1 p-6">
        {tab === "history" ? <HistoryPage /> : <CategoriesPage />}
      </div>
    </div>
  );
}

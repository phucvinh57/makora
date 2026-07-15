import {
  History,
  PanelLeftClose,
  PanelLeftOpen,
  Settings as SettingsIcon,
  Tags,
} from "lucide-react";
import { useState } from "react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import CategoriesPage from "./CategoriesPage";
import HistoryPage from "./HistoryPage";
import SettingsPage from "./SettingsPage";

type Tab = "history" | "categories" | "settings";

const NAV_ITEMS: { tab: Tab; label: string; icon: typeof History }[] = [
  { tab: "history", label: "History", icon: History },
  { tab: "categories", label: "Categories", icon: Tags },
  { tab: "settings", label: "Settings", icon: SettingsIcon },
];

const COLLAPSE_STORAGE_KEY = "makora-sidebar-collapsed";

export default function App() {
  const [tab, setTab] = useState<Tab>("history");
  const [collapsed, setCollapsed] = useState(
    () => localStorage.getItem(COLLAPSE_STORAGE_KEY) === "1",
  );

  function toggleCollapsed() {
    setCollapsed((prev) => {
      const next = !prev;
      localStorage.setItem(COLLAPSE_STORAGE_KEY, next ? "1" : "0");
      return next;
    });
  }

  return (
    <div className="flex h-screen box-border">
      <aside
        className={cn(
          "flex shrink-0 flex-col border-r bg-sidebar text-sidebar-foreground transition-[width] duration-150",
          collapsed ? "w-14" : "w-52",
        )}
      >
        <div
          className={cn(
            "flex items-center py-4",
            collapsed ? "justify-center px-2" : "justify-between px-4",
          )}
        >
          {!collapsed && <h1 className="m-0 text-xl font-semibold">makora</h1>}
          <button
            type="button"
            onClick={toggleCollapsed}
            aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
            className="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-sidebar-accent/50 hover:text-sidebar-accent-foreground"
          >
            {collapsed ? (
              <PanelLeftOpen className="size-4" />
            ) : (
              <PanelLeftClose className="size-4" />
            )}
          </button>
        </div>
        <nav className="flex flex-1 flex-col gap-1 px-2">
          {NAV_ITEMS.map(({ tab: itemTab, label, icon: Icon }) => {
            const itemClassName = cn(
              "flex items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-sm font-medium transition-colors",
              collapsed && "justify-center px-0",
              tab === itemTab
                ? "bg-sidebar-accent text-sidebar-accent-foreground"
                : "text-muted-foreground hover:bg-sidebar-accent/50 hover:text-sidebar-accent-foreground",
            );

            if (!collapsed) {
              return (
                <button
                  key={itemTab}
                  type="button"
                  onClick={() => setTab(itemTab)}
                  className={itemClassName}
                >
                  <Icon className="size-4 shrink-0" />
                  {label}
                </button>
              );
            }

            return (
              <Tooltip key={itemTab}>
                <TooltipTrigger
                  className={itemClassName}
                  onClick={() => setTab(itemTab)}
                  aria-label={label}
                >
                  <Icon className="size-4 shrink-0" />
                </TooltipTrigger>
                <TooltipContent side="right">{label}</TooltipContent>
              </Tooltip>
            );
          })}
        </nav>
      </aside>
      <div className="min-w-0 flex-1 overflow-y-auto p-6">
        {tab === "history" ? (
          <HistoryPage />
        ) : tab === "categories" ? (
          <CategoriesPage />
        ) : (
          <SettingsPage />
        )}
      </div>
    </div>
  );
}

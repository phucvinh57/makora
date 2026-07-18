import {
  BarChart3,
  History,
  Hourglass,
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
import { createRootRoute, Link, Outlet } from "@tanstack/react-router";

type NavItem = { to: string; label: string; icon: typeof History };

const NAV_ITEMS: NavItem[] = [
  { to: "/statistics", label: "Statistics", icon: BarChart3 },
  { to: "/history", label: "History", icon: History },
  { to: "/labelling", label: "Labelling", icon: Tags },
  { to: "/settings", label: "Settings", icon: SettingsIcon },
];

const COLLAPSE_STORAGE_KEY = "makora-sidebar-collapsed";

export const Route = createRootRoute({
  component: RootLayout,
});

function RootLayout() {
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
          {!collapsed && (
            <div className="flex items-center gap-2">
              <div className="flex size-7 shrink-0 items-center justify-center rounded-md bg-primary text-primary-foreground">
                <Hourglass className="size-4" />
              </div>
              <span className="text-base font-semibold tracking-tight">
                Makora
              </span>
            </div>
          )}
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
          {NAV_ITEMS.map(({ to, label, icon: Icon }) => {
            const itemClassName = cn(
              "flex items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-sm font-medium transition-colors",
              collapsed && "justify-center px-0",
            );
            const inactiveClassName =
              "text-muted-foreground hover:bg-sidebar-accent/50 hover:text-sidebar-accent-foreground";
            const activeClassName =
              "bg-sidebar-primary/10 text-sidebar-primary";

            if (!collapsed) {
              return (
                <Link
                  key={to}
                  to={to}
                  className={itemClassName}
                  activeProps={{ className: activeClassName }}
                  inactiveProps={{ className: inactiveClassName }}
                >
                  <Icon className="size-4 shrink-0" />
                  {label}
                </Link>
              );
            }

            return (
              <Tooltip key={to}>
                <TooltipTrigger
                  render={
                    <Link
                      to={to}
                      className={itemClassName}
                      activeProps={{ className: activeClassName }}
                      inactiveProps={{ className: inactiveClassName }}
                      aria-label={label}
                    >
                      <Icon className="size-4 shrink-0" />
                    </Link>
                  }
                />
                <TooltipContent side="right">{label}</TooltipContent>
              </Tooltip>
            );
          })}
        </nav>
      </aside>
      <div className="@container/main min-w-0 flex-1 overflow-y-auto subtle-scrollbar p-6">
        <Outlet />
      </div>
    </div>
  );
}

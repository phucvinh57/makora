import {
  createRootRoute,
  Link,
  type LinkProps,
  Outlet,
} from "@tanstack/react-router";
import {
  BarChart3,
  History,
  Hourglass,
  PanelLeftClose,
  PanelLeftOpen,
  ScrollText,
  Settings as SettingsIcon,
  Tags,
} from "lucide-react";
import { BackgroundTasksIndicator } from "@/components/BackgroundTasksIndicator";
import { Dialogs } from "@/components/dialogs/Dialogs";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useBackgroundTaskSync } from "@/hooks/useBackgroundTasks";
import { useNavigationRequests } from "@/hooks/useNavigationRequests";
import { FLAG_CODEC, usePersistentState } from "@/hooks/usePersistentState";
import { useSettingsLoadIssue } from "@/hooks/useSettingsLoadIssue";
import { cn } from "@/lib/utils";

type NavItem = {
  to: LinkProps["to"];
  label: string;
  icon: typeof History;
};

const NAV_ITEMS: NavItem[] = [
  { to: "/statistics", label: "Statistics", icon: BarChart3 },
  { to: "/categories", label: "Categories", icon: Tags },
  { to: "/history", label: "History", icon: History },
  { to: "/logs", label: "Logs", icon: ScrollText },
  { to: "/settings", label: "Settings", icon: SettingsIcon },
];

const COLLAPSE_STORAGE_KEY = "makora-sidebar-collapsed";

export const Route = createRootRoute({
  component: RootLayout,
});

function NavLink({
  item: { to, label, icon: Icon },
  collapsed,
}: {
  item: NavItem;
  collapsed: boolean;
}) {
  const link = (
    <Link
      to={to}
      className={cn(
        "flex items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-sm font-medium transition-colors",
        collapsed && "justify-center px-0",
      )}
      activeProps={{ className: "bg-sidebar-primary/10 text-sidebar-primary" }}
      inactiveProps={{
        className:
          "text-muted-foreground hover:bg-sidebar-accent/50 hover:text-sidebar-accent-foreground",
      }}
      aria-label={collapsed ? label : undefined}
    >
      <Icon className="size-4 shrink-0" />
      {!collapsed && label}
    </Link>
  );

  if (!collapsed) return link;
  return (
    <Tooltip>
      <TooltipTrigger render={link} />
      <TooltipContent side="right">{label}</TooltipContent>
    </Tooltip>
  );
}

function RootLayout() {
  const [collapsed, setCollapsed] = usePersistentState(
    COLLAPSE_STORAGE_KEY,
    false,
    FLAG_CODEC,
  );
  useBackgroundTaskSync();
  useNavigationRequests();
  useSettingsLoadIssue();

  return (
    <div className="flex h-screen box-border">
      <Dialogs />
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
            onClick={() => setCollapsed(!collapsed)}
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
          {NAV_ITEMS.map((item) => (
            <NavLink key={item.label} item={item} collapsed={collapsed} />
          ))}
        </nav>
        <div className="px-2 pb-2">
          <BackgroundTasksIndicator collapsed={collapsed} />
        </div>
      </aside>
      <div className="@container/main min-w-0 flex-1 overflow-y-auto subtle-scrollbar p-6">
        <Outlet />
      </div>
    </div>
  );
}

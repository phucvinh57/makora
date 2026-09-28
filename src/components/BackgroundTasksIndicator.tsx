import { Loader2 } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useBackgroundTasks } from "@/hooks/useBackgroundTasks";
import { cn } from "@/lib/utils";

export function BackgroundTasksIndicator({
  collapsed,
}: {
  collapsed: boolean;
}) {
  const tasks = useBackgroundTasks();
  if (tasks.length === 0) return null;

  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <div
            className={cn(
              "flex items-center gap-2 rounded-md px-2.5 py-1.5 text-sm text-muted-foreground",
              collapsed && "justify-center px-0",
            )}
          >
            <Loader2 className="size-4 shrink-0 animate-spin" />
            {!collapsed && (
              <span>
                {tasks.length} task{tasks.length === 1 ? "" : "s"} running
              </span>
            )}
          </div>
        }
      />
      <TooltipContent
        side="right"
        className="flex max-w-none flex-col items-start gap-1.5 text-left"
      >
        {tasks.map((task) => (
          <div key={task.id} className="flex flex-col">
            <span className="font-medium">{task.label}</span>
            <span className="text-background/70">{task.progress}</span>
          </div>
        ))}
      </TooltipContent>
    </Tooltip>
  );
}

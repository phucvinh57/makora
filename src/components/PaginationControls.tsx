import { ChevronLeft, ChevronRight } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/** Previous/next with a "Page 2 of 5" readout. `page` is zero-based. */
export function PaginationControls({
  page,
  pageCount,
  label,
  onPage,
  className,
}: {
  page: number;
  pageCount: number;
  /** What is being counted, e.g. "120 sessions". */
  label: string;
  onPage: (page: number) => void;
  className?: string;
}) {
  return (
    <nav
      aria-label="pagination"
      className={cn(
        "flex items-center justify-center gap-2 text-sm",
        className,
      )}
    >
      <Button
        variant="ghost"
        size="sm"
        disabled={page <= 0}
        onClick={() => onPage(page - 1)}
      >
        <ChevronLeft />
        Previous
      </Button>
      <span className="px-2 text-muted-foreground">
        Page {page + 1} of {pageCount} ({label})
      </span>
      <Button
        variant="ghost"
        size="sm"
        disabled={page + 1 >= pageCount}
        onClick={() => onPage(page + 1)}
      >
        Next
        <ChevronRight />
      </Button>
    </nav>
  );
}

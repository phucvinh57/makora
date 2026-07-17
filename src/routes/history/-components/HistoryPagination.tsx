import {
  Pagination,
  PaginationContent,
  PaginationItem,
  PaginationNext,
  PaginationPrevious,
} from "@/components/ui/pagination";

export function HistoryPagination({
  page,
  pageCount,
  total,
  onPrev,
  onNext,
}: {
  page: number;
  pageCount: number;
  total: number;
  onPrev: () => void;
  onNext: () => void;
}) {
  return (
    <footer className="flex items-center justify-center gap-4 pt-3 text-sm">
      <Pagination className="w-auto">
        <PaginationContent>
          <PaginationItem>
            <PaginationPrevious
              href="#"
              aria-disabled={page === 0}
              className={
                page === 0 ? "pointer-events-none opacity-50" : undefined
              }
              onClick={(e) => {
                e.preventDefault();
                if (page > 0) onPrev();
              }}
            />
          </PaginationItem>
          <PaginationItem>
            <span className="px-2 text-sm text-muted-foreground">
              Page {page + 1} of {pageCount} ({total} sessions)
            </span>
          </PaginationItem>
          <PaginationItem>
            <PaginationNext
              href="#"
              aria-disabled={page + 1 >= pageCount}
              className={
                page + 1 >= pageCount
                  ? "pointer-events-none opacity-50"
                  : undefined
              }
              onClick={(e) => {
                e.preventDefault();
                if (page + 1 < pageCount) onNext();
              }}
            />
          </PaginationItem>
        </PaginationContent>
      </Pagination>
    </footer>
  );
}

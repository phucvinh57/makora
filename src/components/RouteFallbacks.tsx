import { type ErrorComponentProps, Link } from "@tanstack/react-router";
import { Button } from "@/components/ui/button";

export function RouteError({ error, reset }: ErrorComponentProps) {
  return (
    <div className="mx-auto flex max-w-md flex-col items-start gap-3 py-16">
      <h1 className="m-0 text-lg font-semibold">Something went wrong</h1>
      <p className="m-0 text-sm text-muted-foreground">
        {error instanceof Error ? error.message : String(error)}
      </p>
      <Button size="sm" onClick={reset}>
        Try again
      </Button>
    </div>
  );
}

export function RouteNotFound() {
  return (
    <div className="mx-auto flex max-w-md flex-col items-start gap-3 py-16">
      <h1 className="m-0 text-lg font-semibold">Page not found</h1>
      <Link to="/statistics" className="text-sm text-primary hover:underline">
        Back to statistics
      </Link>
    </div>
  );
}

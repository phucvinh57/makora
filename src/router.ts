import { createRouter } from "@tanstack/react-router";
import { RouteError, RouteNotFound } from "@/components/RouteFallbacks";
import { routeTree } from "./routeTree.gen";

export const router = createRouter({
  routeTree,
  defaultErrorComponent: RouteError,
  defaultNotFoundComponent: RouteNotFound,
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

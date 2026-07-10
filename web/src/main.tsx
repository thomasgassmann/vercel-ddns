import "@fontsource-variable/roboto/index.css";
import { QueryCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import React from "react";
import { createRoot } from "react-dom/client";
import { Unauthenticated } from "./api";
import { routeTree } from "./routeTree.gen";

const queryClient = new QueryClient({
    queryCache: new QueryCache({
        onError: (error) => {
            if (error instanceof Unauthenticated) {
                void router.navigate({ to: "/login" });
            }
        },
    }),
});

const router = createRouter({ routeTree, context: { queryClient } });

declare module "@tanstack/react-router" {
    interface Register {
        router: typeof router;
    }
}

createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
        <QueryClientProvider client={queryClient}>
            <RouterProvider router={router} />
        </QueryClientProvider>
    </React.StrictMode>,
);

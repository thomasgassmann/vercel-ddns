import CssBaseline from "@mui/material/CssBaseline";
import { ThemeProvider } from "@mui/material/styles";
import type { QueryClient } from "@tanstack/react-query";
import { Outlet, createRootRouteWithContext } from "@tanstack/react-router";
import { theme } from "../theme";

interface RouterContext {
    queryClient: QueryClient;
}

export const Route = createRootRouteWithContext<RouterContext>()({
    component: () => (
        <ThemeProvider theme={theme} defaultMode="system">
            <CssBaseline />
            <Outlet />
        </ThemeProvider>
    ),
});

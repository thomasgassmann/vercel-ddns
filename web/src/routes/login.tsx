import { createFileRoute, redirect } from "@tanstack/react-router";
import { meQuery } from "../api";

export const Route = createFileRoute("/login")({
    beforeLoad: async ({ context }) => {
        try {
            await context.queryClient.ensureQueryData(meQuery);
            throw redirect({ to: "/" });
        } catch (error) {
            if (error instanceof Response) {
                throw error;
            }

            // Start the server-side IdP flow rather than navigating within the SPA.
            throw redirect({ href: "/auth/login", reloadDocument: true, replace: true });
        }
    },
});

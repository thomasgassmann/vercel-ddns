import DarkMode from "@mui/icons-material/DarkMode";
import DnsIcon from "@mui/icons-material/Dns";
import LightMode from "@mui/icons-material/LightMode";
import SettingsBrightness from "@mui/icons-material/SettingsBrightness";
import AppBar from "@mui/material/AppBar";
import Box from "@mui/material/Box";
import Button from "@mui/material/Button";
import Container from "@mui/material/Container";
import IconButton from "@mui/material/IconButton";
import Toolbar from "@mui/material/Toolbar";
import Typography from "@mui/material/Typography";
import { useColorScheme } from "@mui/material/styles";
import { useQueryClient } from "@tanstack/react-query";
import { Outlet, createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { meQuery } from "../api";

export const Route = createFileRoute("/_authed")({
    beforeLoad: async ({ context }) => {
        try {
            const me = await context.queryClient.ensureQueryData(meQuery);
            return { me };
        } catch {
            throw redirect({ to: "/login" });
        }
    },
    component: AuthedLayout,
});

const MODES = ["system", "light", "dark"] as const;
const MODE_ICONS = {
    system: <SettingsBrightness />,
    light: <LightMode />,
    dark: <DarkMode />,
};

function ColorModeToggle() {
    const { mode = "system", setMode } = useColorScheme();
    const next = MODES[(MODES.indexOf(mode) + 1) % MODES.length];
    return (
        <IconButton color="inherit" title={`Color mode: ${mode}`} onClick={() => setMode(next)}>
            {MODE_ICONS[mode]}
        </IconButton>
    );
}

function AuthedLayout() {
    const { me } = Route.useRouteContext();
    const queryClient = useQueryClient();
    const navigate = useNavigate();

    const logout = async () => {
        await fetch("/auth/logout", { method: "POST" });
        queryClient.clear();
        await navigate({ to: "/login" });
    };

    return (
        <>
            <AppBar position="sticky">
                <Toolbar>
                    <DnsIcon />
                    <Typography variant="h6" sx={{ ml: 1 }}>
                        ddnser
                    </Typography>
                    <Box sx={{ flexGrow: 1 }} />
                    <ColorModeToggle />
                    <Typography variant="body2" sx={{ mx: 2 }}>
                        {me.email ?? me.sub}
                    </Typography>
                    <Button color="inherit" onClick={logout}>
                        Sign out
                    </Button>
                </Toolbar>
            </AppBar>
            <Container sx={{ py: 3 }}>
                <Outlet />
            </Container>
        </>
    );
}

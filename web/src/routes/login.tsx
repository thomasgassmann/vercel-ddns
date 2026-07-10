import DnsIcon from "@mui/icons-material/Dns";
import Button from "@mui/material/Button";
import Stack from "@mui/material/Stack";
import Typography from "@mui/material/Typography";
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/login")({
    component: Login,
});

function Login() {
    return (
        <Stack
            spacing={3}
            sx={{ minHeight: "80vh", alignItems: "center", justifyContent: "center" }}
        >
            <DnsIcon sx={{ fontSize: 72 }} color="primary" />
            <Typography variant="h4">ddnser</Typography>
            <Button variant="contained" href="/auth/login">
                Sign in
            </Button>
        </Stack>
    );
}

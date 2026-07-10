import AddIcon from "@mui/icons-material/Add";
import DeleteIcon from "@mui/icons-material/Delete";
import EditIcon from "@mui/icons-material/Edit";
import SyncIcon from "@mui/icons-material/Sync";
import Alert from "@mui/material/Alert";
import Button from "@mui/material/Button";
import Card from "@mui/material/Card";
import CardContent from "@mui/material/CardContent";
import Chip from "@mui/material/Chip";
import IconButton from "@mui/material/IconButton";
import Stack from "@mui/material/Stack";
import Table from "@mui/material/Table";
import TableBody from "@mui/material/TableBody";
import TableCell from "@mui/material/TableCell";
import TableHead from "@mui/material/TableHead";
import TableRow from "@mui/material/TableRow";
import Tooltip from "@mui/material/Tooltip";
import Typography from "@mui/material/Typography";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import { api, entriesQuery, statusQuery, type Entry, type SyncOutcome } from "../../api";
import ConfirmDelete from "../../components/ConfirmDelete";
import EntryDialog from "../../components/EntryDialog";

export const Route = createFileRoute("/_authed/")({
    loader: ({ context }) => {
        void context.queryClient.prefetchQuery(entriesQuery);
        void context.queryClient.prefetchQuery(statusQuery);
    },
    component: Home,
});

function SyncedIp({ ip, at, show }: { ip: string | null; at: string | null; show: boolean }) {
    if (!show) {
        return null;
    }

    if (!ip) {
        return <span>never</span>;
    }

    return (
        <Tooltip title={at ? new Date(at).toLocaleString() : ""}>
            <span>{ip}</span>
        </Tooltip>
    );
}

function describeSync(sync: SyncOutcome): string {
    const finished = new Date(sync.finished_at).toLocaleString();
    const counts = `${sync.created} created, ${sync.updated} updated, ${sync.unchanged} unchanged, ${sync.failed} failed`;
    return `${finished} (${sync.source}) — ${counts}`;
}

function StatusCard() {
    const { data: status } = useSuspenseQuery(statusQuery);
    const queryClient = useQueryClient();
    const sync = useMutation({
        mutationFn: api.syncNow,
        onSettled: async () => {
            await queryClient.invalidateQueries({ queryKey: ["status"] });
            await queryClient.invalidateQueries({ queryKey: ["entries"] });
        },
    });

    const last = status.last_sync;
    return (
        <Card>
            <CardContent>
                <Stack direction="row" spacing={2} sx={{ alignItems: "center", flexWrap: "wrap" }}>
                    <Typography variant="h6" sx={{ flexGrow: 1 }}>
                        Status
                    </Typography>
                    <Button
                        variant="outlined"
                        startIcon={<SyncIcon />}
                        onClick={() => sync.mutate()}
                        disabled={sync.isPending}
                    >
                        {sync.isPending ? "Syncing..." : "Sync now"}
                    </Button>
                </Stack>
                {last ? (
                    <Stack spacing={1} sx={{ mt: 1 }}>
                        <Typography variant="body2">Last sync: {describeSync(last)}</Typography>
                        <Stack direction="row" spacing={1}>
                            {last.ipv4 && <Chip size="small" label={`IPv4 ${last.ipv4}`} />}
                            {last.ipv6 && <Chip size="small" label={`IPv6 ${last.ipv6}`} />}
                            <Chip
                                size="small"
                                color={last.failed > 0 || last.error ? "error" : "success"}
                                label={last.failed > 0 || last.error ? "problems" : "healthy"}
                            />
                        </Stack>
                        {last.error && <Alert severity="error">{last.error}</Alert>}
                    </Stack>
                ) : (
                    <Typography variant="body2" sx={{ mt: 1 }}>
                        No sync has run yet.
                    </Typography>
                )}
                <Typography
                    variant="caption"
                    color="text.secondary"
                    sx={{ display: "block", mt: 1 }}
                >
                    Automatic sync every {Math.round(status.sync_interval_secs / 60)} minutes, plus
                    on every router webhook.
                </Typography>
            </CardContent>
        </Card>
    );
}

function Home() {
    const { data: entries } = useSuspenseQuery(entriesQuery);
    const queryClient = useQueryClient();
    const [dialog, setDialog] = useState<{ open: boolean; entry: Entry | null }>({
        open: false,
        entry: null,
    });
    const [deleting, setDeleting] = useState<Entry | null>(null);

    const remove = useMutation({
        mutationFn: (id: number) => api.deleteEntry(id),
        onSuccess: async () => {
            await queryClient.invalidateQueries({ queryKey: ["entries"] });
            setDeleting(null);
        },
    });

    return (
        <Stack spacing={3}>
            <StatusCard />
            <Card>
                <CardContent>
                    <Stack direction="row" spacing={2} sx={{ alignItems: "center" }}>
                        <Typography variant="h6" sx={{ flexGrow: 1 }}>
                            DNS entries
                        </Typography>
                        <Button
                            variant="contained"
                            startIcon={<AddIcon />}
                            onClick={() => setDialog({ open: true, entry: null })}
                        >
                            Add entry
                        </Button>
                    </Stack>
                    <Table size="small" sx={{ mt: 2 }}>
                        <TableHead>
                            <TableRow>
                                <TableCell>FQDN</TableCell>
                                <TableCell>Records</TableCell>
                                <TableCell>TTL</TableCell>
                                <TableCell>IPv6 target</TableCell>
                                <TableCell>Last synced</TableCell>
                                <TableCell align="right" />
                            </TableRow>
                        </TableHead>
                        <TableBody>
                            {entries.map((entry) => (
                                <TableRow key={entry.id} hover>
                                    <TableCell>{entry.fqdn}</TableCell>
                                    <TableCell>
                                        <Stack direction="row" spacing={0.5}>
                                            {entry.ipv4 && <Chip size="small" label="A" />}
                                            {entry.ipv6 && <Chip size="small" label="AAAA" />}
                                        </Stack>
                                    </TableCell>
                                    <TableCell>{entry.ttl}</TableCell>
                                    <TableCell>
                                        {entry.ipv6
                                            ? (entry.ipv6_override ?? "this host's IPv6")
                                            : "—"}
                                    </TableCell>
                                    <TableCell>
                                        <Stack spacing={0.5}>
                                            <SyncedIp
                                                ip={entry.ipv4 ? entry.last_synced_ipv4 : null}
                                                at={entry.last_synced_ipv4_at}
                                                show={entry.ipv4}
                                            />
                                            <SyncedIp
                                                ip={entry.ipv6 ? entry.last_synced_ipv6 : null}
                                                at={entry.last_synced_ipv6_at}
                                                show={entry.ipv6}
                                            />
                                        </Stack>
                                    </TableCell>
                                    <TableCell align="right">
                                        <IconButton
                                            size="small"
                                            onClick={() => setDialog({ open: true, entry })}
                                        >
                                            <EditIcon fontSize="small" />
                                        </IconButton>
                                        <IconButton size="small" onClick={() => setDeleting(entry)}>
                                            <DeleteIcon fontSize="small" />
                                        </IconButton>
                                    </TableCell>
                                </TableRow>
                            ))}
                            {entries.length === 0 && (
                                <TableRow>
                                    <TableCell colSpan={6}>
                                        <Typography
                                            variant="body2"
                                            color="text.secondary"
                                            sx={{ py: 2 }}
                                        >
                                            No entries yet. Add a domain to keep it pointed at your
                                            home IP.
                                        </Typography>
                                    </TableCell>
                                </TableRow>
                            )}
                        </TableBody>
                    </Table>
                </CardContent>
            </Card>
            {dialog.open && (
                <EntryDialog
                    open={dialog.open}
                    entry={dialog.entry}
                    onClose={() => setDialog({ open: false, entry: null })}
                />
            )}
            <ConfirmDelete
                entry={deleting}
                pending={remove.isPending}
                error={remove.isError ? remove.error.message : null}
                onCancel={() => setDeleting(null)}
                onConfirm={() => deleting && remove.mutate(deleting.id)}
            />
        </Stack>
    );
}

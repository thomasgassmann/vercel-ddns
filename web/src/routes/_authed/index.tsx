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
import Typography from "@mui/material/Typography";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import { api, recordsQuery, statusQuery, type DnsRecord, type SyncOutcome } from "../../api";
import ConfirmDelete from "../../components/ConfirmDelete";
import RecordDialog from "../../components/RecordDialog";

export const Route = createFileRoute("/_authed/")({
    loader: ({ context }) => {
        void context.queryClient.prefetchQuery(recordsQuery);
        void context.queryClient.prefetchQuery(statusQuery);
    },
    component: Home,
});

function describeSync(sync: SyncOutcome): string {
    const finished = new Date(sync.finished_at).toLocaleString();
    const counts = `${sync.created} created, ${sync.updated} updated, ${sync.unchanged} unchanged, ${sync.failed} failed`;
    return `${finished} (${sync.source}) - ${counts}`;
}

function StatusCard() {
    const { data: status } = useSuspenseQuery(statusQuery);
    const queryClient = useQueryClient();
    const sync = useMutation({
        mutationFn: api.syncNow,
        onSettled: async () => {
            await queryClient.invalidateQueries({ queryKey: ["status"] });
            await queryClient.invalidateQueries({ queryKey: ["records"] });
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
    const { data: records } = useSuspenseQuery(recordsQuery);
    const queryClient = useQueryClient();
    const [dialog, setDialog] = useState<{ open: boolean; record: DnsRecord | null }>({
        open: false,
        record: null,
    });
    const [deleting, setDeleting] = useState<DnsRecord | null>(null);

    const remove = useMutation({
        mutationFn: (id: number) => api.deleteRecord(id),
        onSuccess: async () => {
            await queryClient.invalidateQueries({ queryKey: ["records"] });
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
                            DNS records
                        </Typography>
                        <Button
                            variant="contained"
                            startIcon={<AddIcon />}
                            onClick={() => setDialog({ open: true, record: null })}
                        >
                            Add record
                        </Button>
                    </Stack>
                    <Table size="small" sx={{ mt: 2 }}>
                        <TableHead>
                            <TableRow>
                                <TableCell>FQDN</TableCell>
                                <TableCell>Type</TableCell>
                                <TableCell>Value</TableCell>
                                <TableCell>TTL</TableCell>
                                <TableCell align="right" />
                            </TableRow>
                        </TableHead>
                        <TableBody>
                            {records.map((record) => (
                                <TableRow key={record.id} hover>
                                    <TableCell>{record.fqdn}</TableCell>
                                    <TableCell>
                                        <Stack direction="row" spacing={0.5}>
                                            <Chip size="small" label={record.record_type} />
                                        </Stack>
                                    </TableCell>
                                    <TableCell>
                                        {record.value ?? <em>dynamic</em>}
                                        {record.record_type === "SRV" &&
                                            record.priority !== null && (
                                                <Typography
                                                    variant="caption"
                                                    sx={{ display: "block" }}
                                                >
                                                    priority {record.priority}, weight{" "}
                                                    {record.weight}, port {record.port}
                                                </Typography>
                                            )}
                                        {record.record_type === "MX" &&
                                            record.priority !== null && (
                                                <Typography
                                                    variant="caption"
                                                    sx={{ display: "block" }}
                                                >
                                                    priority {record.priority}
                                                </Typography>
                                            )}
                                    </TableCell>
                                    <TableCell>{record.ttl}</TableCell>
                                    <TableCell align="right">
                                        <IconButton
                                            size="small"
                                            onClick={() => setDialog({ open: true, record })}
                                        >
                                            <EditIcon fontSize="small" />
                                        </IconButton>
                                        <IconButton
                                            size="small"
                                            onClick={() => setDeleting(record)}
                                        >
                                            <DeleteIcon fontSize="small" />
                                        </IconButton>
                                    </TableCell>
                                </TableRow>
                            ))}
                            {records.length === 0 && (
                                <TableRow>
                                    <TableCell colSpan={5}>
                                        <Typography
                                            variant="body2"
                                            color="text.secondary"
                                            sx={{ py: 2 }}
                                        >
                                            No records yet. Add one to manage it here.
                                        </Typography>
                                    </TableCell>
                                </TableRow>
                            )}
                        </TableBody>
                    </Table>
                </CardContent>
            </Card>
            {dialog.open && (
                <RecordDialog
                    open={dialog.open}
                    record={dialog.record}
                    onClose={() => setDialog({ open: false, record: null })}
                />
            )}
            <ConfirmDelete
                record={deleting}
                pending={remove.isPending}
                error={remove.isError ? remove.error.message : null}
                onCancel={() => setDeleting(null)}
                onConfirm={() => deleting && remove.mutate(deleting.id)}
            />
        </Stack>
    );
}

import Alert from "@mui/material/Alert";
import Button from "@mui/material/Button";
import Checkbox from "@mui/material/Checkbox";
import Dialog from "@mui/material/Dialog";
import DialogActions from "@mui/material/DialogActions";
import DialogContent from "@mui/material/DialogContent";
import DialogTitle from "@mui/material/DialogTitle";
import FormControlLabel from "@mui/material/FormControlLabel";
import FormGroup from "@mui/material/FormGroup";
import Stack from "@mui/material/Stack";
import TextField from "@mui/material/TextField";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { api, type Entry, type EntryInput } from "../api";

interface Props {
    open: boolean;
    entry: Entry | null;
    onClose: () => void;
}

export default function EntryDialog({ open, entry, onClose }: Props) {
    const queryClient = useQueryClient();
    const [fqdn, setFqdn] = useState(entry?.fqdn ?? "");
    const [ipv4, setIpv4] = useState(entry?.ipv4 ?? true);
    const [ipv6, setIpv6] = useState(entry?.ipv6 ?? false);
    const [ttl, setTtl] = useState(String(entry?.ttl ?? 3600));
    const [ipv6Override, setIpv6Override] = useState(entry?.ipv6_override ?? "");

    const save = useMutation({
        mutationFn: (input: EntryInput) =>
            entry ? api.updateEntry(entry.id, input) : api.createEntry(input),
        onSuccess: async () => {
            await queryClient.invalidateQueries({ queryKey: ["entries"] });
            await queryClient.invalidateQueries({ queryKey: ["status"] });
            onClose();
        },
    });

    const submit = () => {
        save.mutate({
            fqdn: fqdn.trim(),
            ipv4,
            ipv6,
            ttl: Number(ttl),
            ipv6_override: ipv6 && ipv6Override.trim() ? ipv6Override.trim() : null,
        });
    };

    return (
        <Dialog open={open} onClose={onClose} fullWidth maxWidth="sm">
            <DialogTitle>{entry ? `Edit ${entry.fqdn}` : "Add entry"}</DialogTitle>
            <DialogContent>
                <Stack spacing={2} sx={{ mt: 1 }}>
                    <TextField
                        label="FQDN"
                        placeholder="home.example.com"
                        value={fqdn}
                        onChange={(e) => setFqdn(e.target.value)}
                        autoFocus
                        fullWidth
                    />
                    <FormGroup row>
                        <FormControlLabel
                            control={
                                <Checkbox
                                    checked={ipv4}
                                    onChange={(e) => setIpv4(e.target.checked)}
                                />
                            }
                            label="IPv4 (A record, router address)"
                        />
                        <FormControlLabel
                            control={
                                <Checkbox
                                    checked={ipv6}
                                    onChange={(e) => setIpv6(e.target.checked)}
                                />
                            }
                            label="IPv6 (AAAA record)"
                        />
                    </FormGroup>
                    {!ipv4 && !ipv6 && (
                        <Alert severity="warning">Enable at least one of IPv4 and IPv6.</Alert>
                    )}
                    <TextField
                        label="TTL (seconds)"
                        type="number"
                        value={ttl}
                        onChange={(e) => setTtl(e.target.value)}
                        slotProps={{ htmlInput: { min: 60 } }}
                        fullWidth
                    />
                    {ipv6 && (
                        <TextField
                            label="IPv6 address (optional)"
                            placeholder="Leave empty to use this host's own IPv6"
                            helperText="Static IPv6 of the device serving this name; empty = the host running ddnser"
                            value={ipv6Override}
                            onChange={(e) => setIpv6Override(e.target.value)}
                            fullWidth
                        />
                    )}
                    {save.isError && <Alert severity="error">{save.error.message}</Alert>}
                </Stack>
            </DialogContent>
            <DialogActions>
                <Button onClick={onClose}>Cancel</Button>
                <Button
                    variant="contained"
                    onClick={submit}
                    disabled={save.isPending || !fqdn.trim() || (!ipv4 && !ipv6)}
                >
                    Save
                </Button>
            </DialogActions>
        </Dialog>
    );
}

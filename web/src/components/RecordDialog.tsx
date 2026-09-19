import Alert from "@mui/material/Alert";
import Button from "@mui/material/Button";
import Dialog from "@mui/material/Dialog";
import DialogActions from "@mui/material/DialogActions";
import DialogContent from "@mui/material/DialogContent";
import DialogTitle from "@mui/material/DialogTitle";
import FormControl from "@mui/material/FormControl";
import InputLabel from "@mui/material/InputLabel";
import MenuItem from "@mui/material/MenuItem";
import Select from "@mui/material/Select";
import Stack from "@mui/material/Stack";
import TextField from "@mui/material/TextField";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { api, type DnsRecord, type RecordInput, type RecordType } from "../api";

interface Props {
    open: boolean;
    record: DnsRecord | null;
    onClose: () => void;
}

const TYPES: RecordType[] = ["A", "AAAA", "CAA", "CNAME", "MX", "TXT", "SRV"];

const PLACEHOLDERS: Partial<Record<RecordType, string>> = {
    A: "Leave empty to use the router's public IPv4",
    AAAA: "2001:db8::1",
    CAA: "0 issue letsencrypt.org",
    CNAME: "target.example.com",
    MX: "mail.example.com",
    TXT: "text content, case preserved",
    SRV: "sip.example.com",
};

export default function RecordDialog({ open, record, onClose }: Props) {
    const queryClient = useQueryClient();
    const [fqdn, setFqdn] = useState(record?.fqdn ?? "");
    const [recordType, setRecordType] = useState<RecordType>(record?.record_type ?? "A");
    const [value, setValue] = useState(record?.value ?? "");
    const [ttl, setTtl] = useState(String(record?.ttl ?? 60));
    const [priority, setPriority] = useState(record?.priority?.toString() ?? "");
    const [weight, setWeight] = useState(record?.weight?.toString() ?? "");
    const [port, setPort] = useState(record?.port?.toString() ?? "");

    const isDynamicA = recordType === "A";
    const needsPriority = recordType === "MX" || recordType === "SRV";
    const needsSrvData = recordType === "SRV";

    const save = useMutation({
        mutationFn: (input: RecordInput) =>
            record ? api.updateRecord(record.id, input) : api.createRecord(input),
        onSuccess: async () => {
            await queryClient.invalidateQueries({ queryKey: ["records"] });
            await queryClient.invalidateQueries({ queryKey: ["status"] });
            onClose();
        },
    });

    const submit = () => {
        save.mutate({
            fqdn: fqdn.trim(),
            record_type: recordType,
            value: isDynamicA && !value.trim() ? null : recordType === "TXT" ? value : value.trim(),
            ttl: Number(ttl),
            priority: needsPriority ? Number(priority) : null,
            weight: recordType === "SRV" ? Number(weight) : null,
            port: recordType === "SRV" ? Number(port) : null,
        });
    };

    return (
        <Dialog open={open} onClose={onClose} fullWidth maxWidth="sm">
            <DialogTitle>{record ? `Edit ${record.fqdn}` : "Add record"}</DialogTitle>
            <DialogContent>
                <Stack spacing={2} sx={{ mt: 1 }}>
                    <TextField
                        label="FQDN"
                        placeholder="home.example.com"
                        value={fqdn}
                        onChange={(e) => setFqdn(e.target.value)}
                        disabled={record !== null}
                        helperText={record ? "Name and type cannot change" : undefined}
                        autoFocus
                        fullWidth
                    />
                    <FormControl fullWidth disabled={record !== null}>
                        <InputLabel id="record-type-label">Type</InputLabel>
                        <Select
                            labelId="record-type-label"
                            label="Type"
                            value={recordType}
                            onChange={(e) => setRecordType(e.target.value as RecordType)}
                        >
                            {TYPES.map((type) => (
                                <MenuItem key={type} value={type}>
                                    {type}
                                </MenuItem>
                            ))}
                        </Select>
                    </FormControl>
                    <TextField
                        label={valueLabel(recordType)}
                        placeholder={PLACEHOLDERS[recordType]}
                        value={value}
                        onChange={(e) => setValue(e.target.value)}
                        required={!isDynamicA}
                        fullWidth
                    />
                    {needsPriority && (
                        <TextField
                            label="Priority"
                            type="number"
                            value={priority}
                            onChange={(e) => setPriority(e.target.value)}
                            slotProps={{ htmlInput: { min: 0, max: 65535 } }}
                            required={needsPriority}
                            fullWidth
                        />
                    )}
                    {needsSrvData && (
                        <>
                            <TextField
                                label="Weight"
                                type="number"
                                value={weight}
                                onChange={(e) => setWeight(e.target.value)}
                                slotProps={{ htmlInput: { min: 0, max: 65535 } }}
                                fullWidth
                            />
                            <TextField
                                label="Port"
                                type="number"
                                value={port}
                                onChange={(e) => setPort(e.target.value)}
                                slotProps={{ htmlInput: { min: 0, max: 65535 } }}
                                fullWidth
                            />
                        </>
                    )}
                    <TextField
                        label="TTL (seconds)"
                        type="number"
                        value={ttl}
                        onChange={(e) => setTtl(e.target.value)}
                        helperText="1 = automatic; otherwise 60 to 86400 seconds"
                        slotProps={{ htmlInput: { min: 1, max: 86400 } }}
                        fullWidth
                    />
                    {save.isError && <Alert severity="error">{save.error.message}</Alert>}
                </Stack>
            </DialogContent>
            <DialogActions>
                <Button onClick={onClose}>Cancel</Button>
                <Button
                    variant="contained"
                    onClick={submit}
                    disabled={
                        save.isPending ||
                        !fqdn.trim() ||
                        (needsPriority && !priority) ||
                        (needsSrvData && (!weight || !port))
                    }
                >
                    Save
                </Button>
            </DialogActions>
        </Dialog>
    );
}

function valueLabel(type: RecordType): string {
    switch (type) {
        case "A":
            return "IPv4 address (optional)";
        case "AAAA":
            return "IPv6 address";
        case "CAA":
            return "CAA value (flags tag value)";
        case "TXT":
            return "Text value";
        default:
            return "Target";
    }
}

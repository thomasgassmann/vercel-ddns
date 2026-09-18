import Alert from "@mui/material/Alert";
import Button from "@mui/material/Button";
import Dialog from "@mui/material/Dialog";
import DialogActions from "@mui/material/DialogActions";
import DialogContent from "@mui/material/DialogContent";
import DialogContentText from "@mui/material/DialogContentText";
import DialogTitle from "@mui/material/DialogTitle";
import type { DnsRecord } from "../api";

interface Props {
    record: DnsRecord | null;
    pending: boolean;
    error: string | null;
    onCancel: () => void;
    onConfirm: () => void;
}

export default function ConfirmDelete({ record, pending, error, onCancel, onConfirm }: Props) {
    return (
        <Dialog open={record !== null} onClose={onCancel}>
            <DialogTitle>Delete {record?.fqdn}?</DialogTitle>
            <DialogContent>
                <DialogContentText>
                    This removes the record from ddnser and from Cloudflare.
                </DialogContentText>
                {error && (
                    <Alert severity="error" sx={{ mt: 1 }}>
                        {error}
                    </Alert>
                )}
            </DialogContent>
            <DialogActions>
                <Button onClick={onCancel}>Cancel</Button>
                <Button color="error" variant="contained" onClick={onConfirm} disabled={pending}>
                    Delete
                </Button>
            </DialogActions>
        </Dialog>
    );
}

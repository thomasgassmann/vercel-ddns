import Alert from "@mui/material/Alert";
import Button from "@mui/material/Button";
import Dialog from "@mui/material/Dialog";
import DialogActions from "@mui/material/DialogActions";
import DialogContent from "@mui/material/DialogContent";
import DialogContentText from "@mui/material/DialogContentText";
import DialogTitle from "@mui/material/DialogTitle";
import type { Entry } from "../api";

interface Props {
    entry: Entry | null;
    pending: boolean;
    error: string | null;
    onCancel: () => void;
    onConfirm: () => void;
}

export default function ConfirmDelete({ entry, pending, error, onCancel, onConfirm }: Props) {
    return (
        <Dialog open={entry !== null} onClose={onCancel}>
            <DialogTitle>Delete {entry?.fqdn}?</DialogTitle>
            <DialogContent>
                <DialogContentText>
                    This removes the entry and deletes its DNS records at Vercel.
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

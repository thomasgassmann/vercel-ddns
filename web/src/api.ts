import { queryOptions } from "@tanstack/react-query";

export type RecordType = "A" | "AAAA" | "CAA" | "CNAME" | "MX" | "TXT" | "SRV";

export interface DnsRecord {
    id: number;
    fqdn: string;
    record_type: RecordType;
    value: string | null;
    ttl: number;
    priority: number | null;
    weight: number | null;
    port: number | null;
    created_at: string;
    updated_at: string;
}

export interface RecordInput {
    fqdn: string;
    record_type: RecordType;
    value: string | null;
    ttl: number;
    priority: number | null;
    weight: number | null;
    port: number | null;
}

export interface SyncOutcome {
    source: string;
    started_at: string;
    finished_at: string;
    ipv4: string | null;
    ipv6: string | null;
    created: number;
    updated: number;
    unchanged: number;
    failed: number;
    error: string | null;
}

export interface Status {
    last_sync: SyncOutcome | null;
    sync_interval_secs: number;
}

export interface Session {
    sub: string;
    email: string | null;
}

export class Unauthenticated extends Error {}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
    const res = await fetch(path, init);
    if (res.status === 401) {
        throw new Unauthenticated();
    }

    if (!res.ok) {
        throw new Error(`${init?.method ?? "GET"} ${path}: ${res.status} ${await res.text()}`);
    }

    if (res.status === 204) {
        return undefined as T;
    }

    return res.json();
}

const json = (method: string, body?: unknown): RequestInit => ({
    method,
    headers: body === undefined ? undefined : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
});

export const api = {
    me: () => request<Session>("/api/me"),
    logout: () => request<void>("/auth/logout", { method: "POST" }),

    records: () => request<DnsRecord[]>("/api/records"),
    createRecord: (input: RecordInput) => request<DnsRecord>("/api/records", json("POST", input)),
    updateRecord: (id: number, input: RecordInput) =>
        request<DnsRecord>(`/api/records/${id}`, json("PUT", input)),
    deleteRecord: (id: number) => request<void>(`/api/records/${id}`, json("DELETE")),

    status: () => request<Status>("/api/status"),
    syncNow: () => request<SyncOutcome>("/api/sync", json("POST")),
};

export const meQuery = queryOptions({
    queryKey: ["me"],
    queryFn: api.me,
    staleTime: Infinity,
    retry: false,
});

export const recordsQuery = queryOptions({ queryKey: ["records"], queryFn: api.records });

export const statusQuery = queryOptions({
    queryKey: ["status"],
    queryFn: api.status,
    refetchInterval: 5000,
});

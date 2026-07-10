import { queryOptions } from "@tanstack/react-query";

export interface Entry {
    id: number;
    fqdn: string;
    ipv4: boolean;
    ipv6: boolean;
    ttl: number;
    ipv6_override: string | null;
    last_synced_ipv4: string | null;
    last_synced_ipv4_at: string | null;
    last_synced_ipv6: string | null;
    last_synced_ipv6_at: string | null;
    created_at: string;
    updated_at: string;
}

export interface EntryInput {
    fqdn: string;
    ipv4: boolean;
    ipv6: boolean;
    ttl: number;
    ipv6_override: string | null;
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

    entries: () => request<Entry[]>("/api/entries"),
    createEntry: (input: EntryInput) => request<Entry>("/api/entries", json("POST", input)),
    updateEntry: (id: number, input: EntryInput) =>
        request<Entry>(`/api/entries/${id}`, json("PUT", input)),
    deleteEntry: (id: number) => request<void>(`/api/entries/${id}`, json("DELETE")),

    status: () => request<Status>("/api/status"),
    syncNow: () => request<SyncOutcome>("/api/sync", json("POST")),
};

export const meQuery = queryOptions({
    queryKey: ["me"],
    queryFn: api.me,
    staleTime: Infinity,
    retry: false,
});

export const entriesQuery = queryOptions({ queryKey: ["entries"], queryFn: api.entries });

export const statusQuery = queryOptions({
    queryKey: ["status"],
    queryFn: api.status,
    refetchInterval: 5000,
});

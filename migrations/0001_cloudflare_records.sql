CREATE TABLE records (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    fqdn TEXT NOT NULL,
    record_type TEXT NOT NULL,
    value TEXT,
    ttl BIGINT NOT NULL DEFAULT 3600 CHECK (ttl = 1 OR ttl BETWEEN 60 AND 86400),
    priority INTEGER,
    weight INTEGER,
    port INTEGER,
    zone_id TEXT,
    provider_record_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE NULLS NOT DISTINCT (fqdn, record_type, value),
    UNIQUE (zone_id, provider_record_id),
    CHECK (value IS NOT NULL OR record_type = 'A'),
    CHECK (
        (record_type IN ('MX', 'SRV') AND priority IS NOT NULL AND priority BETWEEN 0 AND 65535)
        OR (record_type NOT IN ('MX', 'SRV') AND priority IS NULL)
    ),
    CHECK (
        (record_type = 'SRV' AND weight IS NOT NULL AND port IS NOT NULL
         AND weight BETWEEN 0 AND 65535 AND port BETWEEN 0 AND 65535)
        OR (record_type <> 'SRV' AND weight IS NULL AND port IS NULL)
    )
);

CREATE TABLE sync_runs (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ NOT NULL,
    ipv4 TEXT,
    created INTEGER NOT NULL,
    updated INTEGER NOT NULL,
    unchanged INTEGER NOT NULL,
    failed INTEGER NOT NULL,
    error TEXT
);

CREATE INDEX sync_runs_finished_at_idx ON sync_runs (finished_at DESC);

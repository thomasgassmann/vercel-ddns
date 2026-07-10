CREATE TABLE entries (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    fqdn TEXT NOT NULL UNIQUE,
    -- Which address families to manage for this name (A and/or AAAA record).
    ipv4 BOOLEAN NOT NULL DEFAULT TRUE,
    ipv6 BOOLEAN NOT NULL DEFAULT FALSE,
    ttl BIGINT NOT NULL DEFAULT 3600 CHECK (ttl >= 60),
    -- Static AAAA target for hosts other than the one running ddnser.
    -- NULL means "this host's own public IPv6". Only meaningful when ipv6.
    ipv6_override TEXT,
    last_synced_ipv4 TEXT,
    last_synced_ipv4_at TIMESTAMPTZ,
    last_synced_ipv6 TEXT,
    last_synced_ipv6_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (ipv4 OR ipv6)
);

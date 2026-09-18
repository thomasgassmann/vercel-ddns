# ddnser

Dynamic DNS and DNS record manager (pronounced _denser_) for domains whose DNS is
managed by [Cloudflare](https://developers.cloudflare.com/api/resources/dns/):

- **Router webhook** - a dyndns2-compatible `GET /nic/update` endpoint your router
  calls whenever its public IP changes (tested against the Zyxel EX5301 "DNS user
  defined" mode).
- **Periodic sync** - a full reconciliation every `DDNSER_SYNC_INTERVAL` seconds
  (default hourly), so records heal even if the webhook never fires.
- **Web UI** - an OIDC-protected admin interface (React + MUI) to manage the DNS
  records stored in Postgres: A, AAAA, CAA, CNAME, MX, TXT and SRV, see sync status
  and trigger a manual sync.
- **Metrics** - Prometheus metrics on a separate listener at `/metrics`. Do not
  expose this listener through an ingress; allow only Prometheus with a network policy.
- **Record types** - A/AAAA can resolve dynamically (router IPv4 via multi-resolver
  consensus, host IPv6 from the kernel), everything else has a fixed value.

## Reconciliation

ddnser manages only records configured in its database. It does not import or
remove unrelated Cloudflare records. When a configured record is first pushed,
ddnser reuses an identical Cloudflare record automatically; otherwise it creates
one. It stores the returned record ID so later edits and deletion address that
exact Cloudflare record.

Static records are pushed when created or edited. Timer and router-webhook syncs
reconcile only dynamic A and AAAA records.

## How dynamic records are resolved

- **A records** point at the router's public IPv4: the `myip` parameter from the
  webhook when the router provides a valid public address, otherwise eight
  independent resolvers are queried (DNS: OpenDNS, Google DNS, Akamai; HTTP:
  ipify, whatismyipaddress, icanhazip, ident.me, AWS) and the answer with the
  most votes wins, requiring at least two agreeing resolvers. These lookups
  currently include plaintext protocols and are not safe against an on-path attacker.
- **AAAA records** resolve to the global IPv6 of the host running ddnser (derived
  from the kernel's source-address selection, falling back to public-ip).

The zone for each FQDN is resolved automatically against the zones on the
Cloudflare account. Sync only upserts owned records.

## Router setup (Zyxel "DNS user defined")

| Field               | Value                                                 |
| ------------------- | ----------------------------------------------------- |
| Service Provider    | DNS user defined                                      |
| Connection Type     | HTTPS through a TLS-terminating proxy                 |
| URL Update          | `https://<ddnser-host>/nic/update`                    |
| Host Name           | ignored                                               |
| Username / Password | `DDNSER_WEBHOOK_USERNAME` / `DDNSER_WEBHOOK_PASSWORD` |

The endpoint replies `good <ip>` / `nochg <ip>` (dyndns2), so the router's status
page shows the authentication result and last update.

Observed EX5301 behavior (firmware tested 2026-07):

- **No explicit port in the URL** - `host:8080/...` silently never connects
  (the router pings the host, then gives up). ddnser must be reachable on
  port 80 (HTTP) or 443 (HTTPS behind the proxy).
- It authenticates with HTTP Basic auth and sends the Host Name field as
  `hostname=`, but **no `myip` parameter** - so IPv4 detection always uses the
  multi-resolver lookup, which is fine.
- The router only inspects the HTTP status code, not the dyndns2 body, so
  ddnser pairs the body with an honest code: 200 `good`/`nochg`, 401 `badauth`,
  500 `911`. "Accepted" in the router UI therefore really means a successful
  sync; check the ddnser logs to tell auth failures from sync failures.
- "Current Dynamic IP" in the router UI is just a DNS resolution of the Host
  Name field.

## Configuration

| Environment variable        | Default                 | Purpose                                            |
| --------------------------- | ----------------------- | -------------------------------------------------- |
| `DDNSER_PORT`               | `8080`                  | HTTP listen port                                   |
| `DDNSER_METRICS_BIND`       | `0.0.0.0`               | Prometheus listener bind address                   |
| `DDNSER_METRICS_PORT`       | `9091`                  | Prometheus listener port                           |
| `DDNSER_LOG_LEVEL`          | `info`                  | Log filter, e.g. `ddnser=debug`                    |
| `DDNSER_DATABASE_URL`       | -                       | Postgres connection string                         |
| `DDNSER_CLOUDFLARE_TOKEN`   | -                       | Cloudflare API token with DNS edit permission      |
| `DDNSER_WEBHOOK_USERNAME`   | -                       | Basic auth user for `/nic/update`                  |
| `DDNSER_WEBHOOK_PASSWORD`   | -                       | Basic auth password for `/nic/update`              |
| `DDNSER_SYNC_INTERVAL`      | `3600`                  | Full-sync interval in seconds                      |
| `DDNSER_OIDC_ISSUER`        | -                       | OIDC issuer URL                                    |
| `DDNSER_OIDC_CLIENT_ID`     | -                       | OIDC client ID                                     |
| `DDNSER_OIDC_CLIENT_SECRET` | -                       | OIDC client secret                                 |
| `DDNSER_PUBLIC_URL`         | `http://localhost:8080` | Externally reachable base URL (OIDC redirect)      |
| `DDNSER_ADMIN_CLAIM`        | `groups`                | ID-token claim checked for admin authorization     |
| `DDNSER_ADMIN_GROUP`        | unset                   | Required group; unset = any authenticated identity |
| `DDNSER_SESSION_SECRET`     | -                       | Session cookie encryption key (32+ chars)          |
| `DDNSER_DEV_FORWARD`        | -                       | Vite dev server URL for local frontend development |

## Development

The schema starts with `0001_cloudflare_records.sql` and requires a fresh database.
Existing databases from the Vercel version are not migration-compatible.

```sh
./dev.sh frontend       # postgres+dex in docker, vite HMR, app via cargo
./dev.sh backend        # postgres+dex in docker, embedded frontend build
./dev.sh test           # postgres+dex in docker, cargo test, then tear down
./dev.sh test -- --test-threads=1  # single-threaded test run
```

`DDNSER_CLOUDFLARE_TOKEN` defaults to an invalid token: the UI, OIDC flow and
webhook all work, sync reports failures until a real token is exported.

## Tests

```sh
cargo test --offline --locked          # unit tests
./dev.sh test                          # all tests, including integration
pnpm --dir web run build               # type-checked frontend build
pnpm --dir web run lint && pnpm --dir web run format:check
```

The integration test covers reuse-or-create reconciliation and verifies that
later edits update the exact Cloudflare record ID rather than creating a second
record.

## Credits

Originally forked from [krosf/vercel-ddns](https://github.com/krosf/vercel-ddns)

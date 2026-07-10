# ddnser

Dynamic DNS daemon (pronounced *denser*) for domains whose DNS is managed by
[Vercel](https://vercel.com/docs/rest-api/reference/endpoints/dns). It keeps A/AAAA
records pointed at your home network:

- **Router webhook** — a dyndns2-compatible `GET /nic/update` endpoint your router
  calls whenever its public IP changes (tested against the Zyxel EX5301 "DNS user
  defined" mode).
- **Periodic sync** — a full reconciliation every `DDNSER_SYNC_INTERVAL` seconds
  (default hourly), so records heal even if the webhook never fires.
- **Web UI** — an OIDC-protected admin interface (React + MUI) to manage the DNS
  entries stored in Postgres, see sync status, and trigger a manual sync.

The apex domain for each FQDN is resolved automatically against the domains on the
Vercel account. Sync only upserts records it manages; deleting an entry in the UI
also deletes the record at Vercel.


```sh
## Development

```sh
./dev.sh frontend   # postgres + dex (mock OIDC) in docker, vite HMR, app via cargo
./dev.sh backend    # same, but serving the embedded production frontend build
```

dev.sh binds port 80 by default so a real router on the LAN can call the
webhook (one sudo prompt to `setcap` the binary after each rebuild); run with
`DDNSER_PORT=8080` to skip that. Open http://localhost and sign in (dex
auto-login test user). Simulate the router with:

```sh
curl -u router:router "http://localhost/nic/update?hostname=ignored&myip=8.8.8.8"
```

## Credits

Originally forked from [krosf/vercel-ddns](https://github.com/krosf/vercel-ddns)
(MIT), a one-shot CLI updater; rewritten as a daemon.

# deploy/prod — PROD host (150.241.68.57)

Everything needed to turn a bare Ubuntu box into the DROP IT PROD host and to
ship releases to it. Server record: [`../servers/prod.md`](../servers/prod.md).
Rollback card: [`ROLLBACK.md`](ROLLBACK.md).

No Docker on this host: two static `x86_64-musl` binaries under systemd,
PostgreSQL and Redis from the distro, nginx serving pre-built web bundles.

## Files

| Path | Role |
|---|---|
| `provision.sh` | orchestrator (runs locally): packages, user, `/srv/dropit`, Postgres, nginx, units, ufw, env file, acceptance checks |
| `deploy.sh` | orchestrator (runs locally): upload `build/` artifacts → install config → psql migrations → restart → health check |
| `lib.sh` | shared shell helpers (ssh/rsync/askpass, secret generation) |
| `host/provision-host.sh` | the on-host provisioning script, fed in over stdin |
| `host/apply-config.sh` | installs nginx site + systemd units; checksum-based, self-rollback on `nginx -t` failure |
| `host/migrate.sh` | psql migration runner with `schema_migrations` bookkeeping |
| `nginx/dropit.conf` | the site: `/` club SPA, `/admin/` admin SPA, `/dj/` console, `/status/` public service status, `/v1/`+`/healthz`+`/readyz` → API |
| `systemd/dropit-api.service` | API unit (`MemoryMax=256M`) |
| `systemd/dropit-workers.service` | workers unit (`MemoryMax=192M`) |
| `systemd/dropit.env.example` | runtime env template, rendered to `/srv/dropit/etc/dropit.env` |
| `.env.example` | template for `.env.local` (host address + secrets) |
| `ROLLBACK.md` | what to do when something breaks |

## First-time setup

```sh
cp deploy/prod/.env.example deploy/prod/.env.local
chmod 600 deploy/prod/.env.local
$EDITOR deploy/prod/.env.local      # set DROPIT_SSH_PASS; secrets self-generate
./deploy/prod/provision.sh
```

`provision.sh` generates any missing secret, writes it back to `.env.local`,
and re-running it is safe — unchanged files are skipped, the host env file is
preserved.

## Deploying a release

```sh
./scripts/build-linux.sh      # -> ./build/  (gitignored)
./deploy/prod/deploy.sh
```

`deploy.sh` reads the host and secrets from `.env.local`, so no arguments are
needed. Flags: `--dry-run`, `--verify-only`, `--skip-migrate`,
`--no-restart`, `--skip-config`, `--build-dir DIR`, `--host`.

For a separate host, keep a separate private env file and select it explicitly
for **both** provisioning and deployment. This leaves the default PROD target
unchanged:

```sh
DROPIT_DEPLOY_ENV="$PWD/deploy/prod/.env.15024168204.local" ./deploy/prod/deploy.sh
```

The test stand at `https://150.241.68.204` is documented in
[`../servers/test-15024168204.md`](../servers/test-15024168204.md).
`host/bootstrap-test-club.py` accepts private account JSON over stdin after
migrations and creates a club, DJ, superadmin with TOTP, and a two-week room QR.
It refuses a server configured with Stripe credentials. Install
`python3-argon2 python3-cryptography` before running it.

For a public IP without a domain, `host/enable-ip-tls.sh <IP>` installs an
isolated current Certbot, requests an IP certificate, enables the optional nginx
TLS listener, and schedules renewal twice daily. The ACME webroot lives outside
the deployed web bundles so `rsync --delete` cannot remove challenges.

## Secrets

Only in `deploy/prod/.env.local` (gitignored, mode 600) and, on the host, in
`/srv/dropit/etc/dropit.env` (mode 600, `dropit:dropit`). Nothing secret is
git-tracked. Rotating: edit `.env.local`, then `./deploy/prod/provision.sh
--sync-env` — this re-renders the host env file *and* re-syncs the Postgres
role password via `ALTER ROLE`.

## Migrations

psql only, via `host/migrate.sh`. Never `DROPIT_RUN_MIGRATIONS=true` here:
`crates/infra-db` bakes the migrations path at compile time, so it panics on
the host. Applied files are tracked in `schema_migrations(filename PK)`.

# PROD — 150.241.68.57

Server record for the DROP IT production host. Single source of truth for
"what is on this box and how do I touch it". Companion to
`deploy/docker-compose.yml` (local stand) and `IMPLEMENTATION_STATUS.md`.

| Field | Value |
|---|---|
| Public IP | `150.241.68.57` |
| Hostname | `node-42989.nodehost.ru` |
| Provider | nodehost.ru (VPS) |
| OS | Ubuntu 26.04 LTS, x86_64 |
| Hardware | 1 vCPU, 956 MB RAM, 9.8 GB disk (~6.5 GB free) |
| Role | **PROD**, currently running **Test Mode** |
| Public surface | HTTP on `:80` only (no TLS yet — see "Known gaps") |
| Stand URL | <http://150.241.68.57/> · status: <http://150.241.68.57/status/> |
| Deployed by | `deploy/prod/deploy.sh` from the repo checkout |

"PROD" here means the owner-designated production host. "Test Mode" means the
external integrations are deliberately switched off — MockStripe, NoopPush,
no NATS — so no real money moves and no notifications are sent. The full
product surface (auth, sessions, QR, requests, payments-mock, payouts,
panels) is live against a real Postgres.

---

## What runs where

| Component | Where | Notes |
|---|---|---|
| `dropit-api` (axum gateway) | systemd `dropit-api.service`, `/srv/dropit/bin/dropit-api` | binds `127.0.0.1:8080` only |
| `dropit-workers` (supervisor) | systemd `dropit-workers.service`, `/srv/dropit/bin/dropit-workers` | no listening port |
| nginx | systemd `nginx.service` | public `:80`, reverse-proxies to the API; serves `/status/` (public page) |
| PostgreSQL 18 | systemd `postgresql.service`, cluster `18/main` | `127.0.0.1:5432`, unix socket `/var/run/postgresql` |
| Redis 8 | systemd `redis-server.service` | `127.0.0.1:6379`, `MemoryMax=96M` |
| chrony | systemd `chrony.service` | clock sync; webhook signature tolerance depends on it |
| ufw | systemd `ufw.service` | 22/80/443 in, everything else denied |

No Docker, no JVM, no Node on the host: the two binaries are static
`x86_64-musl` ELF and nginx serves the pre-built web bundles. That is a
deliberate constraint — 956 MB does not fit a container runtime plus a
JVM/Node toolchain alongside Postgres.

### Ports

| Port | Bound to | What |
|---|---|---|
| 22/tcp | 0.0.0.0 | sshd (password auth as root; config untouched) |
| 80/tcp | 0.0.0.0 | nginx — the only public entry point |
| 443/tcp | — | allowed in ufw, nothing listening yet (no TLS) |
| 8080/tcp | 127.0.0.1 | dropit-api (nginx upstream) |
| 5432/tcp | 127.0.0.1 | PostgreSQL |
| 6379/tcp | 127.0.0.1 | Redis |

`ss -tlnp` on the host is the authoritative check.

---

## Filesystem layout

```
/srv/dropit/
├── bin/                    dropit-api, dropit-workers   (root:dropit 755)
├── web/
│   ├── club/               club SPA  — Vite base '/'    -> nginx /
│   │                       (also owns /dj, /dj/login, /dj/invite/:token,
│   │                        /connect, /login, /register — client-side routes)
│   ├── admin/              admin SPA — Vite base '/admin/' -> nginx /admin/
│   ├── dj-mock/index.html  static DJ console PROTOTYPE  -> nginx /dj-mock/
│   └── status/             public service-status page   -> nginx /status/
│                           (design mock, not the product surface)
├── etc/dropit.env          runtime env, secrets, 0600 dropit:dropit
└── data/
    ├── migrations/*.sql    uploaded by deploy.sh; applied by migrate.sh
    └── backup/             reserved for pg_dump output
```

Supporting files outside the tree:

| Path | What |
|---|---|
| `/etc/nginx/sites-available/dropit.conf` | nginx site (symlinked into `sites-enabled/`) |
| `/etc/systemd/system/dropit-api.service` | API unit |
| `/etc/systemd/system/dropit-workers.service` | workers unit |
| `/etc/postgresql/18/main/conf.d/99-dropit.conf` | Postgres tuning |
| `/etc/postgresql/18/main/pg_hba.conf` | distro default: local peer, `127.0.0.1` scram |
| `/etc/sysctl.d/99-dropit-memory.conf` | `vm.swappiness=20` |
| `/swapfile` | 1 GiB swap (in `/etc/fstab`) |
| `/etc/systemd/system/redis-server.service.d/dropit-memory.conf` | `MemoryMax=96M` |
| `/root/dropit-deploy/` | config bundle uploaded by provision.sh/deploy.sh |
| `/var/log/nginx/dropit.{access,error}.log` | nginx logs |

Nothing under `/srv/dropit` is git-tracked: the repo only holds the scripts and
config that produce it.

---

## Connecting

SSH is password auth as **root** on port 22 (no key installed, `sshd_config`
left exactly as the provider shipped it).

The root password lives in **`deploy/prod/.env.local`** as `DROPIT_SSH_PASS`
(that file is gitignored; see `.gitignore`). It is never written into this
document, into any other git-tracked file, or onto the host.

Because `sshpass` is not available everywhere, the deploy scripts use an
askpass helper: the password is exported as `DROPIT_SSH_PASS` and a small
helper script answers ssh's prompt, so the password never appears in argv.

```sh
# one-off shell on the box
cd <repo>
set -a; . deploy/prod/.env.local; set +a
SSH_ASKPASS="${TMPDIR:-/tmp}/dsh-ssh/askpass.sh" SSH_ASKPASS_REQUIRE=force \
  ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null root@150.241.68.57
```

Day-to-day operations go through the scripts, not raw ssh — they are
idempotent and encode the change detection:

```sh
./deploy/prod/provision.sh --verify-only   # re-run acceptance checks
./deploy/prod/deploy.sh --verify-only      # health checks against the live host
```

### Secrets

| Secret | Lives in | Notes |
|---|---|---|
| root SSH password | `deploy/prod/.env.local` → `DROPIT_SSH_PASS` | gitignored, `chmod 600` |
| Postgres role password | `deploy/prod/.env.local` → `DROPIT_DB_PASSWORD`, and `/srv/dropit/etc/dropit.env` | synced by `provision.sh` via `ALTER ROLE`, so the two cannot drift |
| `DROPIT_JWT_SECRET` | `deploy/prod/.env.local`, `/srv/dropit/etc/dropit.env` | 48 random bytes hex |
| `DROPIT_TOTP_KEY` | `deploy/prod/.env.local`, `/srv/dropit/etc/dropit.env` | 32 bytes hex = AES-256-GCM key |
| Stripe / FCM / APNs | intentionally empty | Test Mode: MockStripe + NoopPush |
| `DROPIT_OPEN_SIGNUP` | `/srv/dropit/etc/dropit.env` | `1` on this stand — open club/DJ registration, no e-mail verification |
| `DROPIT_SIGNUP_RATE_PER_HOUR` | `/srv/dropit/etc/dropit.env` | `100` on this stand (code default 10); per-IP successful registrations per hour |

`provision.sh` generates any missing value once and writes it back to
`deploy/prod/.env.local`, so rotating a secret is: edit `.env.local`, then
`./deploy/prod/provision.sh --sync-env` (re-renders the host env file and
re-syncs the Postgres password).

The host env file is the source of truth at runtime. `deploy.sh` deliberately
does **not** overwrite it, so a redeploy cannot rotate secrets by accident.

### Postgres DSN

```
postgres://dropit:<password>@127.0.0.1:5432/dropit
```

- role `dropit`, database `dropit`, `127.0.0.1:5432`
- the password is `DROPIT_DB_PASSWORD` in `deploy/prod/.env.local`, and the
  same value is inlined in `DATABASE_URL` inside `/srv/dropit/etc/dropit.env`
  (mode `0600 dropit:dropit`); it is not reproduced here
- auth: `scram-sha-256` over TCP, peer over the unix socket
- extensions `pgcrypto` and `citext` are created by `provision.sh` as
  superuser, because the migrations run as the unprivileged `dropit` role

---

## Build

The Lead cross-compiles on macOS and hands the result over as a local
`build/` directory (gitignored). The exact contract `deploy.sh` consumes:

```
build/dropit-api            static x86_64-musl binary
build/dropit-workers        static x86_64-musl binary
build/web/club/…            Vite dist of apps/web-club   (base '/')
build/web/admin/…           Vite dist of apps/web-admin  (base '/admin/')
build/web/dj/index.html     static DJ console prototype -> /srv/dropit/web/dj-mock/
build/migrations/*.sql      crates/infra-db/migrations
```

`scripts/build-linux.sh` produces exactly that layout:

```sh
./scripts/build-linux.sh            # binaries + web + migrations -> ./build/
./scripts/build-linux.sh --bin-only # binaries + migrations only
./scripts/build-linux.sh --web-only # web bundles only
./deploy/prod/deploy.sh --dry-run   # sanity-check the artifact set
```

What it does (and why it is not a plain `cargo build`):

- target `x86_64-unknown-linux-musl`, linked with **zig** via `cargo-zigbuild`
  (zig ships a musl libc and compiles the C parts, e.g. `ring`);
- the target and toolchain live in a separate writable `RUSTUP_HOME`/`HOME`
  (`$DROPIT_TOOLCHAIN_DIR`, default `/Volumes/Хранилище/Dev/dropit_ascii`),
  because the agent sandbox mounts `~/.rustup` read-only;
- `LTO=false`, `codegen-units=16` for build speed on a stand;
- it also emits `build/seed_demo` (demo club/DJ/admin + tracks) and copies
  `crates/infra-db/migrations/*.sql` into `build/migrations/`.

`seed_demo` is not deployed by `deploy.sh` — run it manually once against the
host database when demo data is wanted (see the README's Test Mode section).

`deploy.sh` refuses a Mach-O binary in `build/`, so a forgotten
`--target x86_64-unknown-linux-musl` fails loudly before it reaches the host.

---

## Deploying

```sh
./deploy/prod/deploy.sh              # the normal path
```

No arguments needed: host and secrets come from `deploy/prod/.env.local`.
The sequence is: preflight (artifact completeness + ELF check) → upload
binaries to staging names → swap in → rsync web bundles → install
nginx/systemd config → apply migrations with psql → restart → health check.

Useful flags: `--dry-run`, `--verify-only`, `--skip-migrate`,
`--no-restart`, `--build-dir DIR`.

It is idempotent: config files are compared by sha256 and nginx/systemd are
only reloaded when something actually changed; already-applied migrations are
skipped by `schema_migrations`. Re-running with an unchanged build restarts
the services but changes nothing else.

### Migrations — psql only, never in-process

`crates/infra-db/src/migrations.rs` resolves its directory with
`env!("CARGO_MANIFEST_DIR")`, baked in **at compile time**. On the host that
path does not exist, so `DROPIT_RUN_MIGRATIONS=true` would panic instead of
migrating. It is therefore pinned to `false` in `/srv/dropit/etc/dropit.env`,
and migrations are applied by `deploy/prod/host/migrate.sh`:

- one transaction per file, `ON_ERROR_STOP=1`, filename order (`LC_ALL=C`);
- applied files recorded in `schema_migrations(filename PK, checksum, applied_at, source)`;
- re-runs skip already-applied files — no duplicate application;
- a changed checksum on an applied file is reported as **drift** (a warning;
  `--strict-drift` makes it fatal). Never edit an applied migration — add a
  new one;
- connects as the `dropit` role over the unix socket (peer auth), so no
  password is passed on any command line.

Inspect without changing anything:

```sh
ssh root@150.241.68.57 'bash /root/dropit-deploy/host/migrate.sh --status'
ssh root@150.241.68.57 'bash /root/dropit-deploy/host/migrate.sh --dry-run'
```

---

## Health checks

On the host (loopback — immune to workstation DNS/firewall):

| Check | Expected |
|---|---|
| `curl -sI http://127.0.0.1:8080/healthz` | `200` |
| `curl -s http://127.0.0.1:8080/readyz` | `200`, `{"status":"ok","version":…}` |
| `curl -sI http://127.0.0.1/healthz` | `200` (same endpoint through nginx) |
| `curl -sI http://127.0.0.1/` | `200` (club SPA) |
| `curl -sI http://127.0.0.1/admin/` | `200` (admin SPA) |
| `curl -sI http://127.0.0.1/dj` | `200` (club SPA shell → client router) |
| `curl -sI http://127.0.0.1/dj/login` | `200` (club SPA shell → DJ login route) |
| `curl -sI http://127.0.0.1/dj-mock/` | `200` (static prototype) |
| `systemctl is-active dropit-api dropit-workers` | `active` |

From outside, same paths on the same origin:

| URL | What |
|---|---|
| <http://150.241.68.57/> | landing (Connect) — CTAs lead to `/register` |
| <http://150.241.68.57/register> | **open registration** — club or DJ in one click (Test Mode) |
| <http://150.241.68.57/login> | club login |
| <http://150.241.68.57/dj> | **live DJ console** — route inside the club SPA, role-gated |
| <http://150.241.68.57/dj/login> | DJ login (club SPA route) |
| <http://150.241.68.57/dj/invite/:token> | DJ invite acceptance (club SPA route) |
| <http://150.241.68.57/admin/> | admin panel |
| <http://150.241.68.57/dj-mock/> | static DJ console **prototype** — design reference only |
| <http://150.241.68.57/healthz> | API liveness (through nginx) |
| <http://150.241.68.57/readyz> | API readiness + version (through nginx) |

`deploy.sh --verify-only` runs the whole matrix.

**Routing note.** `/dj`, `/dj/login` and `/dj/invite/:token` are *not* nginx
locations — they must fall through to `location /` so the club SPA shell is
returned and React Router resolves them client-side. A `location /dj/` block
would shadow the real DJ panel with the static prototype (this was a real bug,
fixed). The prototype therefore lives at `/dj-mock/`.

---

## Operations

```sh
# status
systemctl status dropit-api dropit-workers --no-pager
# logs
journalctl -u dropit-api -f
journalctl -u dropit-workers -f -n 100
# nginx
nginx -t && systemctl reload nginx
tail -f /var/log/nginx/dropit.error.log
# postgres
pg_lsclusters
runuser -u postgres -- psql -d dropit -c 'select count(*) from request'
# disk / memory
df -h /; free -m; swapon --show
```

### Restart

```sh
systemctl restart dropit-api dropit-workers
```

Postgres or nginx: `systemctl restart postgresql` / `systemctl restart nginx`.
The units are `Restart=always`, so a crashed process comes back on its own.

### Rollback

There is no artifact history on the host (no releases directory), so a
rollback is "deploy the previous build again":

1. Rebuild or check out the previous revision and re-run the build step:
   `./scripts/build-linux.sh` → `build/`.
2. `./deploy/prod/deploy.sh --skip-migrate` — this skips the migration step,
   which is what you want when rolling **back** past a migration. Migrations
   follow expand→contract (`tehnicheskaya-karta.md`), so the older binary is
   expected to run against the newer schema; if a migration was not
   expand-only, the rollback needs a forward-fix instead.
3. Verify: `./deploy/prod/deploy.sh --verify-only`.

For a config-only problem (nginx/units), the previous file is kept by
`apply-config.sh` at `/root/dropit-deploy/backup/*.bak`; restore it and
`nginx -t && systemctl reload nginx`.

To stop the stand without touching data:
`systemctl stop dropit-api dropit-workers`.

### Backups

Not automated yet — this is a Test Mode stand with synthetic data. Before it
carries anything worth keeping, add a nightly
`pg_dump -Fc dropit > /srv/dropit/data/backup/dropit-$(date +%F).dump` cron
plus an off-host copy, and run a restore drill (a backup that has never been
restored is not a backup).

---

## Resource budget (956 MB)

| Consumer | Limit |
|---|---|
| PostgreSQL | `shared_buffers=128MB`, `max_connections=40`, `work_mem=3MB` |
| `dropit-api` | `MemoryHigh=208M`, `MemoryMax=256M` |
| `dropit-workers` | `MemoryHigh=160M`, `MemoryMax=192M` |
| Redis | `MemoryMax=96M` |
| nginx | unbounded, but small (static files + proxy) |
| swap | 1 GiB `/swapfile`, `vm.swappiness=20` |

`max_connections=40` is deliberate headroom over the API pool + workers +
occasional `psql`; `max_parallel_workers_per_gather=1` because a single vCPU
makes parallel query a pessimisation, not a win.

---

## Known gaps (deliberate, for the record)

- **No TLS.** Port 80 only; `443/tcp` is open in ufw but nothing listens.
  Before this is treated as a real production endpoint, add certbot + HSTS
  and redirect 80→443. Not done here because it needs a domain pointed at the
  IP (the stand is reached by bare IP today).
- **No backups** — see above.
- **No monitoring/alerting.** Prometheus/Grafana/Sentry from
  `tehnicheskaya-karta.md` are not deployed; health is checked manually via
  `deploy.sh --verify-only`.
- **Password-only root SSH.** Provider default, `sshd_config` untouched by
  request. Installing a key and disabling password auth is the first hardening
  step once the owner's access model is settled.
- **No NATS**, so the outbox relay is a no-op and events accumulate in the
  outbox table. Redis *is* present, so WS tickets work across a restart.
- **Single node.** No HA, no replica; Redis is loopback-only and the API is a
  single process, so an in-memory fallback would be equivalent here anyway.

---

## Demo data (seeded on this host)

`seed_demo` was run against the host database, so the stand has working
accounts out of the box:

| Role | Login | Password |
|---|---|---|
| Club | `demo@club.test` | `demo-club-pass` |
| DJ | `demo@dj.test` | `demo-dj-pass` |
| Admin | `demo@admin.test` | `demo-admin-pass` + TOTP `JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP` |

Plus 5 catalog tracks. Admin login always asks for the 2FA code — add the
TOTP key to any authenticator app, or generate the current code locally:

```sh
python3 - <<'PY'
import base64, hashlib, hmac, struct, time
key = base64.b32decode('JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP')
h = hmac.new(key, struct.pack('>Q', int(time.time()) // 30), hashlib.sha1).digest()
o = h[-1] & 0x0F
print(str((struct.unpack('>I', h[o:o+4])[0] & 0x7FFFFFFF) % 10**6).zfill(6))
PY
```

Re-seeding is idempotent (`ON CONFLICT`), run it the same way it was run:

```sh
scp build/seed_demo root@150.241.68.57:/srv/dropit/bin/
ssh root@150.241.68.57 'chmod 755 /srv/dropit/bin/seed_demo &&
  set -a && . /srv/dropit/etc/dropit.env && set +a && /srv/dropit/bin/seed_demo'
```

Registration is open on top of that: anybody can create a fresh club or DJ
account at <http://150.241.68.57/register> (one click, no e-mail), which is
the point of the Test Mode stand. All Test Mode registrations land with
`club.status='active'` and burn no real money — Stripe is the mock provider.

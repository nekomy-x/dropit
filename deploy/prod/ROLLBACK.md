# DROP IT PROD — rollback notes

Short operational card. Full context in `deploy/servers/prod.md`.

## What can go wrong, and the fastest way back

### Bad application release (API/workers crash or misbehave)

The host keeps no artifact history, so "rollback" means redeploying the
previous build. Migrations follow expand→contract, so the older binary is
expected to run against the newer schema.

```sh
# 1. rebuild the previous revision
git checkout <previous-rev>
./scripts/build-linux.sh

# 2. deploy WITHOUT running migrations (do not roll the schema backwards)
./deploy/prod/deploy.sh --skip-migrate

# 3. verify
./deploy/prod/deploy.sh --verify-only
```

If the previous release is already in `build/` and unchanged, step 1 can be
skipped — `deploy.sh` is idempotent and will simply re-promote the same
binaries.

### Bad nginx or systemd config

`apply-config.sh` backs up every file it replaces to
`/root/dropit-deploy/backup/`, and it *refuses* to leave a config that fails
`nginx -t`: the previous file is restored and nginx reloaded automatically.
So this failure mode is self-healing; if it still needs a manual hand:

```sh
ssh root@150.241.68.57
cp /root/dropit-deploy/backup/dropit.conf.bak /etc/nginx/sites-available/dropit.conf
nginx -t && systemctl reload nginx
```

Same for units:

```sh
cp /root/dropit-deploy/backup/dropit-api.service.bak /etc/systemd/system/dropit-api.service
systemctl daemon-reload && systemctl restart dropit-api
```

### Bad migration

`migrate.sh` runs one transaction per file with `ON_ERROR_STOP=1`, so a
failing migration rolls back and **stops the run** — later files are not
applied. Nothing is recorded in `schema_migrations` for the failed file, so
the fix is: correct the SQL in a new file, ship it, re-run `deploy.sh`.

Never edit an already-applied migration: `migrate.sh` reports it as checksum
drift. Add a new migration instead.

If a migration must be undone manually (last resort):

```sh
ssh root@150.241.68.57
runuser -u dropit -- psql -d dropit -c '\dt'          # inspect
runuser -u dropit -- psql -d dropit -c \
  "DELETE FROM schema_migrations WHERE filename = '0011_open_signup.sql'"
# ...then apply the compensating SQL by hand, in a transaction, with a backup.
```

Take a dump first: `pg_dump -Fc dropit > /srv/dropit/data/backup/pre-rollback.dump`.

### Postgres will not start after a tuning change

```sh
pg_lsclusters
tail -50 /var/log/postgresql/postgresql-18-main.log
# remove the generated tuning and restart on defaults
rm /etc/postgresql/18/main/conf.d/99-dropit.conf
systemctl restart postgresql
```

### Locked out / firewall mistake

ufw always allows 22/tcp **before** being enabled, and the rule is added on
every run, so this should not happen. If it does, use the provider's VNC/rescue
console (nodehost.ru panel), then:

```sh
ufw allow 22/tcp && ufw reload
```

### Everything is on fire

```sh
systemctl stop dropit-api dropit-workers   # stop serving, keep data
# or, for a full stop of the web tier:
systemctl stop nginx
```

Data lives in PostgreSQL only; stopping the services never touches it.

## Restart cheat sheet

| Goal | Command |
|---|---|
| restart app tier | `systemctl restart dropit-api dropit-workers` |
| restart db | `systemctl restart postgresql` |
| restart proxy | `nginx -t && systemctl reload nginx` |
| tail api logs | `journalctl -u dropit-api -f` |
| tail worker logs | `journalctl -u dropit-workers -f` |
| check health | `./deploy/prod/deploy.sh --verify-only` |
| check migrations | `ssh root@… 'bash /root/dropit-deploy/host/migrate.sh --status'` |

# Rollback scripts

One `<version>_<name>.down.sql` per reversible migration in `backend/migrations/`.

These are **manual**. sqlx's migrator only walks files at the top level of
`migrations/`, so this directory is invisible to `sqlx migrate run` and to the
`sqlx::migrate!()` bootstrap in `src/db/mod.rs` — adding a script here cannot
change what a deploy applies.

To roll a migration back, apply its script and then delete the bookkeeping row
so the migrator will re-apply the migration on the next run:

```sh
psql "$DATABASE_URL" -f migrations/rollback/<version>_<name>.down.sql
psql "$DATABASE_URL" -c "DELETE FROM _sqlx_migrations WHERE version = <version>;"
```

Roll back newest-first; the scripts are not written to be applied out of order.

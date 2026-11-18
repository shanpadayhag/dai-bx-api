# Restore a backup

Use this for the restore drill and for a real recovery. Steps 1 to 6 are the same for both. Step 7 is only for a real recovery, and step 8 closes either one.

## Before you start

You need:

- the backup private key and `TOTP_KEYS` / `TOTP_KEY_CURRENT`, from the password manager;
- AWS credentials that can read `daibx-backups-566252491895`;
- `age`, and Postgres client tools at least as new as the version in `.github/workflows/backup.yml`.

Check the date of the last **Nightly backup** run in the repository's Actions tab first. GitHub can switch off scheduled workflows on a quiet public repository, and no failure email comes when that happens.

Work in a private folder, and keep every decrypted file inside it:

```sh
umask 077
restore_dir="$(mktemp -d)"
cd "$restore_dir"
```

## 1. Save the list of accounts deleted since the backup

Do this first in a real recovery if the old database can still be read. Fill in the backup's time in step 3 and come back if you don't know it yet.

```sh
psql "$OLD_DATABASE_URL" --set=ON_ERROR_STOP=1 --command="\copy (SELECT subject_ref, occurred_at FROM security_events WHERE event_type = 'account_deleted' AND occurred_at >= '<started_at>') TO 'deleted-accounts.csv' WITH (FORMAT csv)"
```

If the old database is gone, this list can't be rebuilt. Go through any erasure requests you received since the backup and delete those accounts by hand, then write down what you did in the incident record.

## 2. Download the backup

```sh
aws s3 ls s3://daibx-backups-566252491895/backups/ --region ap-northeast-1
aws s3 cp s3://daibx-backups-566252491895/backups/<name>.tar.age . --region ap-northeast-1
```

## 3. Decrypt and unpack

Paste the private key from the password manager when `age` has nothing else to read it from:

```sh
pbpaste >backup-key.txt
age --decrypt --identity backup-key.txt --output backup.tar <name>.tar.age
rm backup-key.txt
tar --extract --file=backup.tar
cat started_at.txt
```

## 4. Restore into an empty database

For the drill, start a throwaway local cluster and set `RESTORED_DATABASE_URL` to a new database in it. For a real recovery, create a new Supabase project and use its session pooler URL with the `postgres` role.

```sh
psql "$RESTORED_DATABASE_URL" --set=ON_ERROR_STOP=1 --command='CREATE EXTENSION IF NOT EXISTS citext WITH SCHEMA public'
psql "$RESTORED_DATABASE_URL" --set=ON_ERROR_STOP=1 --file=ops/create-database-roles.sql
pg_restore --list daibx.dump | grep -v ' SCHEMA - public ' >restore.list
pg_restore --no-owner --no-privileges --exit-on-error --use-list=restore.list --dbname="$RESTORED_DATABASE_URL" daibx.dump
```

## 5. Check it

Run these from the api repository, with `restore_dir` still set. The first prints nothing when every table's count matches. The second must show every migration as installed.

```sh
psql "$RESTORED_DATABASE_URL" --no-psqlrc --quiet --tuples-only --no-align --field-separator=$'\t' --file=ops/count-rows.sql | diff - "$restore_dir/rows.tsv"
DATABASE_URL="$RESTORED_DATABASE_URL" sqlx migrate info
```

## 6. Erase accounts deleted since the backup

Skip this only when step 1 found no rows. Running it twice is safe.

```sh
psql "$RESTORED_DATABASE_URL" --set=ON_ERROR_STOP=1 --set=deletions_file="$restore_dir/deleted-accounts.csv" --file=ops/reapply-deletions.sql
```

### Drill: sign in to the copy

Write a private env file in `restore_dir`, based on the api's `.env`, with `DATABASE_URL` set to the restored database and the production `TOTP_KEYS` and `TOTP_KEY_CURRENT` from the password manager. Quote every value. Keep the local mail settings, so no real email can go out.

```sh
cargo lambda watch --bin daibx-api --env-file "$restore_dir/restore.env"
```

Start the local web app and sign in with your real password and authenticator code. The drill passes when your own tasks appear. Then stop both and remove the cluster.

## 7. Real recovery only

1. Close the Data API and restore grants:
   ```sh
   psql "$RESTORED_DATABASE_URL" --set=ON_ERROR_STOP=1 --file=ops/restore-access.sql
   ```
2. End every restored session, because sessions revoked after the backup come back active:
   ```sh
   psql "$RESTORED_DATABASE_URL" --set=ON_ERROR_STOP=1 --file=ops/end-all-sessions.sql
   ```
3. Schedule the purge again with `ops/schedule-purge.sql`.
4. Create the backup role with `ops/create-backup-role.sql`, and update `BACKUP_DATABASE_URL`.
5. Set a new `daibx_app` password as in `docs/security/platform-checklist.md`. Point `DATABASE_URL` in the prod Lambdas at the new database as `daibx_app`, and the GitHub `prod` environment at it as the owner role.
6. Tell users what happened. Anything after the backup is lost, and a password or authenticator changed after it is back to its old value, so those people must change it again. Recovery codes used after the backup work again until then.

## 8. Clean up

```sh
cd /
rm -rf "$restore_dir"
```

Record a drill's date and result in `docs/security/platform-checklist.md`. Record a real recovery in the private incident register.

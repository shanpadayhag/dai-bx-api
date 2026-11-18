-- Run once per database as the role that runs migrations, so future tables are covered:
--   psql "$DATABASE_URL" --set=ON_ERROR_STOP=1 --file=ops/create-backup-role.sql
-- Then set its password with \password daibx_backup, which keeps it out of files and history.
--
-- BYPASSRLS is needed because every table has row level security on with no policies,
-- and pg_dump refuses to read a table whose rows would be filtered.

DO $$
BEGIN
    CREATE ROLE daibx_backup LOGIN NOINHERIT BYPASSRLS;
EXCEPTION
    WHEN duplicate_object OR unique_violation THEN
        NULL;
END
$$;

ALTER ROLE daibx_backup SET default_transaction_read_only = on;

GRANT USAGE ON SCHEMA public TO daibx_backup;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO daibx_backup;
GRANT SELECT ON ALL SEQUENCES IN SCHEMA public TO daibx_backup;

ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO daibx_backup;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON SEQUENCES TO daibx_backup;

-- The API and the scheduler connect as this role. Migrations keep the owner role.
-- It has no password here: each environment sets its own (docs/security/platform-checklist.md).
-- Row level security is on with no other policies, so every new table needs a
-- daibx_app policy too; tests/schema/app_database_role.rs fails until it has one.

DO $$
BEGIN
    CREATE ROLE daibx_app LOGIN NOINHERIT;
EXCEPTION
    WHEN duplicate_object OR unique_violation THEN
        NULL;
END
$$;

GRANT USAGE ON SCHEMA public TO daibx_app;

DO $$
DECLARE
    app_table_name text;
BEGIN
    FOR app_table_name IN
        SELECT tablename FROM pg_tables
        WHERE schemaname = 'public'
          AND tablename NOT IN ('_sqlx_migrations', 'security_events')
    LOOP
        EXECUTE format(
            'GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE %I TO daibx_app',
            app_table_name
        );
        EXECUTE format(
            'CREATE POLICY daibx_app_reads_and_writes ON %I FOR ALL TO daibx_app USING (true) WITH CHECK (true)',
            app_table_name
        );
    END LOOP;
END
$$;

GRANT SELECT, INSERT ON TABLE security_events TO daibx_app;

CREATE POLICY daibx_app_reads_security_events ON security_events
    FOR SELECT
    TO daibx_app
    USING (true);

CREATE POLICY daibx_app_adds_security_events ON security_events
    FOR INSERT
    TO daibx_app
    WITH CHECK (true);

ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO daibx_app;

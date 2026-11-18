-- A backup carries no grants, so a real restore into Supabase runs this afterwards.
-- It closes the Data API again and gives the invite operator and the app back their rights.

DO $$
DECLARE
    public_table_name text;
    data_api_role_name text;
    app_table_name text;
BEGIN
    FOR public_table_name IN
        SELECT tablename FROM pg_tables WHERE schemaname = 'public'
    LOOP
        FOREACH data_api_role_name IN ARRAY ARRAY['anon', 'authenticated', 'service_role']
        LOOP
            IF EXISTS (SELECT FROM pg_roles WHERE rolname = data_api_role_name) THEN
                EXECUTE format('REVOKE ALL ON TABLE public.%I FROM %I', public_table_name, data_api_role_name);
            END IF;
        END LOOP;
    END LOOP;

    IF EXISTS (SELECT FROM pg_roles WHERE rolname = 'invite_operator') THEN
        GRANT USAGE ON SCHEMA public TO invite_operator;
        GRANT SELECT, INSERT ON TABLE public.invites TO invite_operator;
    END IF;

    IF EXISTS (SELECT FROM pg_roles WHERE rolname = 'daibx_app') THEN
        GRANT USAGE ON SCHEMA public TO daibx_app;
        FOR app_table_name IN
            SELECT tablename FROM pg_tables
            WHERE schemaname = 'public'
              AND tablename NOT IN ('_sqlx_migrations', 'security_events')
        LOOP
            EXECUTE format(
                'GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE public.%I TO daibx_app',
                app_table_name
            );
        END LOOP;
        GRANT SELECT, INSERT ON TABLE public.security_events TO daibx_app;
        ALTER DEFAULT PRIVILEGES IN SCHEMA public
            GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO daibx_app;
    END IF;
END
$$;

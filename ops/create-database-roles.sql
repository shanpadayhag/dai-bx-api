-- Run before pg_restore on a new database server. The backup's access rules name these
-- roles, and the restore stops if they don't exist. They get no rights here:
-- ops/restore-access.sql grants those after the restore.

DO $$
DECLARE
    database_role_name text;
BEGIN
    FOREACH database_role_name IN ARRAY ARRAY['invite_operator', 'daibx_app']
    LOOP
        BEGIN
            EXECUTE format('CREATE ROLE %I LOGIN NOINHERIT', database_role_name);
        EXCEPTION
            WHEN duplicate_object OR unique_violation THEN
                NULL;
        END;
    END LOOP;
END
$$;

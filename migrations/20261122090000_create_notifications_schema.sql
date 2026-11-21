ALTER TYPE security_event_type ADD VALUE 'push_device_added';
ALTER TYPE security_event_type ADD VALUE 'push_device_removed';

ALTER TABLE users
    ADD COLUMN notify_check_ins boolean NOT NULL DEFAULT true,
    ADD COLUMN notify_schedule_runs boolean NOT NULL DEFAULT true,
    ADD COLUMN notify_blocks_starting boolean NOT NULL DEFAULT true,
    ADD COLUMN notify_quiet_outside_working_hours boolean NOT NULL DEFAULT true;

CREATE TABLE push_devices (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    session_id uuid REFERENCES sessions (id) ON DELETE SET NULL,
    endpoint text NOT NULL UNIQUE CHECK (char_length(endpoint) BETWEEN 9 AND 1000),
    p256dh text NOT NULL CHECK (char_length(p256dh) = 87),
    auth_secret text NOT NULL CHECK (char_length(auth_secret) = 22),
    device_name text NOT NULL CHECK (char_length(device_name) BETWEEN 1 AND 60),
    time_zone text NOT NULL CHECK (char_length(time_zone) BETWEEN 1 AND 64),
    created_at timestamptz NOT NULL DEFAULT now(),
    last_delivered_at timestamptz,
    UNIQUE (id, user_id)
);

CREATE INDEX push_devices_user_id_index ON push_devices (user_id);
CREATE INDEX push_devices_session_id_index ON push_devices (session_id);

CREATE TABLE push_deliveries (
    device_id uuid NOT NULL,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind text NOT NULL CHECK (kind IN ('check_in', 'schedule_ran', 'block_starting')),
    subject_key text NOT NULL CHECK (char_length(subject_key) BETWEEN 1 AND 120),
    due_at timestamptz NOT NULL,
    sent_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (device_id, kind, subject_key, due_at),
    FOREIGN KEY (device_id, user_id) REFERENCES push_devices (id, user_id) ON DELETE CASCADE
);

CREATE INDEX push_deliveries_user_id_index ON push_deliveries (user_id);
CREATE INDEX push_deliveries_sent_at_index ON push_deliveries (sent_at);

DO $$
DECLARE
    push_table_name text;
    data_api_role_name text;
BEGIN
    FOREACH push_table_name IN ARRAY ARRAY['push_devices', 'push_deliveries']
    LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', push_table_name);
        FOREACH data_api_role_name IN ARRAY ARRAY['anon', 'authenticated', 'service_role']
        LOOP
            IF EXISTS (SELECT FROM pg_roles WHERE rolname = data_api_role_name) THEN
                EXECUTE format('REVOKE ALL ON TABLE %I FROM %I', push_table_name, data_api_role_name);
            END IF;
        END LOOP;
        EXECUTE format(
            'CREATE POLICY daibx_app_reads_and_writes ON %I FOR ALL TO daibx_app USING (true) WITH CHECK (true)',
            push_table_name
        );
    END LOOP;
END
$$;

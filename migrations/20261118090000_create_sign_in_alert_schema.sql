ALTER TYPE security_event_type ADD VALUE 'sign_in_alert_sent';
ALTER TYPE security_event_type ADD VALUE 'sign_in_alert_failed';
ALTER TYPE security_event_type ADD VALUE 'email_cap_reached';

CREATE TABLE known_browsers (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    created_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL
);

CREATE INDEX known_browsers_user_id_index ON known_browsers (user_id);

CREATE TABLE pending_sign_in_alerts (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    signed_in_at timestamptz NOT NULL,
    user_agent text,
    sign_in_method text NOT NULL CHECK (sign_in_method IN ('authenticator', 'recovery_code')),
    next_attempt_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX pending_sign_in_alerts_due_index ON pending_sign_in_alerts (next_attempt_at);
CREATE INDEX pending_sign_in_alerts_user_id_index ON pending_sign_in_alerts (user_id);

DO $$
DECLARE
    sign_in_table_name text;
    data_api_role_name text;
BEGIN
    FOREACH sign_in_table_name IN ARRAY ARRAY['known_browsers', 'pending_sign_in_alerts']
    LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', sign_in_table_name);
        FOREACH data_api_role_name IN ARRAY ARRAY['anon', 'authenticated', 'service_role']
        LOOP
            IF EXISTS (SELECT FROM pg_roles WHERE rolname = data_api_role_name) THEN
                EXECUTE format('REVOKE ALL ON TABLE %I FROM %I', sign_in_table_name, data_api_role_name);
            END IF;
        END LOOP;
    END LOOP;
END
$$;

CREATE OR REPLACE FUNCTION purge_expired_auth_data() RETURNS void
LANGUAGE sql
SECURITY INVOKER
SET search_path = public
AS $$
    DELETE FROM security_events WHERE occurred_at < now() - interval '12 months';

    DELETE FROM sessions
    WHERE revoked_at IS NOT NULL
       OR last_seen_at < now() - interval '7 days'
       OR created_at < now() - interval '30 days';

    DELETE FROM session_tokens WHERE expires_at < now();

    DELETE FROM auth_challenges WHERE expires_at < now();

    DELETE FROM email_tokens WHERE expires_at < now();

    DELETE FROM remembered_devices WHERE expires_at < now() OR revoked_at IS NOT NULL;

    DELETE FROM known_browsers WHERE expires_at < now();

    DELETE FROM invites WHERE expires_at < now() AND used_at IS NULL;

    DELETE FROM auth_throttles
    WHERE window_started_at < now() - interval '1 day'
      AND (blocked_until IS NULL OR blocked_until < now() - interval '1 day');
$$;

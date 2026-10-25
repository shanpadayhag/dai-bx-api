CREATE EXTENSION IF NOT EXISTS citext;

CREATE TYPE theme_mode AS ENUM ('light', 'dark', 'system');
CREATE TYPE accent_color AS ENUM ('blue', 'indigo', 'teal', 'violet', 'graphite');
CREATE TYPE email_token_purpose AS ENUM ('verify_email', 'password_reset', 'change_email');
CREATE TYPE auth_challenge_purpose AS ENUM ('mfa_verify', 'mfa_setup', 'mfa_replace');
CREATE TYPE session_token_kind AS ENUM ('access', 'refresh');
CREATE TYPE security_event_type AS ENUM (
    'sign_up',
    'email_verified',
    'sign_in_succeeded',
    'sign_in_failed',
    'mfa_set_up',
    'recovery_code_used',
    'recovery_codes_regenerated',
    'password_changed',
    'password_reset_requested',
    'password_reset_completed',
    'email_changed',
    'session_revoked',
    'signed_out_everywhere',
    'refresh_token_reuse_detected',
    'invite_used',
    'account_deleted'
);

CREATE TABLE users (
    id uuid PRIMARY KEY,
    security_ref uuid NOT NULL UNIQUE,
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    email citext NOT NULL UNIQUE CHECK (char_length(email) BETWEEN 3 AND 254),
    email_verified_at timestamptz,
    password_hash text NOT NULL,
    totp_secret_encrypted bytea,
    totp_last_used_step bigint,
    mfa_enabled_at timestamptz,
    accent_color accent_color NOT NULL DEFAULT 'blue',
    theme theme_mode NOT NULL DEFAULT 'system',
    terms_version text NOT NULL,
    terms_presented_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE invites (
    id uuid PRIMARY KEY,
    code_hash bytea NOT NULL UNIQUE CHECK (octet_length(code_hash) = 32),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    used_at timestamptz,
    used_by_user_id uuid REFERENCES users (id) ON DELETE SET NULL
);

CREATE TABLE email_tokens (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    purpose email_token_purpose NOT NULL,
    token_hash bytea NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    new_email citext,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    used_at timestamptz,
    CHECK ((purpose = 'change_email') = (new_email IS NOT NULL))
);
CREATE INDEX email_tokens_user_id_purpose_index ON email_tokens (user_id, purpose);

CREATE TABLE auth_challenges (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    purpose auth_challenge_purpose NOT NULL,
    token_hash bytea NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    pending_totp_secret bytea,
    attempts smallint NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL
);
CREATE INDEX auth_challenges_user_id_index ON auth_challenges (user_id);

CREATE TABLE sessions (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    revoked_at timestamptz,
    revoked_reason text,
    ip inet,
    user_agent text
);
CREATE INDEX sessions_user_id_index ON sessions (user_id);

CREATE TABLE session_tokens (
    id uuid PRIMARY KEY,
    session_id uuid NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    kind session_token_kind NOT NULL,
    token_hash bytea NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    issued_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    replaced_at timestamptz
);
CREATE INDEX session_tokens_session_id_index ON session_tokens (session_id);

CREATE TABLE remembered_devices (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    last_used_at timestamptz,
    revoked_at timestamptz,
    user_agent text
);
CREATE INDEX remembered_devices_user_id_index ON remembered_devices (user_id);

CREATE TABLE recovery_codes (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    salt bytea NOT NULL,
    code_hash bytea NOT NULL,
    used_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX recovery_codes_user_id_index ON recovery_codes (user_id);

CREATE TABLE auth_throttles (
    key text PRIMARY KEY,
    failures integer NOT NULL DEFAULT 0 CHECK (failures >= 0),
    window_started_at timestamptz NOT NULL DEFAULT now(),
    blocked_until timestamptz
);

CREATE TABLE security_events (
    id uuid PRIMARY KEY,
    occurred_at timestamptz NOT NULL DEFAULT now(),
    event_type security_event_type NOT NULL,
    subject_ref uuid,
    ip inet,
    user_agent text,
    details jsonb NOT NULL DEFAULT '{}'::jsonb
);
CREATE INDEX security_events_occurred_at_index ON security_events (occurred_at);
CREATE INDEX security_events_subject_ref_index ON security_events (subject_ref);

DO $$
DECLARE
    auth_table_name text;
    data_api_role_name text;
BEGIN
    FOREACH auth_table_name IN ARRAY ARRAY[
        'users', 'invites', 'email_tokens', 'auth_challenges', 'sessions',
        'session_tokens', 'remembered_devices', 'recovery_codes',
        'auth_throttles', 'security_events'
    ]
    LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', auth_table_name);
        FOREACH data_api_role_name IN ARRAY ARRAY['anon', 'authenticated', 'service_role']
        LOOP
            IF EXISTS (SELECT FROM pg_roles WHERE rolname = data_api_role_name) THEN
                EXECUTE format('REVOKE ALL ON TABLE %I FROM %I', auth_table_name, data_api_role_name);
            END IF;
        END LOOP;
    END LOOP;
END
$$;

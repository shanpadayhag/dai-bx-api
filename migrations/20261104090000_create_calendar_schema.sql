ALTER TABLE categories ADD UNIQUE (id, user_id);
ALTER TABLE tasks ADD UNIQUE (id, user_id);

CREATE TABLE calendar_events (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind text NOT NULL CHECK (kind IN ('planned', 'tracked')),
    title text NOT NULL CHECK (char_length(title) BETWEEN 1 AND 500 AND title = btrim(title)),
    category_id uuid,
    task_id uuid,
    card_id uuid,
    starts_at timestamptz NOT NULL,
    ends_at timestamptz,
    finished_at timestamptz,
    last_answered_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (
        kind <> 'planned'
        OR (
            task_id IS NULL
            AND card_id IS NULL
            AND finished_at IS NULL
            AND last_answered_at IS NULL
            AND ends_at IS NOT NULL
            AND ends_at >= starts_at + interval '15 minutes'
        )
    ),
    CHECK (kind <> 'tracked' OR num_nonnulls(task_id, card_id) <= 1),
    CHECK (ends_at > starts_at),
    CHECK (finished_at IS NULL OR (ends_at IS NOT NULL AND finished_at >= ends_at)),
    FOREIGN KEY (category_id, user_id) REFERENCES categories (id, user_id) ON DELETE SET NULL (category_id),
    FOREIGN KEY (task_id, user_id) REFERENCES tasks (id, user_id) ON DELETE SET NULL (task_id),
    FOREIGN KEY (card_id, user_id) REFERENCES cards (id, user_id) ON DELETE SET NULL (card_id)
);
CREATE INDEX calendar_events_user_id_starts_at_index ON calendar_events (user_id, starts_at);
CREATE INDEX calendar_events_running_user_id_index ON calendar_events (user_id) WHERE ends_at IS NULL;
CREATE UNIQUE INDEX calendar_events_running_task_id_index ON calendar_events (task_id) WHERE ends_at IS NULL;
CREATE UNIQUE INDEX calendar_events_running_card_id_index ON calendar_events (card_id) WHERE ends_at IS NULL;
CREATE INDEX calendar_events_category_id_index ON calendar_events (category_id);
CREATE INDEX calendar_events_task_id_index ON calendar_events (task_id);
CREATE INDEX calendar_events_card_id_index ON calendar_events (card_id);

ALTER TABLE users
    ADD COLUMN check_ins_enabled boolean NOT NULL DEFAULT true,
    ADD COLUMN check_in_interval_minutes integer NOT NULL DEFAULT 30
        CHECK (check_in_interval_minutes BETWEEN 5 AND 240),
    ADD COLUMN first_check_in_minutes integer NOT NULL DEFAULT 30
        CHECK (first_check_in_minutes BETWEEN 5 AND 240),
    ADD COLUMN working_hours_start time NOT NULL DEFAULT '16:00',
    ADD COLUMN working_hours_end time NOT NULL DEFAULT '02:00',
    ADD COLUMN tracked_category_id uuid,
    ADD FOREIGN KEY (tracked_category_id, id) REFERENCES categories (id, user_id) ON DELETE SET NULL (tracked_category_id);
CREATE INDEX users_tracked_category_id_index ON users (tracked_category_id);

DO $$
DECLARE
    data_api_role_name text;
BEGIN
    ALTER TABLE calendar_events ENABLE ROW LEVEL SECURITY;
    FOREACH data_api_role_name IN ARRAY ARRAY['anon', 'authenticated', 'service_role']
    LOOP
        IF EXISTS (SELECT FROM pg_roles WHERE rolname = data_api_role_name) THEN
            EXECUTE format('REVOKE ALL ON TABLE calendar_events FROM %I', data_api_role_name);
        END IF;
    END LOOP;
END
$$;

ALTER TABLE users
    ADD COLUMN calendar_time_zone text CHECK (char_length(calendar_time_zone) BETWEEN 1 AND 64);

CREATE TABLE card_schedules (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL,
    board_id uuid NOT NULL,
    card_id uuid NOT NULL,
    kind text NOT NULL CHECK (kind IN ('once', 'repeating')),
    target_list_id uuid,
    time_zone text NOT NULL CHECK (char_length(time_zone) BETWEEN 1 AND 64),
    local_start timestamp NOT NULL,
    repeat_rule jsonb CHECK (jsonb_typeof(repeat_rule) = 'object'),
    block_minutes integer CHECK (block_minutes >= 15),
    block_category_id uuid,
    paused boolean NOT NULL DEFAULT false,
    next_run_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((kind = 'once') = (repeat_rule IS NULL)),
    CHECK (block_category_id IS NULL OR block_minutes IS NOT NULL),
    UNIQUE (id, user_id),
    FOREIGN KEY (board_id, user_id) REFERENCES boards (id, user_id) ON DELETE CASCADE,
    FOREIGN KEY (card_id, board_id) REFERENCES cards (id, board_id) ON DELETE CASCADE,
    FOREIGN KEY (target_list_id, board_id) REFERENCES board_lists (id, board_id)
        ON DELETE SET NULL (target_list_id),
    FOREIGN KEY (block_category_id, user_id) REFERENCES categories (id, user_id)
        ON DELETE SET NULL (block_category_id)
);
CREATE INDEX card_schedules_card_id_index ON card_schedules (card_id);
CREATE INDEX card_schedules_board_id_index ON card_schedules (board_id);
CREATE INDEX card_schedules_user_id_index ON card_schedules (user_id);
CREATE INDEX card_schedules_target_list_id_index ON card_schedules (target_list_id);
CREATE INDEX card_schedules_block_category_id_index ON card_schedules (block_category_id);
CREATE INDEX card_schedules_due_next_run_at_index ON card_schedules (next_run_at)
    WHERE NOT paused AND target_list_id IS NOT NULL AND next_run_at IS NOT NULL;

CREATE TABLE schedule_runs (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL,
    schedule_id uuid NOT NULL,
    occurrence_at timestamptz NOT NULL,
    ran_at timestamptz NOT NULL,
    how text NOT NULL CHECK (how IN ('on_time', 'late', 'by_hand')),
    moved boolean NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (schedule_id, user_id) REFERENCES card_schedules (id, user_id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX schedule_runs_scheduled_occurrence_index ON schedule_runs (schedule_id, occurrence_at)
    WHERE how <> 'by_hand';
CREATE INDEX schedule_runs_schedule_id_ran_at_index ON schedule_runs (schedule_id, ran_at);
CREATE INDEX schedule_runs_user_id_index ON schedule_runs (user_id);

CREATE TABLE event_series (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title text NOT NULL CHECK (char_length(title) BETWEEN 1 AND 500 AND title = btrim(title)),
    category_id uuid,
    time_zone text NOT NULL CHECK (char_length(time_zone) BETWEEN 1 AND 64),
    local_start timestamp NOT NULL,
    duration_minutes integer NOT NULL CHECK (duration_minutes >= 15),
    repeat_rule jsonb NOT NULL CHECK (jsonb_typeof(repeat_rule) = 'object'),
    schedule_id uuid,
    until_instant timestamptz,
    hidden boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, user_id),
    FOREIGN KEY (category_id, user_id) REFERENCES categories (id, user_id)
        ON DELETE SET NULL (category_id),
    FOREIGN KEY (schedule_id, user_id) REFERENCES card_schedules (id, user_id)
        ON DELETE SET NULL (schedule_id)
);
CREATE INDEX event_series_user_id_index ON event_series (user_id);
CREATE INDEX event_series_category_id_index ON event_series (category_id);
CREATE INDEX event_series_schedule_id_index ON event_series (schedule_id);

CREATE TABLE event_exceptions (
    series_id uuid NOT NULL,
    user_id uuid NOT NULL,
    original_start timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (series_id, original_start),
    FOREIGN KEY (series_id, user_id) REFERENCES event_series (id, user_id) ON DELETE CASCADE
);
CREATE INDEX event_exceptions_user_id_index ON event_exceptions (user_id);

ALTER TABLE calendar_events
    ADD COLUMN time_zone text CHECK (char_length(time_zone) BETWEEN 1 AND 64),
    ADD COLUMN series_id uuid,
    ADD COLUMN original_start timestamptz,
    ADD COLUMN schedule_id uuid,
    ADD CHECK ((series_id IS NULL) = (original_start IS NULL)),
    ADD CHECK (kind = 'planned' OR num_nonnulls(series_id, schedule_id) = 0),
    ADD UNIQUE (series_id, original_start),
    ADD FOREIGN KEY (series_id, user_id) REFERENCES event_series (id, user_id) ON DELETE CASCADE,
    ADD FOREIGN KEY (schedule_id, user_id) REFERENCES card_schedules (id, user_id)
        ON DELETE SET NULL (schedule_id);
CREATE INDEX calendar_events_schedule_id_index ON calendar_events (schedule_id);

DO $$
DECLARE
    new_table_name text;
    data_api_role_name text;
BEGIN
    FOREACH new_table_name IN ARRAY ARRAY['card_schedules', 'schedule_runs', 'event_series', 'event_exceptions']
    LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', new_table_name);
        FOREACH data_api_role_name IN ARRAY ARRAY['anon', 'authenticated', 'service_role']
        LOOP
            IF EXISTS (SELECT FROM pg_roles WHERE rolname = data_api_role_name) THEN
                EXECUTE format('REVOKE ALL ON TABLE %I FROM %I', new_table_name, data_api_role_name);
            END IF;
        END LOOP;
    END LOOP;
END
$$;

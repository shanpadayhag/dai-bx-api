CREATE TABLE activity_entries (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    occurred_at timestamptz NOT NULL,
    kind text NOT NULL CHECK (
        kind IN (
            'started',
            'finished',
            'checked_in',
            'task_completed',
            'task_reopened',
            'card_moved',
            'schedule_ran',
            'plan_accepted'
        )
    ),
    task_id uuid,
    card_id uuid,
    title text CHECK (char_length(title) BETWEEN 1 AND 500),
    context_name text CHECK (char_length(context_name) BETWEEN 1 AND 200),
    details jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(details) = 'object'),
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (num_nonnulls(task_id, card_id) <= 1),
    CHECK (title IS NOT NULL OR kind = 'plan_accepted'),
    FOREIGN KEY (task_id, user_id) REFERENCES tasks (id, user_id) ON DELETE SET NULL (task_id),
    FOREIGN KEY (card_id, user_id) REFERENCES cards (id, user_id) ON DELETE SET NULL (card_id)
);

CREATE INDEX activity_entries_user_id_occurred_at_id_index
    ON activity_entries (user_id, occurred_at DESC, id DESC);
CREATE INDEX activity_entries_task_id_index ON activity_entries (task_id);
CREATE INDEX activity_entries_card_id_index ON activity_entries (card_id);

ALTER TABLE activity_entries ENABLE ROW LEVEL SECURITY;

DO $$
DECLARE
    data_api_role_name text;
BEGIN
    FOREACH data_api_role_name IN ARRAY ARRAY['anon', 'authenticated', 'service_role']
    LOOP
        IF EXISTS (SELECT FROM pg_roles WHERE rolname = data_api_role_name) THEN
            EXECUTE format('REVOKE ALL ON TABLE activity_entries FROM %I', data_api_role_name);
        END IF;
    END LOOP;
END
$$;

REVOKE UPDATE, DELETE, TRUNCATE ON TABLE activity_entries FROM daibx_app;

CREATE POLICY daibx_app_reads_activity ON activity_entries
    FOR SELECT
    TO daibx_app
    USING (true);

CREATE POLICY daibx_app_adds_activity ON activity_entries
    FOR INSERT
    TO daibx_app
    WITH CHECK (true);

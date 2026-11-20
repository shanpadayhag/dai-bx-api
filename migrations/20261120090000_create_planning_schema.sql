ALTER TABLE tasks
    ADD COLUMN priority text NOT NULL DEFAULT 'normal' CHECK (priority IN ('high', 'normal', 'low')),
    ADD COLUMN due_date date,
    ADD COLUMN due_time time,
    ADD COLUMN plan_window_start time,
    ADD COLUMN plan_window_end time,
    ADD CHECK (due_time IS NULL OR due_date IS NOT NULL),
    ADD CHECK ((plan_window_start IS NULL) = (plan_window_end IS NULL)),
    ADD CHECK (plan_window_start IS NULL OR plan_window_start <> plan_window_end),
    ADD CONSTRAINT tasks_planning_fields_top_level_check CHECK (
        parent_id IS NULL
        OR (priority = 'normal' AND due_date IS NULL AND plan_window_start IS NULL)
    );

ALTER TABLE users
    ADD COLUMN planning_hours jsonb CHECK (jsonb_typeof(planning_hours) = 'array');

ALTER TABLE calendar_events
    DROP CONSTRAINT calendar_events_check,
    ADD CONSTRAINT calendar_events_planned_check CHECK (
        kind <> 'planned'
        OR (
            card_id IS NULL
            AND finished_at IS NULL
            AND last_answered_at IS NULL
            AND ends_at IS NOT NULL
            AND ends_at >= starts_at + interval '15 minutes'
            AND (task_id IS NULL OR num_nonnulls(series_id, schedule_id) = 0)
        )
    );

CREATE INDEX calendar_events_planned_task_id_ends_at_index
    ON calendar_events (task_id, ends_at)
    WHERE kind = 'planned';

ALTER TABLE task_groups
    ADD COLUMN show_rule jsonb CHECK (jsonb_typeof(show_rule) = 'object'),
    ADD COLUMN show_start_date date,
    ADD COLUMN show_window_start time,
    ADD COLUMN show_window_end time,
    ADD COLUMN uncheck_rule jsonb CHECK (jsonb_typeof(uncheck_rule) = 'object'),
    ADD COLUMN uncheck_start_date date,
    ADD COLUMN uncheck_time time,
    ADD CHECK ((show_rule IS NULL) = (show_start_date IS NULL)),
    ADD CHECK ((show_window_start IS NULL) = (show_window_end IS NULL)),
    ADD CHECK (show_window_start IS NULL OR show_window_start <> show_window_end),
    ADD CHECK (show_window_start IS NULL OR show_rule IS NOT NULL),
    ADD CHECK ((uncheck_rule IS NULL) = (uncheck_start_date IS NULL)),
    ADD CHECK ((uncheck_rule IS NULL) = (uncheck_time IS NULL));

ALTER TABLE card_schedules
    ADD COLUMN block_local_time time,
    ADD CHECK (block_local_time IS NULL OR block_minutes IS NOT NULL);

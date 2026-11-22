ALTER TABLE tasks
    ADD COLUMN search_vector tsvector GENERATED ALWAYS AS (to_tsvector('simple'::regconfig, name)) STORED;
CREATE INDEX tasks_search_vector_index ON tasks USING gin (search_vector);

ALTER TABLE task_groups
    ADD COLUMN search_vector tsvector GENERATED ALWAYS AS (to_tsvector('simple'::regconfig, name)) STORED;
CREATE INDEX task_groups_search_vector_index ON task_groups USING gin (search_vector);

ALTER TABLE cards
    ADD COLUMN search_vector tsvector GENERATED ALWAYS AS (
        setweight(to_tsvector('simple'::regconfig, title), 'A')
        || setweight(to_tsvector('simple'::regconfig, notes), 'B')
    ) STORED;
CREATE INDEX cards_search_vector_index ON cards USING gin (search_vector);

ALTER TABLE calendar_events
    ADD COLUMN search_vector tsvector GENERATED ALWAYS AS (to_tsvector('simple'::regconfig, title)) STORED;
CREATE INDEX calendar_events_search_vector_index ON calendar_events USING gin (search_vector)
    WHERE kind = 'planned';

ALTER TABLE event_series
    ADD COLUMN search_vector tsvector GENERATED ALWAYS AS (to_tsvector('simple'::regconfig, title)) STORED;
CREATE INDEX event_series_search_vector_index ON event_series USING gin (search_vector);

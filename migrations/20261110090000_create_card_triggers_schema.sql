ALTER TABLE cards
    ADD COLUMN last_moved_at timestamptz NOT NULL DEFAULT now(),
    ADD COLUMN archived_at timestamptz,
    ADD COLUMN archived_how text CHECK (archived_how IN ('idle', 'by_hand')),
    ADD COLUMN archived_from_list_id uuid,
    ADD COLUMN archived_from_list_name text
        CHECK (char_length(archived_from_list_name) BETWEEN 1 AND 100),
    ALTER COLUMN list_id DROP NOT NULL,
    ALTER COLUMN position DROP NOT NULL,
    ADD CHECK ((archived_at IS NULL) = (list_id IS NOT NULL)),
    ADD CHECK ((list_id IS NULL) = (position IS NULL)),
    ADD CHECK ((archived_at IS NULL) = (archived_how IS NULL)),
    ADD CHECK (archived_at IS NOT NULL OR num_nonnulls(archived_from_list_id, archived_from_list_name) = 0),
    ADD FOREIGN KEY (archived_from_list_id, board_id) REFERENCES board_lists (id, board_id)
        ON DELETE SET NULL (archived_from_list_id);
CREATE INDEX cards_archived_from_list_id_index ON cards (archived_from_list_id);
CREATE INDEX cards_board_id_archived_at_index ON cards (board_id, archived_at DESC, id DESC)
    WHERE archived_at IS NOT NULL;

ALTER TABLE board_lists
    ADD COLUMN archive_after interval CHECK (
        archive_after >= interval '1 day'
        AND archive_after <= interval '365 days'
        AND date_trunc('day', archive_after) = archive_after
    );

ALTER TABLE card_schedules
    ADD COLUMN paused_by_archive boolean NOT NULL DEFAULT false,
    ADD CHECK (NOT paused_by_archive OR paused);

CREATE TABLE card_triggers (
    card_id uuid NOT NULL,
    kind text NOT NULL CHECK (kind IN ('started', 'finished')),
    user_id uuid NOT NULL,
    board_id uuid NOT NULL,
    list_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (card_id, kind),
    FOREIGN KEY (board_id, user_id) REFERENCES boards (id, user_id) ON DELETE CASCADE,
    FOREIGN KEY (card_id, board_id) REFERENCES cards (id, board_id) ON DELETE CASCADE,
    FOREIGN KEY (list_id, board_id) REFERENCES board_lists (id, board_id)
        ON DELETE SET NULL (list_id)
);
CREATE INDEX card_triggers_board_id_index ON card_triggers (board_id);
CREATE INDEX card_triggers_user_id_index ON card_triggers (user_id);
CREATE INDEX card_triggers_list_id_index ON card_triggers (list_id);

CREATE TABLE card_moves (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL,
    card_id uuid NOT NULL,
    moved_at timestamptz NOT NULL,
    from_list_name text CHECK (char_length(from_list_name) BETWEEN 1 AND 100),
    to_list_name text CHECK (char_length(to_list_name) BETWEEN 1 AND 100),
    reason text NOT NULL CHECK (
        reason IN ('by_hand', 'schedule', 'started', 'finished', 'archived_idle', 'archived_by_hand', 'restored')
    ),
    task_title text CHECK (char_length(task_title) BETWEEN 1 AND 500),
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (task_title IS NULL OR reason IN ('started', 'finished')),
    FOREIGN KEY (card_id, user_id) REFERENCES cards (id, user_id) ON DELETE CASCADE
);
CREATE INDEX card_moves_card_id_id_index ON card_moves (card_id, id DESC);
CREATE INDEX card_moves_user_id_index ON card_moves (user_id);

DO $$
DECLARE
    new_table_name text;
    data_api_role_name text;
BEGIN
    FOREACH new_table_name IN ARRAY ARRAY['card_triggers', 'card_moves']
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

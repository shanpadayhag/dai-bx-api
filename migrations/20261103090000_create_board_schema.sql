CREATE TABLE boards (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100 AND name = btrim(name)),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, user_id)
);
CREATE INDEX boards_user_id_index ON boards (user_id);

CREATE TABLE board_lists (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL,
    board_id uuid NOT NULL,
    position integer NOT NULL CHECK (position >= 0),
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100 AND name = btrim(name)),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, board_id),
    FOREIGN KEY (board_id, user_id) REFERENCES boards (id, user_id) ON DELETE CASCADE
);
CREATE INDEX board_lists_board_id_position_index ON board_lists (board_id, position);
CREATE INDEX board_lists_user_id_index ON board_lists (user_id);

CREATE TABLE cards (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL,
    board_id uuid NOT NULL,
    list_id uuid NOT NULL,
    position integer NOT NULL CHECK (position >= 0),
    title text NOT NULL CHECK (char_length(title) BETWEEN 1 AND 500 AND title = btrim(title)),
    notes text NOT NULL CHECK (char_length(notes) <= 20000),
    due_date date,
    due_time time,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (due_time IS NULL OR due_date IS NOT NULL),
    UNIQUE (id, board_id),
    UNIQUE (id, user_id),
    FOREIGN KEY (board_id, user_id) REFERENCES boards (id, user_id) ON DELETE CASCADE,
    FOREIGN KEY (list_id, board_id) REFERENCES board_lists (id, board_id) ON DELETE CASCADE
);
CREATE INDEX cards_board_id_list_id_position_index ON cards (board_id, list_id, position);
CREATE INDEX cards_list_id_board_id_index ON cards (list_id, board_id);
CREATE INDEX cards_user_id_index ON cards (user_id);

CREATE TABLE board_properties (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL,
    board_id uuid NOT NULL,
    position integer NOT NULL CHECK (position >= 0),
    name citext NOT NULL CHECK (char_length(name) BETWEEN 1 AND 50 AND name = btrim(name)),
    kind text NOT NULL CHECK (kind IN ('choice', 'text', 'number')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (board_id, name),
    UNIQUE (id, board_id),
    UNIQUE (id, user_id),
    FOREIGN KEY (board_id, user_id) REFERENCES boards (id, user_id) ON DELETE CASCADE
);
CREATE INDEX board_properties_board_id_position_index ON board_properties (board_id, position);
CREATE INDEX board_properties_user_id_index ON board_properties (user_id);

CREATE TABLE property_options (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL,
    property_id uuid NOT NULL,
    position integer NOT NULL CHECK (position >= 0),
    name citext NOT NULL CHECK (char_length(name) BETWEEN 1 AND 50 AND name = btrim(name)),
    color text CHECK (color ~ '^#[0-9a-f]{6}$'),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (property_id, name),
    UNIQUE (id, property_id),
    FOREIGN KEY (property_id, user_id) REFERENCES board_properties (id, user_id) ON DELETE CASCADE
);
CREATE INDEX property_options_property_id_position_index ON property_options (property_id, position);
CREATE INDEX property_options_user_id_index ON property_options (user_id);

CREATE TABLE card_property_values (
    user_id uuid NOT NULL,
    board_id uuid NOT NULL,
    card_id uuid NOT NULL,
    property_id uuid NOT NULL,
    option_id uuid,
    text_value text CHECK (char_length(text_value) BETWEEN 1 AND 500 AND text_value = btrim(text_value)),
    number_value numeric CHECK (number_value = round(number_value, 6) AND abs(number_value) < 1e15),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (card_id, property_id),
    CHECK (num_nonnulls(option_id, text_value, number_value) = 1),
    FOREIGN KEY (board_id, user_id) REFERENCES boards (id, user_id) ON DELETE CASCADE,
    FOREIGN KEY (card_id, board_id) REFERENCES cards (id, board_id) ON DELETE CASCADE,
    FOREIGN KEY (property_id, board_id) REFERENCES board_properties (id, board_id) ON DELETE CASCADE,
    FOREIGN KEY (option_id, property_id) REFERENCES property_options (id, property_id) ON DELETE CASCADE
);
CREATE INDEX card_property_values_board_id_index ON card_property_values (board_id);
CREATE INDEX card_property_values_property_id_board_id_index ON card_property_values (property_id, board_id);
CREATE INDEX card_property_values_option_id_property_id_index ON card_property_values (option_id, property_id);
CREATE INDEX card_property_values_user_id_index ON card_property_values (user_id);

ALTER TABLE tasks
    ADD COLUMN card_id uuid,
    ADD FOREIGN KEY (card_id, user_id) REFERENCES cards (id, user_id) ON DELETE SET NULL (card_id);
CREATE INDEX tasks_card_id_index ON tasks (card_id);

DO $$
DECLARE
    board_table_name text;
    data_api_role_name text;
BEGIN
    FOREACH board_table_name IN ARRAY ARRAY['boards', 'board_lists', 'cards', 'board_properties', 'property_options', 'card_property_values']
    LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', board_table_name);
        FOREACH data_api_role_name IN ARRAY ARRAY['anon', 'authenticated', 'service_role']
        LOOP
            IF EXISTS (SELECT FROM pg_roles WHERE rolname = data_api_role_name) THEN
                EXECUTE format('REVOKE ALL ON TABLE %I FROM %I', board_table_name, data_api_role_name);
            END IF;
        END LOOP;
    END LOOP;
END
$$;

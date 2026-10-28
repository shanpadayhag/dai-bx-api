CREATE TABLE categories (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name citext NOT NULL CHECK (char_length(name) BETWEEN 1 AND 50 AND name = btrim(name)),
    color text NOT NULL CHECK (color ~ '^#[0-9a-f]{6}$'),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, name),
    UNIQUE (id, user_id)
);

CREATE TABLE task_groups (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 200 AND name = btrim(name)),
    category_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, user_id),
    FOREIGN KEY (category_id, user_id) REFERENCES categories (id, user_id) ON DELETE SET NULL (category_id)
);
CREATE INDEX task_groups_user_id_created_at_index ON task_groups (user_id, created_at DESC, id DESC);
CREATE INDEX task_groups_category_id_index ON task_groups (category_id);

CREATE TABLE tasks (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL,
    group_id uuid NOT NULL,
    parent_id uuid,
    position integer NOT NULL CHECK (position >= 0),
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 500 AND name = btrim(name)),
    estimate_minutes integer CHECK (estimate_minutes BETWEEN 1 AND 1440),
    completed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, group_id),
    FOREIGN KEY (group_id, user_id) REFERENCES task_groups (id, user_id) ON DELETE CASCADE,
    FOREIGN KEY (parent_id, group_id) REFERENCES tasks (id, group_id) ON UPDATE CASCADE ON DELETE CASCADE
);
CREATE INDEX tasks_group_id_parent_id_position_index ON tasks (group_id, parent_id, position);
CREATE INDEX tasks_parent_id_group_id_index ON tasks (parent_id, group_id);
CREATE INDEX tasks_user_id_index ON tasks (user_id);

DO $$
DECLARE
    tasks_table_name text;
    data_api_role_name text;
BEGIN
    FOREACH tasks_table_name IN ARRAY ARRAY['categories', 'task_groups', 'tasks']
    LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', tasks_table_name);
        FOREACH data_api_role_name IN ARRAY ARRAY['anon', 'authenticated', 'service_role']
        LOOP
            IF EXISTS (SELECT FROM pg_roles WHERE rolname = data_api_role_name) THEN
                EXECUTE format('REVOKE ALL ON TABLE %I FROM %I', tasks_table_name, data_api_role_name);
            END IF;
        END LOOP;
    END LOOP;
END
$$;

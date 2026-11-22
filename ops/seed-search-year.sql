\set ON_ERROR_STOP on

BEGIN;

SELECT setseed(0.42);

CREATE TEMP TABLE seed_words (word_index integer PRIMARY KEY, word text NOT NULL) ON COMMIT DROP;
INSERT INTO seed_words (word_index, word)
SELECT ordinality, word
FROM unnest(string_to_array(
    'plan review write read call email meeting report draft budget invoice client project launch '
    'design build test deploy release fix bug feature spec research notes outline chapter lesson '
    'chinese vocabulary tones grammar practice gym run swim stretch groceries fruit laundry dishes '
    'clean cook dinner lunch breakfast doctor dentist bank rent bills taxes travel flight hotel '
    'packing birthday gift party family friends weekend morning evening weekly monthly daily '
    'portfolio resume interview application follow update backlog sprint standup retro demo '
    'database server api frontend backend schema migration index query cache search calendar '
    'board card task group schedule tracking checkin notification export import security audit',
    ' '
)) WITH ORDINALITY AS seed_word_list (word, ordinality);

CREATE FUNCTION pg_temp.seed_text(word_count integer) RETURNS text
    LANGUAGE sql VOLATILE
AS $$
    SELECT initcap(string_agg(
        (SELECT seed_words.word FROM seed_words WHERE seed_words.word_index = picked_words.word_index),
        ' '
    ))
    FROM (
        SELECT 1 + floor(random() * (SELECT count(*) FROM seed_words))::integer AS word_index
        FROM generate_series(1, greatest(word_count, 1))
    ) AS picked_words
$$;

INSERT INTO users (id, security_ref, name, email, password_hash, terms_version, terms_presented_at)
VALUES (gen_random_uuid(), gen_random_uuid(), 'Search Timing', :'email', '$argon2id$not-a-password', 'draft-1', now());

CREATE TEMP TABLE seed_owner ON COMMIT DROP AS
SELECT id AS user_id FROM users WHERE email = :'email';

WITH new_session AS (
    INSERT INTO sessions (id, user_id)
    SELECT gen_random_uuid(), user_id FROM seed_owner
    RETURNING id
)
INSERT INTO session_tokens (id, session_id, kind, token_hash, expires_at)
SELECT gen_random_uuid(), id, 'access', sha256(convert_to(:'access_token', 'UTF8')), now() + interval '2 hours'
FROM new_session;

INSERT INTO task_groups (id, user_id, name, created_at, updated_at)
SELECT gen_random_uuid(), seed_owner.user_id, pg_temp.seed_text(1 + (group_number % 3)),
       now() - (group_number || ' days')::interval, now() - (group_number || ' days')::interval
FROM seed_owner, generate_series(1, 300) AS group_number;

CREATE TEMP TABLE seed_groups ON COMMIT DROP AS
SELECT id, row_number() OVER (ORDER BY id) AS group_number FROM task_groups
WHERE user_id = (SELECT user_id FROM seed_owner);

INSERT INTO tasks (id, user_id, group_id, position, name, completed_at, created_at, updated_at)
SELECT gen_random_uuid(), seed_owner.user_id, seed_groups.id, task_number % 9, pg_temp.seed_text(2 + (task_number % 4)),
       CASE WHEN task_number % 3 <> 0 THEN now() - ((task_number % 365) || ' days')::interval END,
       now() - ((task_number % 365) || ' days')::interval, now() - ((task_number % 365) || ' days')::interval
FROM seed_owner, generate_series(1, 2700) AS task_number
JOIN seed_groups ON seed_groups.group_number = 1 + (task_number % 300);

INSERT INTO tasks (id, user_id, group_id, parent_id, position, name, completed_at, created_at, updated_at)
SELECT gen_random_uuid(), parent_tasks.user_id, parent_tasks.group_id, parent_tasks.id, 0, pg_temp.seed_text(2 + (subtask_number % 3)),
       CASE WHEN subtask_number % 3 <> 0 THEN parent_tasks.created_at END,
       parent_tasks.created_at, parent_tasks.created_at
FROM generate_series(1, 1300) AS subtask_number
JOIN (
    SELECT tasks.*, row_number() OVER (ORDER BY tasks.id) AS task_number FROM tasks
    WHERE tasks.user_id = (SELECT user_id FROM seed_owner) AND tasks.parent_id IS NULL
) AS parent_tasks ON parent_tasks.task_number = 1 + (subtask_number % 2700);

INSERT INTO boards (id, user_id, name)
SELECT gen_random_uuid(), seed_owner.user_id, board_name
FROM seed_owner, unnest(ARRAY['Personal', 'Work', 'Learning']) AS board_name;

INSERT INTO board_lists (id, user_id, board_id, position, name)
SELECT gen_random_uuid(), boards.user_id, boards.id, list_position, list_name
FROM boards
CROSS JOIN unnest(ARRAY['To do', 'Doing', 'Waiting', 'Done']) WITH ORDINALITY AS board_list_names (list_name, list_position)
WHERE boards.user_id = (SELECT user_id FROM seed_owner);

CREATE TEMP TABLE seed_lists ON COMMIT DROP AS
SELECT id, board_id, name, row_number() OVER (ORDER BY board_id, position) AS list_number FROM board_lists
WHERE user_id = (SELECT user_id FROM seed_owner);

INSERT INTO cards (id, user_id, board_id, list_id, position, title, notes, created_at, updated_at)
SELECT gen_random_uuid(), seed_owner.user_id, seed_lists.board_id, seed_lists.id, card_number / 12,
       pg_temp.seed_text(2 + (card_number % 4)),
       CASE WHEN card_number % 5 = 0 THEN '' ELSE left(pg_temp.seed_text(1 + (card_number % 450)), 3000) END,
       now() - ((card_number % 365) || ' days')::interval, now() - ((card_number % 365) || ' days')::interval
FROM seed_owner, generate_series(1, 1200) AS card_number
JOIN seed_lists ON seed_lists.list_number = 1 + (card_number % 12);

UPDATE cards
SET archived_from_list_id = list_id,
    archived_from_list_name = (SELECT name FROM board_lists WHERE board_lists.id = cards.list_id),
    list_id = NULL,
    position = NULL,
    archived_at = updated_at,
    archived_how = 'by_hand'
WHERE id IN (
    SELECT id FROM cards WHERE user_id = (SELECT user_id FROM seed_owner) ORDER BY id LIMIT 150
);

INSERT INTO calendar_events (id, user_id, kind, title, starts_at, ends_at)
SELECT gen_random_uuid(), seed_owner.user_id, 'planned', pg_temp.seed_text(2 + (event_number % 3)),
       date_trunc('hour', now()) - interval '300 days' + (event_number * interval '5 hours'),
       date_trunc('hour', now()) - interval '300 days' + (event_number * interval '5 hours') + interval '1 hour'
FROM seed_owner, generate_series(1, 1500) AS event_number;

INSERT INTO event_series (id, user_id, title, time_zone, local_start, duration_minutes, repeat_rule)
SELECT gen_random_uuid(), seed_owner.user_id, pg_temp.seed_text(2), 'Asia/Manila',
       (date_trunc('day', now() - interval '200 days') + (series_number || ' hours')::interval)::timestamp, 60,
       CASE WHEN series_number % 2 = 0
            THEN '{"frequency":"daily","interval":1,"ends":{"never":true}}'::jsonb
            ELSE '{"frequency":"weekly","interval":1,"weekdays":["mon","wed"],"ends":{"never":true}}'::jsonb
       END
FROM seed_owner, generate_series(1, 40) AS series_number;

INSERT INTO event_exceptions (series_id, user_id, original_start)
SELECT event_series.id, event_series.user_id,
       (event_series.local_start AT TIME ZONE 'Asia/Manila') + (exception_number * interval '7 days')
FROM event_series, generate_series(1, 10) AS exception_number
WHERE event_series.user_id = (SELECT user_id FROM seed_owner);

INSERT INTO calendar_events (id, user_id, kind, title, task_id, starts_at, ends_at)
SELECT gen_random_uuid(), tracked_tasks.user_id, 'tracked', tracked_tasks.name, tracked_tasks.id,
       now() - interval '365 days' + (tracked_number * interval '50 minutes'),
       now() - interval '365 days' + (tracked_number * interval '50 minutes') + interval '30 minutes'
FROM generate_series(1, 10000) AS tracked_number
JOIN (
    SELECT tasks.*, row_number() OVER (ORDER BY tasks.id) AS task_number FROM tasks
    WHERE tasks.user_id = (SELECT user_id FROM seed_owner)
) AS tracked_tasks ON tracked_tasks.task_number = 1 + (tracked_number % 4000);

COMMIT;

ANALYZE tasks;
ANALYZE task_groups;
ANALYZE cards;
ANALYZE calendar_events;
ANALYZE event_series;

SELECT
    (SELECT count(*) FROM task_groups WHERE user_id = users.id) AS group_count,
    (SELECT count(*) FROM tasks WHERE user_id = users.id) AS task_count,
    (SELECT count(*) FROM cards WHERE user_id = users.id) AS card_count,
    (SELECT count(*) FROM cards WHERE user_id = users.id AND archived_at IS NOT NULL) AS archived_card_count,
    (SELECT count(*) FROM calendar_events WHERE user_id = users.id AND kind = 'planned') AS planned_event_count,
    (SELECT count(*) FROM event_series WHERE user_id = users.id) AS series_count,
    (SELECT count(*) FROM calendar_events WHERE user_id = users.id AND kind = 'tracked') AS tracked_event_count
FROM users WHERE email = :'email';

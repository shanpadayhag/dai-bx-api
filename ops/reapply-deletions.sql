-- Erases, from a restored database, the accounts deleted after the backup was taken.
-- deleted-accounts.csv comes from the database being replaced (see restore-backup.md).
--   psql "$RESTORED_DATABASE_URL" --set=ON_ERROR_STOP=1 \
--     --set=deletions_file=deleted-accounts.csv --file=ops/reapply-deletions.sql

BEGIN;

CREATE TEMP TABLE accounts_deleted_since_backup (
    subject_ref uuid NOT NULL,
    occurred_at timestamptz NOT NULL
) ON COMMIT DROP;

\set copy_command '\\copy accounts_deleted_since_backup FROM ' :'deletions_file' ' WITH (FORMAT csv)'
:copy_command

INSERT INTO security_events (id, occurred_at, event_type, subject_ref)
SELECT gen_random_uuid(), deleted_account.occurred_at, 'account_deleted', deleted_account.subject_ref
FROM accounts_deleted_since_backup AS deleted_account
WHERE NOT EXISTS (
    SELECT FROM security_events AS existing_event
    WHERE existing_event.event_type = 'account_deleted'
      AND existing_event.subject_ref = deleted_account.subject_ref
);

DELETE FROM users
USING accounts_deleted_since_backup AS deleted_account
WHERE users.security_ref = deleted_account.subject_ref;

COMMIT;

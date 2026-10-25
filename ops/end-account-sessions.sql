UPDATE sessions
SET revoked_at = now(), revoked_reason = 'ended_by_operator'
WHERE revoked_at IS NULL
  AND user_id = (SELECT id FROM users WHERE email = :'account_email'::citext);

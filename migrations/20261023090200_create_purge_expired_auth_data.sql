CREATE FUNCTION purge_expired_auth_data() RETURNS void
LANGUAGE sql
SECURITY INVOKER
SET search_path = public
AS $$
    DELETE FROM security_events WHERE occurred_at < now() - interval '12 months';

    DELETE FROM sessions
    WHERE revoked_at IS NOT NULL
       OR last_seen_at < now() - interval '7 days'
       OR created_at < now() - interval '30 days';

    DELETE FROM session_tokens WHERE expires_at < now();

    DELETE FROM auth_challenges WHERE expires_at < now();

    DELETE FROM email_tokens WHERE expires_at < now();

    DELETE FROM remembered_devices WHERE expires_at < now() OR revoked_at IS NOT NULL;

    DELETE FROM invites WHERE expires_at < now() AND used_at IS NULL;

    DELETE FROM auth_throttles
    WHERE window_started_at < now() - interval '1 day'
      AND (blocked_until IS NULL OR blocked_until < now() - interval '1 day');
$$;

REVOKE ALL ON FUNCTION purge_expired_auth_data() FROM PUBLIC;

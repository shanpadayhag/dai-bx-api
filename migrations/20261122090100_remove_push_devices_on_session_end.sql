CREATE FUNCTION remove_push_devices_of_ended_session() RETURNS trigger
LANGUAGE plpgsql
SECURITY INVOKER
SET search_path = public
AS $$
BEGIN
    WITH removed_device AS (
        DELETE FROM push_devices WHERE session_id = NEW.id RETURNING id
    )
    INSERT INTO security_events (id, event_type, subject_ref, details)
    SELECT gen_random_uuid(),
           'push_device_removed',
           users.security_ref,
           jsonb_build_object('device_ref', removed_device.id::text, 'reason', 'session_ended')
    FROM removed_device
    CROSS JOIN users
    WHERE users.id = NEW.user_id;
    RETURN NULL;
END
$$;

CREATE TRIGGER sessions_remove_push_devices_on_end
    AFTER UPDATE OF revoked_at ON sessions
    FOR EACH ROW
    WHEN (OLD.revoked_at IS NULL AND NEW.revoked_at IS NOT NULL)
    EXECUTE FUNCTION remove_push_devices_of_ended_session();

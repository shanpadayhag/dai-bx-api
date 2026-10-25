DO $$
BEGIN
    CREATE ROLE invite_operator LOGIN NOINHERIT;
EXCEPTION
    WHEN duplicate_object OR unique_violation THEN
        NULL;
END
$$;

GRANT USAGE ON SCHEMA public TO invite_operator;
GRANT SELECT, INSERT ON TABLE invites TO invite_operator;

CREATE POLICY invite_operator_creates_and_reads_invites ON invites
    FOR ALL
    TO invite_operator
    USING (true)
    WITH CHECK (true);

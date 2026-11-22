CREATE FUNCTION search_prefix_query(search_words text[]) RETURNS tsquery
    LANGUAGE sql
    IMMUTABLE
    STRICT
    PARALLEL SAFE
AS $$
    SELECT to_tsquery('simple', string_agg(quote_literal(search_word) || ':*', ' & '))
    FROM unnest(search_words) AS search_word
$$;

REVOKE ALL ON FUNCTION search_prefix_query(text[]) FROM PUBLIC;

DO $$
BEGIN
    IF EXISTS (SELECT FROM pg_roles WHERE rolname = 'daibx_app') THEN
        GRANT EXECUTE ON FUNCTION search_prefix_query(text[]) TO daibx_app;
    END IF;
END
$$;

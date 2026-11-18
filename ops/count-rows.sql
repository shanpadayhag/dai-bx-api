SELECT format('SELECT %L, count(*) FROM public.%I', table_class.relname, table_class.relname)
FROM pg_class AS table_class
JOIN pg_namespace AS table_namespace ON table_namespace.oid = table_class.relnamespace
WHERE table_namespace.nspname = 'public'
  AND table_class.relkind IN ('r', 'p')
ORDER BY table_class.relname
\gexec

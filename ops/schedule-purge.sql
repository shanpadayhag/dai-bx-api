CREATE EXTENSION IF NOT EXISTS pg_cron;

SELECT cron.unschedule(jobid)
FROM cron.job
WHERE jobname = 'daibx-purge-expired-auth-data';

SELECT cron.schedule(
    'daibx-purge-expired-auth-data',
    '17 3 * * *',
    'SELECT public.purge_expired_auth_data()'
);

SELECT jobid, jobname, schedule, command, active
FROM cron.job
WHERE jobname = 'daibx-purge-expired-auth-data';

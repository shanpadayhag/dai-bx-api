SELECT jobid, jobname, schedule, active
FROM cron.job
WHERE jobname = 'daibx-purge-expired-auth-data';

SELECT details.status, details.return_message, details.start_time, details.end_time
FROM cron.job_run_details AS details
JOIN cron.job AS job ON job.jobid = details.jobid
WHERE job.jobname = 'daibx-purge-expired-auth-data'
ORDER BY details.start_time DESC
LIMIT 5;

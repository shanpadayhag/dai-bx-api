# Breach procedure

What to do when DaiBX may have lost control of someone's personal data: who decides, how to stop it, what to write down, how to judge the risk, and how to tell the National Privacy Commission (NPC) and the people affected.

The law that binds is the Data Privacy Act of 2012 (RA 10173), with NPC Circular 16-03 on personal data breach management. DaiBX also holds itself to the GDPR, so the GDPR's notice duties are followed too. The NPC rules below were checked on 2026-11-19 against secondary guides and news, not the NPC's own text. Read the current circular and advisories on [privacy.gov.ph](https://privacy.gov.ph) before relying on any detail during a real incident.

## Who decides

The owner, Shan Padayhag, is the personal information controller and the data protection officer (DPO). The owner makes every decision in this procedure, and nobody else has access to production. If the owner can't act, nobody can, so the steps are written to be done by one person with a laptop.

## The clock

The 72 hours start when the owner **knows, or reasonably believes**, that a notifiable breach has happened. They don't start when the investigation ends. Write down that moment first, in the register, before doing anything else.

## 1. Contain

Stop the loss first. Each step is safe to take even if the breach later turns out to be a false alarm.

| What may be exposed | Do this |
| --- | --- |
| One account | End its sessions: `psql "$DATABASE_URL" --set=ON_ERROR_STOP=1 --set=account_email=<email> --file=ops/end-account-sessions.sql`. Ask the person to change their password and replace their authenticator. |
| Many accounts, or unknown | End every session: `psql "$DATABASE_URL" --set=ON_ERROR_STOP=1 --file=ops/end-all-sessions.sql`. |
| The website or Vercel account | Replace `ORIGIN_SECRET`: set the new value on Vercel Production and redeploy the web app, then set it on `daibx-api-prod`. Direct calls with the old value stop working at once. Expect a minute of failed requests between the two steps. |
| The database password | Replace the `daibx_app` password in Supabase, then update `DATABASE_URL` on `daibx-api-prod` and `daibx-scheduler-prod`. Replace the `postgres` password too if it may have leaked, and update the GitHub `DATABASE_URL` secret used by migrations. |
| The database and `TOTP_KEYS` together | Authenticator secrets can be decrypted. Add a new key to `TOTP_KEYS`, make it `TOTP_KEY_CURRENT`, and ask every person to replace their authenticator, because existing secrets stay readable with the old key. |
| The mail account | Revoke the Gmail app password, create a new one, and set it on both prod Lambdas. Check the account's sent mail for messages DaiBX didn't send. |
| A backup | Backups are encrypted with the age key. If only the file leaked, the data is safe while the private key is safe. If the key may have leaked, create a new key pair, set `BACKUP_AGE_PUBLIC_KEY`, and delete the old backups once a new one exists. |
| AWS, GitHub or Supabase account | Change its password, remove unknown keys, sessions and members, and check that MFA is still on. Then do the rows above for every secret that account could read. |

Run each `psql` command against production with the connection string from the password manager. Never paste a secret into a chat, an issue or a commit.

## 2. Record

Every incident gets an entry in the private register at `~/Documents/files/secrets/daibx-incident-register.md`, including false alarms and incidents that need no notice. That folder is ignored by the vault's git, so the register never reaches a repository. Write:

- when the owner learned of it, and how;
- what happened, and when it started and stopped, as far as known;
- which data and how many people may be affected;
- the risk judgement below, with the reasons;
- each containment step, with its time;
- whether the NPC and the people affected were told, when, and a copy of the notice;
- what changes so it can't happen again.

The register is the record NPC Circular 16-03 asks every controller to keep, and the source for any annual summary the NPC asks for.

## 3. Judge the risk

Under Circular 16-03, a breach must be reported when **all three** of these are true:

1. It involves sensitive personal information, or information that could be used for identity fraud.
2. There is reason to believe an unauthorised person may have got it.
3. That is likely to cause a real risk of serious harm to someone affected.

Treat the content of tasks, notes, calendar events and time entries as sensitive personal information, because people write health, money and family details into them. Treat calendar and time data as a stalking risk, because it shows where someone is and when. Password hashes, authenticator secrets and recovery codes count as information that enables fraud.

Under the GDPR, tell the supervisory authority unless the breach is unlikely to put anyone's rights at risk, and tell the people affected when the risk is high.

When unsure, report. A report that turns out unnecessary costs little, and a late one can't be fixed.

## 4. Notify

### The NPC

- **Within 72 hours** of knowing: file the notification through the NPC's Data Breach Notification Management System (DBNMS) at [dbnms.privacy.gov.ph](https://dbnms.privacy.gov.ph). A notification sent any other way isn't valid. Send what is known on time; missing details can follow.
- **Within 5 days**: send the full breach report, as the current NPC advisory says (on 2026-11-19, by email to the DBNMS admin address with the subject `FBR_<controller>_<DPO>`).
- Include the nature of the breach, the personal data that may be involved, the steps taken and planned, and the DPO's name and contact details.
- A delay is allowed only as far as needed to find the scope of the breach or stop more disclosure, and never when 100 or more people are affected.

### The people affected

- **Within the same 72 hours**, email each affected person from the DaiBX mail account, using the template below.
- If email may itself be compromised, also put a notice on `dai-bx.vercel.app`.
- Never ask people for their password or a code in a notice. The template says so.

## Notice template

Fill every `<...>`, remove what doesn't apply, and keep the plain wording.

```
Subject: DaiBX security notice: your data may have been exposed

Hello <name>,

On <date>, we learned that <what happened, in one plain sentence>.

What this means for you
<Which of your data may have been seen or taken: for example your email
address, your task notes, or your calendar. Say what was not affected.>

What we have done
<For example: we ended every signed-in session, replaced the affected
passwords and keys, and closed the gap that allowed it.>

What you should do
- Sign in at https://dai-bx.vercel.app and choose a new password.
  If you used the same password anywhere else, change it there too.
- <If authenticator secrets were affected: replace your authenticator in
  Settings.>
- Watch for emails or messages that use details from your DaiBX account.

DaiBX will never ask you for your password, a sign-in code or a
recovery code, by email or any other way.

We have reported this to the National Privacy Commission.

If you have questions, reply to this email or write to the data
protection officer, <name>, at <email>.

<name>
Data protection officer, DaiBX
```

## 5. After

- Fix the cause, with a test that would have caught it, and ship it through the normal release flow.
- Update `asvs-l2.md` and `gdpr-notes.md` if the incident showed a gap or a change in what is stored.
- Close the register entry with what changed.
- Check once a year whether the NPC expects an annual security incident summary from DaiBX, and file it through the DBNMS if so.

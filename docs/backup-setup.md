# Backup setup

Do this once. It sets up what `.github/workflows/backup.yml` needs: a read-only database role, an encryption key, a private S3 bucket in `ap-northeast-1`, and an AWS role that can only add files to it.

## 1. Check the database version

```sql
SHOW server_version;
```

The major version must match `POSTGRES_IMAGE` in the workflow. `pg_dump` refuses to read a newer server, so change the image when production moves to a newer major version. Production ran 17.11 on 2026-11-19.

## 2. Create the database role

Run this as the role that runs migrations, with `DATABASE_URL` from `.env.prod`:

```sh
psql "$DATABASE_URL" --set=ON_ERROR_STOP=1 --file=ops/create-backup-role.sql
psql "$DATABASE_URL" --command='\password daibx_backup'
```

## 3. Create the encryption key

The private key goes straight to the clipboard and never touches the disk. The public key prints in the terminal.

```sh
brew install age
age-keygen | pbcopy
```

Paste it into a new password manager item called `DaiBX backup key`, then copy something else to clear the clipboard. Store `TOTP_KEYS` and `TOTP_KEY_CURRENT` from `.env.prod` in the same place, because a restored database can't check sign-in codes without them.

## 4. Create the bucket

```sh
bucket=daibx-backups-566252491895
region=ap-northeast-1

aws s3api create-bucket --bucket "$bucket" --region "$region" \
  --create-bucket-configuration LocationConstraint="$region"
aws s3api put-public-access-block --bucket "$bucket" --region "$region" \
  --public-access-block-configuration BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true
aws s3api put-bucket-versioning --bucket "$bucket" --region "$region" \
  --versioning-configuration Status=Enabled
aws s3api put-bucket-lifecycle-configuration --bucket "$bucket" --region "$region" \
  --lifecycle-configuration '{
    "Rules": [{
      "ID": "keep-backups-30-days",
      "Status": "Enabled",
      "Filter": { "Prefix": "backups/" },
      "Expiration": { "Days": 30 },
      "NoncurrentVersionExpiration": { "NoncurrentDays": 1 },
      "AbortIncompleteMultipartUpload": { "DaysAfterInitiation": 1 }
    }]
  }'
aws s3api put-bucket-policy --bucket "$bucket" --region "$region" \
  --policy '{
    "Version": "2012-10-17",
    "Statement": [{
      "Sid": "RequireTls",
      "Effect": "Deny",
      "Principal": "*",
      "Action": "s3:*",
      "Resource": ["arn:aws:s3:::daibx-backups-566252491895", "arn:aws:s3:::daibx-backups-566252491895/*"],
      "Condition": { "Bool": { "aws:SecureTransport": "false" } }
    }]
  }'
```

## 5. Create the write-only role

GitHub's OIDC provider already exists in the account for the deploy roles. Only the `backup` environment of this repository can assume this role.

```sh
aws iam create-role --role-name daibx-backup-writer \
  --assume-role-policy-document '{
    "Version": "2012-10-17",
    "Statement": [{
      "Effect": "Allow",
      "Principal": { "Federated": "arn:aws:iam::566252491895:oidc-provider/token.actions.githubusercontent.com" },
      "Action": "sts:AssumeRoleWithWebIdentity",
      "Condition": {
        "StringEquals": {
          "token.actions.githubusercontent.com:aud": "sts.amazonaws.com",
          "token.actions.githubusercontent.com:sub": "repo:shanpadayhag/dai-bx-api:environment:backup"
        }
      }
    }]
  }'
aws iam put-role-policy --role-name daibx-backup-writer --policy-name add-backups-only \
  --policy-document '{
    "Version": "2012-10-17",
    "Statement": [{
      "Effect": "Allow",
      "Action": "s3:PutObject",
      "Resource": "arn:aws:s3:::daibx-backups-566252491895/backups/*"
    }]
  }'
```

Confirm what it can do. Only `s3:PutObject` may say `allowed`:

```sh
aws iam simulate-principal-policy \
  --policy-source-arn arn:aws:iam::566252491895:role/daibx-backup-writer \
  --action-names s3:PutObject s3:GetObject s3:DeleteObject s3:ListBucket \
  --resource-arns 'arn:aws:s3:::daibx-backups-566252491895/backups/test' \
  --query 'EvaluationResults[].[EvalActionName,EvalDecision]' --output table
```

## 6. Create the GitHub environment

In the repository's **Settings → Environments**, create `backup` and limit its deployment branches to `master`. Add:

- secret `BACKUP_DATABASE_URL`: the session pooler URL with the `daibx_backup` user and its password. The backup script adds the certificate check itself, so the URL needs no `sslmode`;
- variable `BACKUP_AGE_PUBLIC_KEY`: the `age1...` public key from step 3.

## 7. Run it

Once the workflow is on `master`, start **Nightly backup** from the Actions tab and confirm a file appears:

```sh
aws s3 ls s3://daibx-backups-566252491895/backups/ --region ap-northeast-1
```

To prove the failure email, set `BACKUP_AGE_PUBLIC_KEY` to `invalid`, run it again, confirm the failure email arrives, and put the real key back. Then follow `ops/restore-backup.md` for the drill.

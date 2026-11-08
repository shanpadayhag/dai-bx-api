# Scheduler setup

`daibx-scheduler` is the second Lambda in this repository. EventBridge Scheduler calls it once a minute, and each call runs the card schedules that are due: it moves each card to the top of its target list and records the run. It has no HTTP address. It connects to the same database as `daibx-api`, with the same credentials.

Set it up once per environment, before pushing a commit whose deploy workflow uploads it: the deploy only updates the function's code and fails if the function doesn't exist yet. Every step runs in `ap-southeast-1`, in account `566252491895`. Replace `<env>` with `dev` or `prod` throughout.

## 1. Build it

```sh
cargo lambda build --release --arm64 --output-format zip --bin daibx-scheduler
```

The zip lands at `target/lambda/daibx-scheduler/bootstrap.zip`.

## 2. Create its execution role

The function only needs to write its logs.

```sh
aws iam create-role \
  --role-name daibx-scheduler-<env> \
  --assume-role-policy-document '{
    "Version": "2012-10-17",
    "Statement": [{
      "Effect": "Allow",
      "Principal": { "Service": "lambda.amazonaws.com" },
      "Action": "sts:AssumeRole"
    }]
  }'
aws iam attach-role-policy \
  --role-name daibx-scheduler-<env> \
  --policy-arn arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole
```

## 3. Create the function

It needs `DATABASE_URL` (the same value `daibx-api-<env>` uses, from `.env.<env>`) and `RUST_LOG`. Pass the settings through a file readable only by you, and delete it afterwards, so the connection string stays out of your shell history.

```sh
umask 077
printf '{"Variables":{"DATABASE_URL":"%s","RUST_LOG":"info"}}' "$DATABASE_URL" > scheduler-env.json
aws lambda create-function \
  --function-name daibx-scheduler-<env> \
  --runtime provided.al2023 \
  --architectures arm64 \
  --handler bootstrap \
  --memory-size 256 \
  --timeout 60 \
  --role arn:aws:iam::566252491895:role/daibx-scheduler-<env> \
  --zip-file fileb://target/lambda/daibx-scheduler/bootstrap.zip \
  --environment file://scheduler-env.json
rm scheduler-env.json
aws lambda wait function-active-v2 --function-name daibx-scheduler-<env>
```

A call handles at most 100 due schedules. 60 seconds leaves room for all of them on a cold start.

## 4. Let the deploy role update it

The deploy role `daibx-api-<env>-deploy` may update only `daibx-api-<env>` so far. Give it the scheduler too, as a second inline policy:

```sh
aws iam put-role-policy \
  --role-name daibx-api-<env>-deploy \
  --policy-name daibx-scheduler-deploy \
  --policy-document '{
    "Version": "2012-10-17",
    "Statement": [{
      "Effect": "Allow",
      "Action": ["lambda:UpdateFunctionCode", "lambda:GetFunction", "lambda:GetFunctionConfiguration"],
      "Resource": "arn:aws:lambda:ap-southeast-1:566252491895:function:daibx-scheduler-<env>"
    }]
  }'
```

## 5. Create the invoke role

Only EventBridge Scheduler, acting for this account, may call the function.

```sh
aws iam create-role \
  --role-name daibx-scheduler-<env>-invoke \
  --assume-role-policy-document '{
    "Version": "2012-10-17",
    "Statement": [{
      "Effect": "Allow",
      "Principal": { "Service": "scheduler.amazonaws.com" },
      "Action": "sts:AssumeRole",
      "Condition": { "StringEquals": { "aws:SourceAccount": "566252491895" } }
    }]
  }'
aws iam put-role-policy \
  --role-name daibx-scheduler-<env>-invoke \
  --policy-name invoke-daibx-scheduler \
  --policy-document '{
    "Version": "2012-10-17",
    "Statement": [{
      "Effect": "Allow",
      "Action": "lambda:InvokeFunction",
      "Resource": "arn:aws:lambda:ap-southeast-1:566252491895:function:daibx-scheduler-<env>"
    }]
  }'
```

## 6. Create the schedule

```sh
aws scheduler create-schedule \
  --name daibx-scheduler-<env> \
  --schedule-expression 'rate(1 minute)' \
  --flexible-time-window Mode=OFF \
  --target '{
    "Arn": "arn:aws:lambda:ap-southeast-1:566252491895:function:daibx-scheduler-<env>",
    "RoleArn": "arn:aws:iam::566252491895:role/daibx-scheduler-<env>-invoke",
    "RetryPolicy": { "MaximumRetryAttempts": 0 }
  }'
```

Retries are off because the next minute's call is the retry: a run that failed leaves its schedule due.

## 7. Check it

Call it once by hand, then read its latest log lines:

```sh
aws lambda invoke --function-name daibx-scheduler-<env> response.json && cat response.json && rm response.json
aws logs tail /aws/lambda/daibx-scheduler-<env> --since 5m
```

The answer looks like `{"ranCount":0,"failedCount":0}`, and the log shows a `due schedules ran` line every minute. A run that fails logs `a due schedule failed to run` with the schedule's id.

To stop the scheduler without deleting anything:

```sh
aws scheduler update-schedule --name daibx-scheduler-<env> --state DISABLED \
  --schedule-expression 'rate(1 minute)' --flexible-time-window Mode=OFF \
  --target '{"Arn":"arn:aws:lambda:ap-southeast-1:566252491895:function:daibx-scheduler-<env>","RoleArn":"arn:aws:iam::566252491895:role/daibx-scheduler-<env>-invoke"}'
```

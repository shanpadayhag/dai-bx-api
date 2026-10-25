use std::process::{Command, Output};

use daibx_api::features::invites::actions::redeem_invite::RedeemInvite;
use sqlx::PgPool;
use uuid::Uuid;

const INVITE_OPERATOR_TEST_PASSWORD: &str = "invite-operator-test-password";

async fn invite_operator_url(db_pool: &PgPool) -> String {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "ALTER ROLE invite_operator WITH PASSWORD '{INVITE_OPERATOR_TEST_PASSWORD}'"
    )))
    .execute(db_pool)
    .await
    .expect("the test database user should be able to set the operator password");
    let connect_options = db_pool.connect_options();
    format!(
        "postgres://invite_operator:{INVITE_OPERATOR_TEST_PASSWORD}@{}:{}/{}",
        connect_options.get_host(),
        connect_options.get_port(),
        connect_options
            .get_database()
            .expect("the test pool should name its database"),
    )
}

fn run_invite_cli(cli_arguments: &[&str], invite_database_url: Option<&str>) -> Output {
    let mut invite_command = Command::new(env!("CARGO_BIN_EXE_invite"));
    invite_command
        .args(cli_arguments)
        .env_remove("DATABASE_URL")
        .env_remove("INVITE_DATABASE_URL");
    if let Some(database_url) = invite_database_url {
        invite_command.env("INVITE_DATABASE_URL", database_url);
    }
    invite_command.output().expect("the invite CLI should run")
}

fn printed_invite_code(cli_output: &Output) -> String {
    String::from_utf8_lossy(&cli_output.stdout)
        .lines()
        .find_map(|output_line| output_line.strip_prefix("Invite code: "))
        .expect("the CLI should print the invite code")
        .trim()
        .to_owned()
}

async fn seed_redeeming_user(db_pool: &PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (id, security_ref, name, email, password_hash, terms_version, terms_presented_at)
         VALUES ($1, gen_random_uuid(), 'Invited Person', $1::text || '@daibx.test', 'unused', 'draft-1', now())
         RETURNING id",
    )
    .bind(Uuid::now_v7())
    .fetch_one(db_pool)
    .await
    .expect("seed user insert should succeed")
}

#[sqlx::test]
async fn create_prints_a_code_that_redeems_using_only_the_operator_role(db_pool: PgPool) {
    let operator_url = invite_operator_url(&db_pool).await;

    let cli_output = run_invite_cli(&["create"], Some(&operator_url));

    assert!(
        cli_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&cli_output.stderr)
    );
    let stdout_text = String::from_utf8_lossy(&cli_output.stdout);
    assert!(stdout_text.contains("Expires: ") && stdout_text.contains(" UTC"));
    let invite_code = printed_invite_code(&cli_output);
    assert_eq!(invite_code.len(), 19);
    let redeeming_user_id = seed_redeeming_user(&db_pool).await;
    RedeemInvite::new()
        .execute(&db_pool, &invite_code, redeeming_user_id)
        .await
        .expect("the printed code should redeem, so only its hash was stored");
}

#[test]
fn explains_its_usage_when_the_command_is_missing_or_unknown() {
    for cli_arguments in [&[][..], &["delete"][..]] {
        let cli_output = run_invite_cli(cli_arguments, None);

        assert_eq!(cli_output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&cli_output.stderr).contains("usage: invite create"));
    }
}

#[test]
fn refuses_to_run_without_the_operator_connection_string() {
    let cli_output = run_invite_cli(&["create"], None);

    assert_eq!(cli_output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&cli_output.stderr).contains("INVITE_DATABASE_URL"));
}

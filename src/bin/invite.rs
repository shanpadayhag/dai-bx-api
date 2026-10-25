use std::env;
use std::process::ExitCode;
use std::time::Duration;

use daibx_api::features::invites::actions::create_invite::CreateInvite;
use sqlx::postgres::PgPoolOptions;

const USAGE: &str = "usage: invite create\n\nCreates a single-use invite code that expires in 7 days.\nConnects with INVITE_DATABASE_URL, the invite_operator role's connection string.";
const DATABASE_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::main]
async fn main() -> ExitCode {
    let cli_arguments: Vec<String> = env::args().skip(1).collect();
    if cli_arguments.len() != 1 || cli_arguments[0] != "create" {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let Some(invite_database_url) = env::var("INVITE_DATABASE_URL")
        .ok()
        .filter(|database_url| !database_url.trim().is_empty())
    else {
        eprintln!("INVITE_DATABASE_URL must be set to the invite_operator connection string");
        return ExitCode::from(1);
    };
    match create_invite(&invite_database_url).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure_message) => {
            eprintln!("could not create an invite: {failure_message}");
            ExitCode::from(1)
        }
    }
}

async fn create_invite(invite_database_url: &str) -> Result<(), String> {
    let operator_pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(DATABASE_ACQUIRE_TIMEOUT)
        .connect(invite_database_url)
        .await
        .map_err(|connect_error| connect_error.to_string())?;
    let created_invite = CreateInvite::new(&operator_pool)
        .execute()
        .await
        .map_err(|invite_error| invite_error.to_string())?;
    println!("Invite code: {}", created_invite.invite_code.as_str());
    println!("Expires: {}", created_invite.expires_at_utc);
    println!("It works for one sign-up. This is the only time it's shown.");
    Ok(())
}

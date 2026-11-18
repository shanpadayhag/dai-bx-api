use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgSslMode};

const LOCAL_DATABASE_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];
const SUPABASE_ROOT_CERTIFICATE_PEM: &[u8] = include_bytes!("../../ops/supabase-root-ca.pem");

pub fn database_connect_options(database_url: &str) -> Result<PgConnectOptions, sqlx::Error> {
    let connect_options = PgConnectOptions::from_str(database_url)?;
    if connect_options.get_socket().is_some()
        || LOCAL_DATABASE_HOSTS.contains(&connect_options.get_host())
    {
        return Ok(connect_options);
    }
    Ok(require_verified_tls(connect_options))
}

fn require_verified_tls(connect_options: PgConnectOptions) -> PgConnectOptions {
    connect_options
        .ssl_mode(PgSslMode::VerifyFull)
        .ssl_root_cert_from_pem(SUPABASE_ROOT_CERTIFICATE_PEM.to_vec())
}

#[cfg(test)]
mod tests {
    use sqlx::postgres::PgConnection;
    use sqlx::{Connection, Error};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    const REMOTE_DATABASE_URL: &str = "postgres://app:secret@db.pooler.supabase.com:5432/postgres";

    #[test]
    fn verifies_the_certificate_of_a_remote_database() {
        let connect_options =
            database_connect_options(REMOTE_DATABASE_URL).expect("the address should parse");

        assert!(matches!(
            connect_options.get_ssl_mode(),
            PgSslMode::VerifyFull
        ));
    }

    #[test]
    fn a_remote_address_cannot_turn_verification_off() {
        let connect_options =
            database_connect_options(&format!("{REMOTE_DATABASE_URL}?sslmode=disable"))
                .expect("the address should parse");

        assert!(matches!(
            connect_options.get_ssl_mode(),
            PgSslMode::VerifyFull
        ));
    }

    #[test]
    fn leaves_a_local_database_as_configured() {
        for local_host in ["localhost", "127.0.0.1", "[::1]"] {
            let connect_options = database_connect_options(&format!(
                "postgres://app@{local_host}:5432/daibx?sslmode=disable"
            ))
            .expect("the address should parse");

            assert!(
                matches!(connect_options.get_ssl_mode(), PgSslMode::Disable),
                "{local_host} should keep its own setting"
            );
        }
    }

    #[tokio::test]
    async fn stops_before_sending_anything_when_the_server_refuses_tls() {
        let database_listener = TcpListener::bind("127.0.0.1:0").await.expect("a free port");
        let database_port = database_listener.local_addr().expect("the port").port();
        let refusing_server = tokio::spawn(async move {
            let (mut database_stream, _) = database_listener.accept().await.expect("a connection");
            let mut tls_request = [0_u8; 8];
            database_stream
                .read_exact(&mut tls_request)
                .await
                .expect("the TLS request");
            database_stream.write_all(b"N").await.expect("the refusal");
            let mut bytes_after_refusal = [0_u8; 64];
            database_stream
                .read(&mut bytes_after_refusal)
                .await
                .unwrap_or(0)
        });
        let connect_options = require_verified_tls(
            PgConnectOptions::new()
                .host("127.0.0.1")
                .port(database_port)
                .username("app")
                .password("secret"),
        );

        let connect_result = PgConnection::connect_with(&connect_options).await;

        assert!(matches!(connect_result, Err(Error::Tls(_))));
        assert_eq!(refusing_server.await.expect("the server task"), 0);
    }
}

use sqlx::PgPool;
use zeroize::Zeroizing;

use crate::core::request_context::RequestContext;
use crate::core::security::password_hash::hash_password;
use crate::core::state::AppState;
use crate::features::auth::error::AuthError;
use crate::features::categories::actions::create_default_categories::CreateDefaultCategories;
use crate::features::email::actions::send_account_email::SendAccountEmail;
use crate::features::email::entities::account_email::AccountEmailKind;
use crate::features::email_tokens::actions::issue_email_token::IssueEmailToken;
use crate::features::email_tokens::entities::email_token::EmailTokenPurpose;
use crate::features::invites::actions::check_invite_usable::CheckInviteUsable;
use crate::features::invites::actions::redeem_invite::RedeemInvite;
use crate::features::password_policy::actions::check_new_password::CheckNewPassword;
use crate::features::password_policy::entities::new_password::PasswordContext;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::users::actions::create_user::CreateUser;
use crate::features::users::entities::user::{NewUser, User};

pub struct SignUpDetails<'a> {
    pub invite_code: Option<&'a str>,
    pub name: &'a str,
    pub email: &'a str,
    pub password: Zeroizing<String>,
}

pub struct SignUp<'a> {
    db_pool: &'a PgPool,
    is_open_signup: bool,
    terms_version: &'a str,
    check_new_password: CheckNewPassword<'a>,
    send_account_email: SendAccountEmail<'a>,
    create_user: CreateUser,
    create_default_categories: CreateDefaultCategories,
    check_invite_usable: CheckInviteUsable,
    redeem_invite: RedeemInvite,
    record_security_event: RecordSecurityEvent,
    issue_email_token: IssueEmailToken,
}

impl<'a> SignUp<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        let app_config = &app_state.app_config;
        Self {
            db_pool: &app_state.db_pool,
            is_open_signup: app_config.open_signups,
            terms_version: &app_config.terms_version,
            check_new_password: CheckNewPassword::new(
                &app_state.http_client,
                &app_config.hibp_api_base_url,
            ),
            send_account_email: SendAccountEmail::new(
                &app_state.mail_transport,
                &app_config.smtp_settings.mail_from,
                &app_config.app_origin,
            ),
            create_user: CreateUser::new(),
            create_default_categories: CreateDefaultCategories::new(),
            check_invite_usable: CheckInviteUsable::new(),
            redeem_invite: RedeemInvite::new(),
            record_security_event: RecordSecurityEvent::new(),
            issue_email_token: IssueEmailToken::new(),
        }
    }

    pub async fn execute(
        &self,
        sign_up_details: SignUpDetails<'_>,
        request_context: &RequestContext,
    ) -> Result<(), AuthError> {
        let invite_code = self.required_invite_code(sign_up_details.invite_code)?;
        self.check_new_password
            .execute(
                &sign_up_details.password,
                PasswordContext {
                    person_name: sign_up_details.name,
                    email: sign_up_details.email,
                },
            )
            .await?;
        let password_hash = hash_password(sign_up_details.password).await?;
        let new_user = NewUser {
            name: sign_up_details.name.to_owned(),
            email: sign_up_details.email.to_owned(),
            password_hash,
            terms_version: self.terms_version.to_owned(),
        };

        let mut sign_up_transaction = self.db_pool.begin().await?;
        let Some(created_user) = self
            .create_user
            .execute(&mut *sign_up_transaction, &new_user)
            .await?
        else {
            if let Some(invite_code) = invite_code {
                self.check_invite_usable
                    .execute(&mut *sign_up_transaction, invite_code)
                    .await?;
            }
            return Ok(());
        };
        self.create_default_categories
            .execute(&mut *sign_up_transaction, created_user.id)
            .await?;
        if let Some(invite_code) = invite_code {
            self.redeem_invite
                .execute(&mut *sign_up_transaction, invite_code, created_user.id)
                .await?;
        }
        self.record(
            &mut sign_up_transaction,
            SecurityEventType::SignUp,
            &created_user,
            request_context,
        )
        .await?;
        if invite_code.is_some() {
            self.record(
                &mut sign_up_transaction,
                SecurityEventType::InviteUsed,
                &created_user,
                request_context,
            )
            .await?;
        }
        let link_token = self
            .issue_email_token
            .execute(
                &mut sign_up_transaction,
                created_user.id,
                EmailTokenPurpose::VerifyEmail,
                None,
            )
            .await?;
        self.send_account_email
            .execute(
                AccountEmailKind::VerifyEmail,
                &created_user.email,
                &link_token,
            )
            .await?;
        sign_up_transaction.commit().await?;
        Ok(())
    }

    fn required_invite_code<'b>(
        &self,
        invite_code: Option<&'b str>,
    ) -> Result<Option<&'b str>, AuthError> {
        if self.is_open_signup {
            return Ok(None);
        }
        invite_code
            .map(str::trim)
            .filter(|entered_code| !entered_code.is_empty())
            .map(Some)
            .ok_or(AuthError::InvalidInvite)
    }

    async fn record(
        &self,
        connection: &mut sqlx::PgConnection,
        event_type: SecurityEventType,
        user: &User,
        request_context: &RequestContext,
    ) -> Result<(), AuthError> {
        self.record_security_event
            .execute(
                connection,
                &NewSecurityEvent {
                    event_type,
                    subject_ref: Some(user.security_ref),
                    client_ip: request_context.client_ip,
                    user_agent: request_context.user_agent.clone(),
                    details: SecurityEventDetails::default(),
                },
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sqlx::PgPool;

    use super::*;
    use crate::core::config::app_config::AppConfig;
    use crate::core::config::smtp_settings::{SmtpSecurity, SmtpSettings};
    use crate::core::http_client::build_http_client;
    use crate::core::mail_transport::build_mail_transport;
    use crate::core::security::totp_secret_cipher::TotpKeyring;
    use crate::features::invites::actions::create_invite::CreateInvite;
    use crate::features::users::actions::find_user_by_email::FindUserByEmail;

    fn app_state_with_unreachable_mail(db_pool: PgPool) -> AppState {
        let smtp_settings = SmtpSettings {
            host: String::from("127.0.0.1"),
            port: 1,
            security: SmtpSecurity::Plain,
            credentials: None,
            mail_from: "DaiBX <no-reply@daibx.test>"
                .parse()
                .expect("sender should parse"),
        };
        AppState {
            db_pool,
            mail_transport: build_mail_transport(&smtp_settings),
            http_client: build_http_client(),
            app_config: Arc::new(AppConfig {
                database_url: String::from("postgres://unused"),
                allowed_app_origins: Vec::new(),
                totp_keyring: TotpKeyring::parse(
                    "v1:AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
                    "v1",
                )
                .expect("test keyring should parse"),
                hibp_api_base_url: String::from("http://127.0.0.1:9"),
                app_origin: String::from("http://localhost:3000"),
                smtp_settings,
                open_signups: false,
                terms_version: String::from("test-terms-1"),
            }),
        }
    }

    #[sqlx::test]
    async fn leaves_no_account_behind_when_the_verification_email_cannot_be_sent(db_pool: PgPool) {
        let app_state = app_state_with_unreachable_mail(db_pool.clone());
        let created_invite = CreateInvite::new(&db_pool)
            .execute()
            .await
            .expect("creating an invite should succeed");

        let sign_up_result = SignUp::new(&app_state)
            .execute(
                SignUpDetails {
                    invite_code: Some(&created_invite.invite_code),
                    name: "Shan Padayhag",
                    email: "person@example.com",
                    password: Zeroizing::new(String::from(
                        "violet lantern drifts past seven quiet docks",
                    )),
                },
                &RequestContext::default(),
            )
            .await;

        assert!(matches!(sign_up_result, Err(AuthError::Internal(_))));
        assert_eq!(
            FindUserByEmail::new(&db_pool)
                .execute("person@example.com")
                .await
                .expect("lookup should succeed"),
            None
        );
        CheckInviteUsable::new()
            .execute(&db_pool, &created_invite.invite_code)
            .await
            .expect("the invite should still be usable after the rollback");
    }
}

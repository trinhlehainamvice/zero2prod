use crate::authentication::{validate_credentials, AuthError, Credentials, HmacSecret};
use crate::error_chain_fmt;
use actix_web::cookie::Cookie;
use actix_web::error::InternalError;
use actix_web::{web, HttpResponse};
use hmac::{Hmac, Mac};
use reqwest::header::LOCATION;
use secrecy::{ExposeSecret, Secret};
use sha2::Sha256;
use sqlx::PgPool;
use std::fmt::Debug;

#[derive(thiserror::Error)]
pub enum LoginError {
    #[error("Invalid Username or Password")]
    AuthFailed(#[source] anyhow::Error),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl Debug for LoginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

#[derive(serde::Deserialize)]
pub struct UserLoginForm {
    username: String,
    password: Secret<String>,
}

#[tracing::instrument(
    name = "Login a user input", 
    skip(login_form, pg_pool, hmac_secret),
    fields(
    username=tracing::field::Empty,
    user_id=tracing::field::Empty
    )
)]
pub async fn login(
    web::Form(login_form): web::Form<UserLoginForm>,
    pg_pool: web::Data<PgPool>,
    hmac_secret: web::Data<HmacSecret>,
) -> Result<HttpResponse, InternalError<LoginError>> {
    let credentials = Credentials {
        username: login_form.username,
        password: login_form.password,
    };
    tracing::Span::current().record("username", tracing::field::display(&credentials.username));

    match validate_credentials(&pg_pool, credentials).await {
        Ok(user_id) => {
            tracing::Span::current().record("user_id", tracing::field::display(&user_id));
            Ok(HttpResponse::SeeOther()
                .insert_header((LOCATION, "/"))
                .finish())
        }
        Err(error) => {
            let error = match error {
                AuthError::InvalidCredentials(_) => LoginError::AuthFailed(error.into()),
                AuthError::UnexpectedError(_) => LoginError::UnexpectedError(error.into()),
            };

            let query_string = format!("error={}", error);

            let hmac_tag = {
                let mut mac = Hmac::<Sha256>::new_from_slice(
                    hmac_secret.as_ref().0.expose_secret().as_bytes(),
                )
                .unwrap();
                mac.update(query_string.as_bytes());
                mac.finalize().into_bytes()
            };

            let flash_message = serde_json::json!({
                "error": error.to_string(),
                "tag": format!("{:x}", hmac_tag)
            });

            let response = HttpResponse::SeeOther()
                .insert_header((LOCATION, "/login"))
                .cookie(Cookie::new("_flash", flash_message.to_string()))
                .finish();

            Err(InternalError::from_response(error, response))
        }
    }
}

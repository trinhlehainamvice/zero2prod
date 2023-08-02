use crate::authentication::{validate_credentials, AuthError, Credentials};
use crate::error_chain_fmt;
use actix_session::Session;
use actix_web::http::StatusCode;
use actix_web::{web, HttpResponse, ResponseError};
use actix_web_flash_messages::FlashMessage;
use reqwest::header::LOCATION;
use secrecy::Secret;
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

impl ResponseError for LoginError {
    fn status_code(&self) -> StatusCode {
        StatusCode::SEE_OTHER
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::SeeOther()
            .insert_header((LOCATION, "/login"))
            .finish()
    }
}

#[derive(serde::Deserialize)]
pub struct UserLoginForm {
    username: String,
    password: Secret<String>,
}

#[tracing::instrument(
    name = "Login a user input", 
    skip(login_form, pg_pool, session),
    fields(
    username=tracing::field::Empty,
    user_id=tracing::field::Empty
    )
)]
pub async fn login(
    web::Form(login_form): web::Form<UserLoginForm>,
    pg_pool: web::Data<PgPool>,
    session: Session,
) -> Result<HttpResponse, LoginError> {
    let credentials = Credentials {
        username: login_form.username,
        password: login_form.password,
    };
    tracing::Span::current().record("username", tracing::field::display(&credentials.username));

    match validate_credentials(&pg_pool, credentials).await {
        Ok(user_id) => {
            tracing::Span::current().record("user_id", tracing::field::display(&user_id));
            session
                .insert("user_id", user_id)
                .map_err(|e| LoginError::UnexpectedError(anyhow::anyhow!(e)))?;
            Ok(HttpResponse::SeeOther()
                .insert_header((LOCATION, "/admin/dashboard"))
                .finish())
        }
        Err(error) => {
            let error = match error {
                AuthError::InvalidCredentials(_) => LoginError::AuthFailed(error.into()),
                AuthError::UnexpectedError(_) => LoginError::UnexpectedError(error.into()),
            };

            FlashMessage::error(error.to_string()).send();

            Err(error)
        }
    }
}

use crate::authentication::{validate_credentials, AuthError, Credentials};
use crate::error_chain_fmt;
use actix_web::http::StatusCode;
use actix_web::{web, HttpResponse, ResponseError};
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
        let encoded_url_error = urlencoding::Encoded::new(self.to_string());
        HttpResponse::build(self.status_code())
            .insert_header((LOCATION, format!("/login?error={}", encoded_url_error)))
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
    skip(login_form, pg_pool),
    fields(
    username=tracing::field::Empty,
    user_id=tracing::field::Empty
    )
)]
pub async fn login(
    web::Form(login_form): web::Form<UserLoginForm>,
    pg_pool: web::Data<PgPool>,
) -> Result<HttpResponse, LoginError> {
    let credentials = Credentials {
        username: login_form.username,
        password: login_form.password,
    };
    tracing::Span::current().record("username", tracing::field::display(&credentials.username));

    let user_id = validate_credentials(&pg_pool, credentials)
        .await
        .map_err(|e| match e {
            AuthError::InvalidCredentials(_) => LoginError::AuthFailed(e.into()),
            AuthError::UnexpectedError(_) => LoginError::UnexpectedError(e.into()),
        })?;
    tracing::Span::current().record("user_id", tracing::field::display(&user_id));

    Ok(HttpResponse::SeeOther()
        .insert_header((LOCATION, "/"))
        .finish())
}

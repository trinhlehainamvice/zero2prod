use crate::utils::{error_chain_fmt, spawn_blocking_task_with_tracing};
use actix_session::{Session, SessionExt, SessionGetError, SessionInsertError};
use actix_web::dev::Payload;
use actix_web::http::header::HeaderMap;
use actix_web::{FromRequest, HttpRequest};
use anyhow::Context;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version};
use base64::Engine;
use secrecy::{ExposeSecret, Secret};
use sqlx::PgPool;
use std::fmt::Debug;
use std::future::{ready, Ready};
use uuid::Uuid;

#[derive(thiserror::Error)]
pub enum AuthError {
    #[error("Invalid Credentials")]
    InvalidCredentials(#[source] anyhow::Error),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl Debug for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

pub struct Credentials {
    pub username: String,
    pub password: Secret<String>,
}

pub struct UserSession(Session);

impl UserSession {
    const USER_ID_KEY: &'static str = "user_id";

    pub fn new(session: Session) -> Self {
        Self(session)
    }

    pub fn renew(&self) {
        self.0.renew();
    }

    pub fn insert_user_id(&self, user_id: Uuid) -> Result<(), SessionInsertError> {
        self.0.insert(Self::USER_ID_KEY, user_id)
    }

    pub fn get_user_id(&self) -> Result<Option<Uuid>, SessionGetError> {
        self.0.get(Self::USER_ID_KEY)
    }

    pub fn logout(&self) {
        self.0.purge();
    }
}

// Implement Extract for TypedSession
impl FromRequest for UserSession {
    type Error = <Session as FromRequest>::Error;
    // Request is performed in asynchronous context
    // So we need to wrap our extracted session into a future even we don't perform any I/O process
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        ready(Ok(UserSession::new(req.get_session())))
    }
}

#[tracing::instrument(name = "Extract credentials from Request header", skip_all)]
pub fn get_credentials_from_basic_auth(header: &HeaderMap) -> Result<Credentials, anyhow::Error> {
    // Basic Authorization Template: "Authorization:Basic <base64encoded_string_segments>"
    // Get the `Authorization` header with valid UTF8 string
    let header_value = header
        .get("Authorization")
        .context("No `Authorization` header found")?
        .to_str()
        .context("`Authorization` header's value is not valid UTF8")?;

    let base64encoded_segment = header_value
        // Remove `Basic` and Padding ` ` -> `Basic `
        .strip_prefix("Basic ")
        .context("`Authorization` header's does not start with `Basic `")?;

    let decoded_bytes = base64::engine::general_purpose::STANDARD
        .decode(base64encoded_segment)
        .context("Failed to base64 decode value `Basic Authorization` header's value to bytes")?;

    let decoded_credentials =
        String::from_utf8(decoded_bytes).context("decoded bytes is not valid UTF8")?;

    let mut credentials = decoded_credentials.splitn(2, ':');
    let username = credentials
        .next()
        .context("Decoded credentials do not contain a username")?
        .to_string();
    let password = credentials
        .next()
        .context("Decoded credentials do not contain a password")?
        .to_string();

    Ok(Credentials {
        username,
        password: Secret::new(password),
    })
}

#[tracing::instrument(name = "Get credentials from database", skip_all)]
async fn get_credentials_from_database(
    pg_pool: &PgPool,
    username: &str,
) -> Result<Option<(Uuid, Secret<String>)>, anyhow::Error> {
    let credentials = sqlx::query!(
        r#"
        SELECT user_id, password_hash
        FROM users
        WHERE username = $1
        "#,
        username
    )
    .fetch_optional(pg_pool)
    .await
    .context("Failed to fetch credentials from database")?
    .map(|row| (row.user_id, Secret::new(row.password_hash)));

    Ok(credentials)
}

#[tracing::instrument(name = "Validate credentials from database", skip_all)]
pub async fn validate_credentials(
    pg_pool: &PgPool,
    credentials: Credentials,
) -> Result<Uuid, AuthError> {
    const HASHED_PASSWORD_IF_INVALID_USERNAME: &str = "$argon2d$v=19$m=15000,t=2,p=1\
        $QhQyHN2/VvKTi5QYqo+VZA\
        $JkXwR/rdESxDi2DfcCf8lk2U4+ShyN3CXZATJQvP0lg";
    let mut user_id = None;
    let mut expected_password_hash = Secret::new(HASHED_PASSWORD_IF_INVALID_USERNAME.to_string());

    if let Some((stored_user_id, stored_password_hash)) =
        get_credentials_from_database(pg_pool, &credentials.username)
            .await
            .map_err(AuthError::UnexpectedError)?
    {
        user_id = Some(stored_user_id);
        expected_password_hash = stored_password_hash;
    }

    // Always verify password hash even if username is invalid
    // Prevent timing attack to guest valid username from database
    spawn_blocking_task_with_tracing(move || {
        verify_password_hash(credentials.password, expected_password_hash)
    })
    .await
    .context("Failed to spawn blocking task")
    .map_err(AuthError::UnexpectedError)??;

    // Validation is satisfied when both user_id and password hash_are valid
    user_id.ok_or_else(|| {
        AuthError::InvalidCredentials(anyhow::anyhow!("Invalid username or password"))
    })
}

#[tracing::instrument(name = "Verify password hash", skip_all)]
pub fn verify_password_hash(
    password: Secret<String>,
    expected_password_hash: Secret<String>,
) -> Result<(), AuthError> {
    let parsed_hash = PasswordHash::new(expected_password_hash.expose_secret())
        .map_err(|e| AuthError::UnexpectedError(anyhow::anyhow!(e)))?;

    Argon2::default()
        .verify_password(password.expose_secret().as_bytes(), &parsed_hash)
        .context("Failed to verify password hash")
        .map_err(AuthError::InvalidCredentials)
}

pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let params = Params::new(15000, 2, 1, None).expect("Fail to create Argon Params");
    let hasher = Argon2::new(Algorithm::Argon2d, Version::V0x13, params);
    let new_password_hash = hasher
        .hash_password(password.as_bytes(), salt.as_salt())
        .context("Failed to hash password")
        .map_err(AuthError::UnexpectedError)?;

    Ok(new_password_hash.to_string())
}

#[tracing::instrument(name = "Update new user's password_hash to database", skip_all)]
pub async fn update_user_password_to_database(
    user_id: &Uuid,
    new_password_hash: &str,
    pg_pool: &PgPool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        UPDATE users
        SET password_hash = $1
        WHERE user_id = $2
        "#,
        new_password_hash,
        user_id
    )
    .execute(pg_pool)
    .await?;
    Ok(())
}

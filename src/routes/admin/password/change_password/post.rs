use crate::authentication::{
    hash_password, update_user_password_to_database, validate_credentials, Credentials, UserId,
};
use crate::utils;
use crate::utils::{e500, get_username_from_database, see_other};
use actix_web::{web, HttpResponse};
use actix_web_flash_messages::FlashMessage;
use anyhow::Context;
use secrecy::{ExposeSecret, Secret};
use sqlx::PgPool;

#[derive(serde::Deserialize)]
pub struct ChangePasswordForm {
    pub current_password: Secret<String>,
    pub new_password: Secret<String>,
    pub confirm_password: Secret<String>,
}

pub async fn change_password(
    user_id: web::ReqData<UserId>,
    pg_pool: web::Data<PgPool>,
    web::Form(change_pwd_form): web::Form<ChangePasswordForm>,
) -> Result<HttpResponse, actix_web::Error> {
    let ChangePasswordForm {
        current_password,
        new_password,
        confirm_password,
    } = change_pwd_form;

    if new_password.expose_secret() != confirm_password.expose_secret() {
        FlashMessage::error("New passwords don't match").send();
        return Ok(see_other("/admin/password"));
    }

    if current_password.expose_secret() == new_password.expose_secret() {
        FlashMessage::error("New password must be different with current password").send();
        return Ok(see_other("/admin/password"));
    }

    let username = get_username_from_database(&pg_pool, &user_id)
        .await
        .map_err(e500)?;

    let credentials = Credentials {
        username,
        password: Secret::new(current_password.expose_secret().clone()),
    };
    let user_id = match validate_credentials(&pg_pool, credentials)
        .await
        .map_err(e500)
    {
        Ok(user_id) => user_id,
        Err(_) => {
            FlashMessage::error("Wrong current password").send();
            return Ok(see_other("/admin/password"));
        }
    };

    let new_password_hash = utils::spawn_blocking_task_with_tracing(move || {
        hash_password(new_password.expose_secret())
            .context("Failed to hash password into PCH format")
    })
    .await
    .context("Failed to spawn blocking task")
    .map_err(e500)?
    .map_err(e500)?;

    update_user_password_to_database(&user_id, &new_password_hash, &pg_pool)
        .await
        .context("Failed to update user password in database")
        .map_err(e500)?;

    FlashMessage::success("Password changed").send();
    Ok(see_other("/admin/password"))
}

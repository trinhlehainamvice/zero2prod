use crate::authentication::UserSession;
use crate::utils::{e500, see_other};
use actix_web::HttpResponse;
use actix_web_flash_messages::FlashMessage;

pub async fn logout(session: UserSession) -> Result<HttpResponse, actix_web::Error> {
    if session.get_user_id().map_err(e500)?.is_some() {
        session.logout();
        FlashMessage::info("You have been logged out").send();
    }
    Ok(see_other("/login"))
}

use crate::authentication::UserSession;
use crate::utils::{e500, see_other};
use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::error::InternalError;
use actix_web::{FromRequest, HttpMessage};
use actix_web_lab::middleware::Next;
use std::fmt::Display;
use std::ops::Deref;
use uuid::Uuid;

#[derive(Copy, Clone, Debug)]
pub struct UserId(Uuid);

impl Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for UserId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub async fn reject_anonymous_users(
    mut req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
    let session = {
        let (http_req, payload) = req.parts_mut();
        UserSession::from_request(http_req, payload).await?
    };

    match session.get_user_id().map_err(e500)? {
        Some(user_id) => {
            req.extensions_mut().insert(UserId(user_id));
            Ok(next.call(req).await?)
        }
        None => {
            let response = see_other("/login");
            let error = anyhow::anyhow!("Login required");
            Err(InternalError::from_response(error, response).into())
        }
    }
}

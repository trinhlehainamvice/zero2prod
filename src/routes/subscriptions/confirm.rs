use actix_web::{web, HttpResponse, Responder};

#[derive(serde::Deserialize)]
pub struct ConfirmTokenParam {
    pub token: String,
}

pub async fn confirm(web::Query(_token): web::Query<ConfirmTokenParam>) -> impl Responder {
    HttpResponse::Ok().finish()
}

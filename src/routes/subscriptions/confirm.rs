use actix_web::{HttpResponse, Responder};

pub async fn confirm() -> impl Responder {
    HttpResponse::Ok().finish()
}

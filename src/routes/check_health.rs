use actix_web::{HttpResponse, Responder};

pub async fn check_health() -> impl Responder {
    HttpResponse::Ok().finish()
}

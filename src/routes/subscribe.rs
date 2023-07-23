use actix_web::{web, HttpResponse, Responder};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Subscriber {
    name: String,
    email: String,
}

pub async fn subscribe(web::Form(_subscriber): web::Form<Subscriber>) -> impl Responder {
    HttpResponse::Ok().finish()
}

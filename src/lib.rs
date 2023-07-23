use actix_web::dev::Server;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use serde::Deserialize;
use std::net::TcpListener;

pub fn run(listener: TcpListener) -> Result<Server, std::io::Error> {
    let server = HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(greet))
            .route("/health_check", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
            .route("/{name}", web::get().to(greet))
    })
    .listen(listener)?
    .run();
    // server is already running at this point

    // await server is making server polling inner future command
    Ok(server)
}

async fn greet(req: HttpRequest) -> impl Responder {
    let name = req.match_info().get("name").unwrap_or("World");
    format!("Hello {}!", &name)
}

async fn health_check() -> impl Responder {
    HttpResponse::Ok().finish()
}

#[derive(Deserialize)]
struct Subscriber {
    name: String,
    email: String,
}

async fn subscribe(web::Form(_subscriber): web::Form<Subscriber>) -> impl Responder {
    HttpResponse::Ok().finish()
}

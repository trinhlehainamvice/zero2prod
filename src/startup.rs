use crate::routes::{check_health, subscribe};
use actix_web::dev::Server;
use actix_web::web::Data;
use actix_web::{web, App, HttpServer};
use sqlx::PgPool;
use std::net::TcpListener;

pub fn run(
    listener: TcpListener,
    db_connection_pool: PgPool,
) -> Result<Server, std::io::Error> {
    // So to share data between threads, actix-web provide web::Data<T>(Arc<T>)
    // which is a thread-safe reference counting pointer to a value of type T
    let db_connection_pool = Data::new(db_connection_pool);
    
    // Actix-web runtime that have multiple threads
    let server = HttpServer::new(move || {
        App::new()
            .route("/health", web::get().to(check_health))
            .route("/subscriptions", web::post().to(subscribe))
            // Application Context, that store state of application
            .app_data(db_connection_pool.clone())
    })
    .listen(listener)?
    .run();
    // server is already running at this point

    // await server is making server polling inner future command
    Ok(server)
}

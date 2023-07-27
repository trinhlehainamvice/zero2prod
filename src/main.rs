use zero2prod::configuration::Settings;
use zero2prod::startup::{build, get_pg_pool, run_until_terminated};
use zero2prod::telemetry::config_tracing;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let settings = Settings::get_configuration().expect("Failed to read configuration");

    config_tracing(&settings.application);

    let pg_pool = get_pg_pool(&settings.database);
    let (server, _) = build(pg_pool, settings).await?;
    run_until_terminated(server).await
}

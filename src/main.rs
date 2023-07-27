use zero2prod::configuration::Settings;
use zero2prod::startup::{get_pg_pool, Application};
use zero2prod::telemetry::config_tracing;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let settings = Settings::get_configuration().expect("Failed to read configuration");

    config_tracing(&settings.application);

    let pg_pool = get_pg_pool(&settings.database);
    let app = Application::build(pg_pool, settings).await?;
    app.run_until_terminated().await
}

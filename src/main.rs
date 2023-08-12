use std::fmt::{Debug, Display};
use tokio::task::JoinError;
use zero2prod::configuration::Settings;
use zero2prod::newsletters_issues::run_worker_until_stopped;
use zero2prod::startup::Application;
use zero2prod::telemetry::config_tracing;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let settings = Settings::get_configuration().expect("Failed to read configuration");

    config_tracing(&settings.application);

    let app = tokio::spawn(
        Application::builder(settings.clone())
            .build()
            .await?
            .run_until_terminated(),
    );
    // Spawn a background worker to handle the newsletters issue process in parallel
    let newsletters_issue_worker = tokio::spawn(run_worker_until_stopped(settings));

    tokio::select! {
        o = app => report_exit("API", o),
        o = newsletters_issue_worker => report_exit("Newsletter Issue Delivery Worker", o),
    }

    Ok(())
}

fn report_exit(task_name: &str, outcome: Result<Result<(), impl Display + Debug>, JoinError>) {
    match outcome {
        Ok(Ok(())) => tracing::info!("{} succeeded", task_name),
        Ok(Err(e)) => {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "{} task failed",
                task_name
            );
        }
        Err(e) => {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "{} task failed",
                task_name
            )
        }
    }
}

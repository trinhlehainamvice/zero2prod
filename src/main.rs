use std::fmt::{Debug, Display};
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::task::JoinError;
use zero2prod::configuration::Settings;
use zero2prod::newsletters_issues::{
    DeleteExpiredIdempotencyWorker, NewslettersIssuesDeliveryWorker,
};
use zero2prod::startup::Application;
use zero2prod::telemetry::config_tracing;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let settings = Settings::get_configuration().expect("Failed to read configuration");

    config_tracing(&settings.application);

    let notify = Arc::new(Notify::new());

    let app = tokio::spawn(
        Application::builder(settings.clone(), notify.clone())
            .build()
            .await?
            .run_until_terminated(),
    );

    let newsletters_issue_worker = tokio::spawn(
        NewslettersIssuesDeliveryWorker::builder(settings.clone(), notify).run_until_terminated(),
    );

    let delete_expired_idempotency_worker =
        tokio::spawn(DeleteExpiredIdempotencyWorker::builder(settings).run_until_terminated());

    tokio::select! {
        o = app => report_exit("API", o),
        o = newsletters_issue_worker => report_exit("Newsletter Issue Delivery Worker", o),
        o = delete_expired_idempotency_worker => report_exit("Delete Expired Idempotency Worker", o),
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

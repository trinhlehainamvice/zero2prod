use crate::configuration::Settings;
use crate::email_client::EmailClient;
use crate::routes::SubscriberEmail;
use crate::startup::{get_email_client, get_pg_pool};
use sqlx::postgres::types::PgInterval;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

pub struct NewslettersIssuesDeliveryWorker {
    settings: Settings,
    notify: Arc<Notify>,
    pg_pool: Option<PgPool>,
}

impl NewslettersIssuesDeliveryWorker {
    pub fn builder(settings: Settings, notify: Arc<Notify>) -> Self {
        Self {
            settings,
            notify,
            pg_pool: None,
        }
    }

    pub fn set_pg_pool(mut self, pg_pool: PgPool) -> Self {
        self.pg_pool = Some(pg_pool);
        self
    }

    pub async fn run_until_terminated(self) -> Result<(), std::io::Error> {
        let pg_pool = self
            .pg_pool
            .unwrap_or_else(|| get_pg_pool(&self.settings.database));
        let email_client = get_email_client(self.settings.email_client.clone());
        worker_loop(pg_pool, email_client, self.notify).await;
        Ok(())
    }
}

async fn worker_loop(pg_pool: PgPool, email_client: EmailClient, notify: Arc<Notify>) {
    loop {
        match try_execute_task(&pg_pool, &email_client).await {
            Ok(ExecutionResult::EmptyQueue) => notify.notified().await,
            // Sleep for a while to improve future chances of success
            // Reference: https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/
            Err(_) => tokio::time::sleep(Duration::from_secs(1)).await,
            Ok(ExecutionResult::TaskCompleted) => {}
        }
    }
}

pub struct NewslettersIssue {
    pub title: String,
    pub text_content: String,
    pub html_content: String,
}

type PgTransaction = sqlx::Transaction<'static, sqlx::Postgres>;

pub enum ExecutionResult {
    EmptyQueue,
    TaskCompleted,
}

#[tracing::instrument(
    name = "Execute newsletter issue task",
    skip_all,
    fields(
        newsletters_issue_id = tracing::field::Empty,
        subscriber_email = tracing::field::Empty
    )
)]
pub async fn try_execute_task(
    pg_pool: &PgPool,
    email_client: &EmailClient,
) -> anyhow::Result<ExecutionResult> {
    let task = dequeue_task(pg_pool).await?;
    if task.is_none() {
        return Ok(ExecutionResult::EmptyQueue);
    }

    let (mut transaction, newsletters_issue_id, subscriber_email) = task.unwrap();
    tracing::Span::current()
        .record(
            "newsletters_issue_id",
            &tracing::field::display(newsletters_issue_id),
        )
        .record(
            "subscriber_email",
            &tracing::field::display(&subscriber_email),
        );

    match SubscriberEmail::parse(subscriber_email.clone()).map_err(|e| anyhow::anyhow!(e)) {
        Ok(subscriber_email) => {
            let issue = get_issue(pg_pool, newsletters_issue_id).await?;
            if let Err(e) = email_client
                .send_email(
                    &subscriber_email,
                    &issue.title,
                    &issue.text_content,
                    &issue.html_content,
                )
                .await
            {
                tracing::error!(
                    error.cause_chain = ?e,
                    error.message = %e,
                    "Failed to send newsletter issue email to subscriber"
                );
                return Err(anyhow::anyhow!(e));
            }
        }
        Err(e) => {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "Skip sending newsletter issue to invalid subscriber email"
            );
        }
    }

    delete_task(&mut transaction, newsletters_issue_id, subscriber_email).await?;
    transaction.commit().await?;
    Ok(ExecutionResult::TaskCompleted)
}

#[tracing::instrument(
    name = "Insert newsletters issue into database",
    skip(newsletters, transaction)
)]
pub async fn insert_newsletters_issue(
    transaction: &mut PgTransaction,
    newsletters_issue_id: uuid::Uuid,
    newsletters: NewslettersIssue,
) -> Result<(), sqlx::Error> {
    let NewslettersIssue {
        title,
        text_content,
        html_content,
    } = newsletters;
    sqlx::query!(
        r#"
        INSERT INTO newsletters_issues (id, title, text_content, html_content, published_at)
        VALUES ($1, $2, $3, $4, now())
        "#,
        newsletters_issue_id,
        title,
        text_content,
        html_content
    )
    .execute(transaction)
    .await?;

    Ok(())
}

#[tracing::instrument(
    name = "Enqueue delivery newsletters issue into database",
    skip(newsletters_issue_id, transaction)
)]
pub async fn enqueue_task(
    transaction: &mut PgTransaction,
    newsletters_issue_id: uuid::Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO newsletters_issues_delivery_queue (id, subscriber_email)
        SELECT $1,
        email FROM subscriptions WHERE status = 'confirmed'
        "#,
        newsletters_issue_id
    )
    .execute(transaction)
    .await?;

    Ok(())
}

#[tracing::instrument(name = "Dequeue delivery newsletters issue into database", skip_all)]
async fn dequeue_task(
    pg_pool: &PgPool,
) -> Result<Option<(PgTransaction, uuid::Uuid, String)>, sqlx::Error> {
    let mut transaction = pg_pool.begin().await?;
    struct Row {
        id: uuid::Uuid,
        subscriber_email: String,
    }
    // Retrieve one task at a time (LIMIT 1)
    // And skip locking row that currently in process (SKIP LOCKED)
    // Lock this row if success to retrieve (FOR UPDATE)
    let result = sqlx::query_as!(
        Row,
        r#"
        SELECT id, subscriber_email
        FROM newsletters_issues_delivery_queue
        FOR UPDATE
        SKIP LOCKED
        LIMIT 1
        "#,
    )
    .fetch_optional(&mut transaction)
    .await?;

    match result {
        Some(Row {
            id,
            subscriber_email,
        }) => Ok(Some((transaction, id, subscriber_email))),
        None => Ok(None),
    }
}

#[tracing::instrument(
    name = "Delete delivery newsletters issue from database",
    skip(transaction, newsletters_issue_id, subscriber_email)
)]
async fn delete_task(
    transaction: &mut PgTransaction,
    newsletters_issue_id: uuid::Uuid,
    subscriber_email: String,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        DELETE FROM newsletters_issues_delivery_queue
        WHERE id = $1 AND subscriber_email = $2
        "#,
        newsletters_issue_id,
        subscriber_email
    )
    .execute(transaction)
    .await?;

    Ok(())
}

#[tracing::instrument(name = "Get newsletter issue from database", skip_all)]
async fn get_issue(pg_pool: &PgPool, id: uuid::Uuid) -> Result<NewslettersIssue, sqlx::Error> {
    let result = sqlx::query_as!(
        NewslettersIssue,
        r#"
        SELECT title, text_content, html_content
        FROM newsletters_issues
        WHERE id = $1
        "#,
        id
    )
    .fetch_one(pg_pool)
    .await?;
    Ok(result)
}

pub struct DeleteExpiredIdempotencyWorker {
    settings: Settings,
    pg_pool: Option<PgPool>,
}

impl DeleteExpiredIdempotencyWorker {
    pub fn builder(settings: Settings) -> Self {
        Self {
            settings,
            pg_pool: None,
        }
    }

    pub fn set_pg_pool(mut self, pg_pool: PgPool) -> Self {
        self.pg_pool = Some(pg_pool);
        self
    }

    pub async fn run_until_terminated(self) -> Result<(), std::io::Error> {
        let expiration_time_millis: Duration =
            Duration::from_millis(self.settings.application.idempotency_expiration_millis);
        let pg_pool = self
            .pg_pool
            .unwrap_or_else(|| get_pg_pool(&self.settings.database));
        remove_expired_idempotency_worker_loop(pg_pool, expiration_time_millis).await;
        Ok(())
    }
}

async fn remove_expired_idempotency_worker_loop(pg_pool: PgPool, expired_time_millis: Duration) {
    loop {
        match delete_expired_idempotency_keys(&pg_pool, expired_time_millis).await {
            Ok(_) => tokio::time::sleep(expired_time_millis).await,
            Err(e) => {
                tracing::error!(
                    error.cause_chain = ?e,
                    error.message = %e,
                    "Failed to delete expired idempotency keys"
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

#[tracing::instrument(
    name = "Delete expired idempotency keys in database",
    skip(pg_pool, expired_time)
)]
async fn delete_expired_idempotency_keys(
    pg_pool: &PgPool,
    expired_time: Duration,
) -> Result<(), anyhow::Error> {
    let expired_time = PgInterval::try_from(expired_time).map_err(|e| anyhow::anyhow!(e))?;
    sqlx::query!(
        r#"
        DELETE FROM idempotency
        WHERE now() - created_at > $1
        "#,
        expired_time
    )
    .execute(pg_pool)
    .await?;
    Ok(())
}

// TODO: e.g. adding a n_retries and
// execute_after columns to keep track of how many attempts have already taken place and how long
// we should wait before trying again. Try implementing it as an exercise

// TODO: add newsletters issues status(processing, published, failed) column to the database
// dequeue task depend on newsletters issues status, no need to query newsletters issue content every time dequeue a task

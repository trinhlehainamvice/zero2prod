use crate::configuration::Settings;
use crate::email_client::EmailClient;
use crate::routes::SubscriberEmail;
use crate::startup::{get_email_client, get_pg_pool};
use sqlx::PgPool;
use std::time::Duration;

pub async fn run_worker_until_stopped(settings: Settings) -> Result<(), std::io::Error> {
    let pg_pool = get_pg_pool(&settings.database);
    let email_client = get_email_client(&settings.email_client);
    worker_loop(pg_pool, email_client).await;
    Ok(())
}

async fn worker_loop(pg_pool: PgPool, email_client: EmailClient) {
    loop {
        match try_execute_task(&pg_pool, &email_client).await {
            Ok(ExecutionResult::EmptyQueue) => tokio::time::sleep(Duration::from_secs(10)).await,
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

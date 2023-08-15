use crate::authentication::UserId;
use crate::idempotency::{
    try_insert_idempotency_response_record_into_database, update_idempotency_response_record,
    ProcessState,
};
use crate::newsletters_issues::{
    enqueue_task, get_tasks_count_in_queue, insert_newsletters_issue,
    update_newsletters_issue_require_n_tasks, NewslettersIssue,
};
use crate::utils::{e400, e500, see_other};
use actix_web::{web, HttpResponse};
use actix_web_flash_messages::FlashMessage;
use anyhow::Context;
use sqlx::PgPool;
use tokio::sync::Notify;

#[derive(serde::Deserialize)]
pub struct NewsletterForm {
    title: String,
    text_content: String,
    html_content: String,
    idempotency_key: String,
}

#[tracing::instrument(
    name = "Publish a newsletter letter",
    skip_all,
    fields(
        username = tracing::field::Empty,
        user_id = tracing::field::Empty
    )
)]
pub async fn publish_newsletters(
    web::Form(NewsletterForm {
        title,
        text_content,
        html_content,
        idempotency_key,
    }): web::Form<NewsletterForm>,
    pg_pool: web::Data<PgPool>,
    user_id: web::ReqData<UserId>,
    notify: web::Data<Notify>,
) -> Result<HttpResponse, actix_web::Error> {
    let idempotency_key = idempotency_key.try_into().map_err(e400)?;
    let user_id = user_id.into_inner();
    let transaction = pg_pool.begin().await.map_err(e500)?;

    let mut transaction = match try_insert_idempotency_response_record_into_database(
        transaction,
        &idempotency_key,
        &user_id,
    )
    .await
    .map_err(e500)?
    {
        ProcessState::Completed(response) => return Ok(response),
        ProcessState::StartProcessing(transaction) => transaction,
    };

    let newsletters_issue_id = uuid::Uuid::new_v4();
    insert_newsletters_issue(
        &mut transaction,
        newsletters_issue_id,
        NewslettersIssue {
            title,
            text_content,
            html_content,
        },
    )
    .await
    .map_err(e500)?;

    enqueue_task(&mut transaction, newsletters_issue_id)
        .await
        .map_err(e500)?;

    let required_n_tasks = get_tasks_count_in_queue(&mut transaction, &newsletters_issue_id)
        .await
        .map_err(e500)?
        .context("Tasks count in newsletters issue delivery queue is None")
        .map_err(e500)? as i32;

    update_newsletters_issue_require_n_tasks(
        &mut transaction,
        &newsletters_issue_id,
        required_n_tasks,
    )
    .await
    .map_err(e500)?;

    FlashMessage::success("Published newsletter successfully!").send();
    let response = see_other("/admin/newsletters");
    let response =
        update_idempotency_response_record(&mut transaction, &idempotency_key, &user_id, response)
            .await
            .map_err(e500)?;
    transaction.commit().await.map_err(e500)?;
    notify.notify_one();
    Ok(response)
}

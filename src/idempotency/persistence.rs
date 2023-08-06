use crate::idempotency::IdempotencyKey;
use actix_web::body::to_bytes;
use actix_web::http::StatusCode;
use actix_web::HttpResponse;
use sqlx::postgres::{PgHasArrayType, PgTypeInfo};
use sqlx::{Postgres, Transaction};

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "header_value")]
struct ResponseHeaderRecord {
    key: String,
    value: Vec<u8>,
}

impl PgHasArrayType for ResponseHeaderRecord {
    fn array_type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("_header_value")
    }
}

pub enum ProcessState {
    StartProcessing(Transaction<'static, Postgres>),
    Completed(HttpResponse),
}

async fn get_idempotency_response_record_from_database(
    transaction: &mut Transaction<'_, Postgres>,
    idempotency_key: &IdempotencyKey,
    user_id: &uuid::Uuid,
) -> Result<Option<HttpResponse>, anyhow::Error> {
    struct Row {
        response_status_code: i16,
        response_headers: Vec<ResponseHeaderRecord>,
        response_body: Vec<u8>,
    }
    let record = sqlx::query_as!(
        Row,
        r#"
        SELECT 
            response_status_code as "response_status_code!",
            response_headers as "response_headers!: Vec<ResponseHeaderRecord>",
            response_body as "response_body!"
        FROM idempotency
        WHERE user_id = $1 AND idempotency_key = $2
        "#,
        user_id,
        idempotency_key.as_ref()
    )
    .fetch_optional(transaction)
    .await?;

    match record {
        Some(Row {
            response_status_code,
            response_headers,
            response_body,
        }) => {
            let status_code = StatusCode::from_u16(response_status_code.try_into()?)?;
            let mut response = HttpResponse::build(status_code);
            for ResponseHeaderRecord { key, value } in response_headers {
                response.append_header((key, value));
            }
            Ok(Some(response.body(response_body)))
        }
        None => Ok(None),
    }
}

pub async fn try_insert_idempotency_response_record_into_database(
    mut transaction: Transaction<'static, Postgres>,
    idempotency_key: &IdempotencyKey,
    user_id: &uuid::Uuid,
) -> Result<ProcessState, anyhow::Error> {
    let n_row_affected = sqlx::query!(
        r#"
        INSERT INTO idempotency (
            user_id,
            idempotency_key,
            created_at
        )
        VALUES (
            $1,
            $2,
            now()
        )
        ON CONFLICT DO NOTHING
        "#,
        user_id,
        idempotency_key.as_ref()
    )
    .execute(&mut transaction)
    .await?
    .rows_affected();

    match n_row_affected {
        // If there is no row affected, query is rejected when trying to insert a new idempotency key
        // Means the idempotency key already exists in the database
        0 => {
            let response = get_idempotency_response_record_from_database(
                &mut transaction,
                idempotency_key,
                user_id,
            )
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("Expected to get a idempotency response record from database")
            })?;

            // Consume the transaction if idempotency response record is already in database
            transaction.commit().await?;
            Ok(ProcessState::Completed(response))
        }
        // Return transaction back to main process to update idempotency response record
        _ => Ok(ProcessState::StartProcessing(transaction)),
    }
}

pub async fn update_idempotency_response_record(
    transaction: &mut Transaction<'_, Postgres>,
    idempotency_key: &IdempotencyKey,
    user_id: &uuid::Uuid,
    response: HttpResponse,
) -> Result<HttpResponse, anyhow::Error> {
    // HttpResponse can't be clone, so we split it parts and combine them back into HttpResponse later
    // After we done processing that need data from its parts
    let (response_headers, body) = response.into_parts();
    let status_code: i16 = response_headers.status().as_u16().try_into()?;
    let headers = {
        let mut headers = Vec::with_capacity(response_headers.headers().len());
        for (key, value) in response_headers.headers().iter() {
            let key = key.as_str().to_owned();
            let value = value.as_bytes().to_owned();
            headers.push(ResponseHeaderRecord { key, value });
        }
        headers
    };
    let body = to_bytes(body).await.map_err(|e| anyhow::anyhow!("{}", e))?;

    sqlx::query!(
        r#"
        UPDATE idempotency
        SET
            response_status_code = $1,
            response_headers = $2,
            response_body = $3
        WHERE
            user_id = $4 AND idempotency_key = $5
        "#,
        status_code,
        headers as _,
        body.as_ref(),
        user_id,
        idempotency_key.as_ref()
    )
    .execute(transaction)
    .await?;

    let response = response_headers.set_body(body).map_into_boxed_body();
    Ok(response)
}

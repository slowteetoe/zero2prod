use crate::authentication::UserId;
use crate::idempotency::save_response;
use crate::idempotency::try_processing;
use crate::idempotency::IdempotencyKey;
use crate::idempotency::NextAction;
use crate::routes::error_chain_fmt;
use crate::utils::e400;
use crate::utils::e500;
use crate::utils::see_other;

use actix_web::http::header;
use actix_web::http::StatusCode;
use actix_web::Either;
use actix_web::{web, HttpResponse, ResponseError};
use actix_web_flash_messages::FlashMessage;
use anyhow::Context;
use reqwest::header::HeaderValue;
use sqlx::PgPool;
use sqlx::Postgres;
use sqlx::Transaction;
use uuid::Uuid;

#[derive(thiserror::Error)]
pub enum PublishError {
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
    #[error("Authentication failed")]
    AuthError(#[source] anyhow::Error),
}

impl std::fmt::Debug for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for PublishError {
    // Since we're providing a custom 'error_response', there's no longer any need to override 'status_code'
    fn error_response(&self) -> HttpResponse {
        match self {
            PublishError::UnexpectedError(_) => {
                HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR)
            }
            PublishError::AuthError(_) => {
                let mut response = HttpResponse::new(StatusCode::UNAUTHORIZED);
                let header_value = HeaderValue::from_str(r#"Basic realm="publish""#).unwrap();
                response
                    .headers_mut()
                    .insert(header::WWW_AUTHENTICATE, header_value);
                response
            }
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct BodyData {
    title: String,
    content: Content,
    idempotency_key: IdempotencyKey,
}

#[derive(Debug, serde::Deserialize)]
pub struct Content {
    html: String,
    text: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct FormData {
    title: String,
    html_content: String,
    text_content: String,
    idempotency_key: String,
}

#[tracing::instrument(name = "Publish a newsletter", skip_all, fields(user_id=%&*user_id))]
pub async fn publish_newsletter(
    body: Either<web::Form<FormData>, web::Json<BodyData>>,
    pool: web::Data<PgPool>,
    user_id: web::ReqData<UserId>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = user_id.into_inner();
    let body: BodyData = match body {
        Either::Right(json) => BodyData {
            title: json.title.to_owned(),
            content: Content {
                html: json.content.html.to_owned(),
                text: json.content.text.to_owned(),
            },
            idempotency_key: json.idempotency_key.clone(),
        },
        Either::Left(form) => BodyData {
            title: form.title.to_owned(),
            content: Content {
                html: form.html_content.to_owned(),
                text: form.text_content.to_owned(),
            },
            idempotency_key: form.idempotency_key.clone().try_into().map_err(e400)?,
        },
    };
    let idempotency_key = body.idempotency_key;

    let mut transaction = match try_processing(&pool, &idempotency_key, *user_id)
        .await
        .map_err(e500)?
    {
        NextAction::StartProcessing(t) => t,
        NextAction::ReturnSavedResponse(saved_response) => {
            success_message().send();
            return Ok(saved_response);
        }
    };

    let issue_id = insert_newsletter_issue(
        &mut transaction,
        &body.title,
        &body.content.html,
        &body.content.text,
    )
    .await
    .context("Failed to store newsletter issue details")
    .map_err(e500)?;

    enqueue_delivery_tasks(&mut transaction, issue_id)
        .await
        .context("Failed to enqueue delivery tasks")
        .map_err(e500)?;

    let response = see_other("/admin/dashboard");
    let response = save_response(transaction, &idempotency_key, *user_id, response)
        .await
        .map_err(e500)?;
    success_message().send();
    Ok(response)
}

fn success_message() -> FlashMessage {
    FlashMessage::info("The newsletter issue has been accepted - emails will go out shortly")
}

#[tracing::instrument(skip_all)]
async fn insert_newsletter_issue(
    transaction: &mut Transaction<'static, Postgres>,
    title: &str,
    html_content: &str,
    text_content: &str,
) -> Result<Uuid, sqlx::Error> {
    let newsletter_issue_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO newsletter_issues (
            newsletter_issue_id,
            title,
            text_content,
            html_content,
            published_at
        ) VALUES ($1, $2, $3, $4, now())
    "#,
        newsletter_issue_id,
        title,
        text_content,
        html_content
    )
    .execute(transaction)
    .await?;
    Ok(newsletter_issue_id)
}

#[tracing::instrument(skip_all)]
async fn enqueue_delivery_tasks(
    transaction: &mut Transaction<'static, Postgres>,
    newsletter_issue_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO issue_delivery_queue (
            newsletter_issue_id,
            subscriber_email
        )
        SELECT $1, email
        FROM subscriptions
        WHERE status = 'confirmed'
    "#,
        newsletter_issue_id
    )
    .execute(transaction)
    .await?;
    Ok(())
}

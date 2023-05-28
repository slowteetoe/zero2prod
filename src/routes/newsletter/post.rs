use crate::authentication::UserId;
use crate::domain::SubscriberEmail;
use crate::email_client::EmailClient;
use crate::idempotency::save_response;
use crate::idempotency::try_processing;
use crate::idempotency::IdempotencyKey;
use crate::idempotency::NextAction;
use crate::routes::error_chain_fmt;
use crate::utils::e400;
use crate::utils::e500;
use actix_web::http::header;
use actix_web::http::StatusCode;
use actix_web::Either;
use actix_web::{web, HttpResponse, ResponseError};
use actix_web_flash_messages::FlashMessage;
use anyhow::Context;
use reqwest::header::HeaderValue;
use reqwest::header::LOCATION;
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

#[tracing::instrument(name = "Publish a newsletter", skip(body, pool, email_client), fields(user_id=tracing::field::Empty))]
pub async fn publish_newsletter(
    body: Either<web::Form<FormData>, web::Json<BodyData>>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    user_id: web::ReqData<UserId>,
) -> Result<HttpResponse, actix_web::Error> {
    tracing::Span::current().record("user_id", &tracing::field::display(*user_id));

    let response_type;
    let body: BodyData = match body {
        Either::Right(json) => {
            response_type = "json".to_owned();
            BodyData {
                title: json.title.to_owned(),
                content: Content {
                    html: json.content.html.to_owned(),
                    text: json.content.text.to_owned(),
                },
                idempotency_key: json.idempotency_key.clone(),
            }
        }
        Either::Left(form) => {
            response_type = "html".to_owned();
            BodyData {
                title: form.title.to_owned(),
                content: Content {
                    html: form.html_content.to_owned(),
                    text: form.text_content.to_owned(),
                },
                idempotency_key: form.idempotency_key.clone().try_into().map_err(e400)?,
            }
        }
    };
    let idempotency_key = body.idempotency_key;

    let transaction = match try_processing(&pool, &idempotency_key, **user_id)
        .await
        .map_err(e500)?
    {
        NextAction::StartProcessing(t) => t,
        NextAction::ReturnSavedResponse(saved_response) => {
            success_message().send();
            return Ok(saved_response);
        }
    };

    let subscribers = get_confirmed_subscribers(&pool).await.map_err(e500)?;

    let mut sent = 0u16;
    let mut errored = 0u16;
    for subscriber in subscribers {
        match subscriber {
            Ok(subscriber) => {
                email_client
                    .send_email(
                        &subscriber.email,
                        &body.title,
                        &body.content.html,
                        &body.content.text,
                    )
                    .await
                    .with_context(|| format!("Failed to send newsletter to {}", subscriber.email))
                    .map_err(e500)?;
                sent += 1;
            }
            Err(error) => {
                tracing::warn!(
                    // record the error chain as a structured field on the log record
                    error.cause_chain = ?error,
                    "Skipping a confirmed subscriber. Their stored contact details are invalid",
                );
                errored += 1;
            }
        }
    }

    // This is getting a little weird - we want to set a flash message if you're using the webpage
    // Should probably return same data in a JSON response...
    let response = match response_type.as_str() {
        "html" => {
            // yes, this slightly breaks idempotency
            FlashMessage::info(format!(
                "The newsletter issue has been published. {} successfully, {} with errors.",
                sent, errored
            ))
            .send();
            HttpResponse::SeeOther()
                .insert_header((LOCATION, "/admin/dashboard"))
                .finish()
        }
        _ => HttpResponse::Ok().finish(),
    };

    let response = save_response(transaction, &idempotency_key, **user_id, response)
        .await
        .map_err(e500)?;
    Ok(response)
}

fn success_message() -> FlashMessage {
    FlashMessage::info("The newsletter issue has been published!")
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

#[derive(Debug)]
pub struct ConfirmedSubscriber {
    email: SubscriberEmail,
}

#[tracing::instrument(name = "Get confirmed subscribers", skip(pool))]
async fn get_confirmed_subscribers(
    pool: &PgPool,
    // We are returning a Vec of Results in the happy case
    // This allows the caller to bubble up errors due to network or transient failures by using the '?' operator
    // while the compiler forces them to handle the subtler mapping error
    // See http://sled.rs/errors.html for a deeper-dive about this technique
) -> Result<Vec<Result<ConfirmedSubscriber, anyhow::Error>>, anyhow::Error> {
    let confirmed_subscribers = sqlx::query!(
        r#"
        SELECT email
        FROM subscriptions
        WHERE status = 'confirmed'
        "#,
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| match SubscriberEmail::parse(r.email) {
        Ok(email) => Ok(ConfirmedSubscriber { email }),
        Err(error) => Err(anyhow::anyhow!(error)),
    })
    .collect();
    Ok(confirmed_subscribers)
}

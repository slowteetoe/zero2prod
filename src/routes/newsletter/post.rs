use crate::authentication::UserId;
use crate::domain::SubscriberEmail;
use crate::email_client::EmailClient;
use crate::routes::error_chain_fmt;
use actix_web::http::header;
use actix_web::http::StatusCode;
use actix_web::Either;
use actix_web::{web, HttpResponse, ResponseError};
use anyhow::Context;

use reqwest::header::HeaderValue;

use sqlx::PgPool;

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
}

#[tracing::instrument(name = "Publish a newsletter", skip(body, pool, email_client), fields(user_id=tracing::field::Empty))]
pub async fn publish_newsletter(
    body: Either<web::Form<FormData>, web::Json<BodyData>>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    user_id: web::ReqData<UserId>,
) -> Result<HttpResponse, PublishError> {
    tracing::Span::current().record("user_id", &tracing::field::display(*user_id));

    let body: BodyData = match body {
        Either::Right(json) => BodyData {
            title: json.title.to_owned(),
            content: Content {
                html: json.content.html.to_owned(),
                text: json.content.text.to_owned(),
            },
        },
        Either::Left(form) => BodyData {
            title: form.title.to_owned(),
            content: Content {
                html: form.html_content.to_owned(),
                text: form.text_content.to_owned(),
            },
        },
    };

    let subscribers = get_confirmed_subscribers(&pool).await?;

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
                    .with_context(|| {
                        format!("Failed to send newsletter to {}", subscriber.email)
                    })?;
            }
            Err(error) => {
                tracing::warn!(
                    // record the error chain as a structured field on the log record
                    error.cause_chain = ?error,
                    "Skipping a confirmed subscriber. Their stored contact details are invalid",
                );
            }
        }
    }
    Ok(HttpResponse::Ok().finish())
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

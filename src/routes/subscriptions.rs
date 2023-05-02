use actix_web::http::StatusCode;
use actix_web::{web, HttpResponse, ResponseError};
use chrono::Utc;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::EmailClient,
    startup::ApplicationBaseUrl,
};

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = String;

    fn try_from(form: FormData) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(form.name)?;
        let email = SubscriberEmail::parse(form.email)?;
        Ok(NewSubscriber { email, name })
    }
}

#[tracing::instrument(name="Adding a new subscriber", skip(form, pool, email_client, base_url), fields(subscriber_email=%form.email, subscriber_name=%form.name))]
pub async fn subscribe(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>,
) -> Result<HttpResponse, SubscribeError> {
    // `web::Form` is a wrapper around `FormData`
    // `form.0` gives us access to the underlying `FormData`
    // since we implemented 'TryFrom<FormData> for NewSubscriber', we can just use try_into()
    let new_subscriber = form.0.try_into().map_err(SubscribeError::ValidationError)?;

    let subscription_token = generate_subscription_token();
    let mut transaction = pool
        .begin()
        .await
        .map_err(SubscribeError::TransactionCommitError)?;

    match insert_subscriber(&mut transaction, &new_subscriber).await {
        Ok(subscriber_id) => {
            // store the token so that subscription can be confirmed
            // it's better that we have extra rows in db (in case email fails to send)
            // than if we have an email sent that can't be confirmed bc the db call failed
            store_token(&mut transaction, subscriber_id, subscription_token.as_str()).await?;

            // commit the tx before sending the email
            transaction
                .commit()
                .await
                .map_err(SubscribeError::TransactionCommitError)?;

            // send confirmation email to the potential subscriber
            send_confirmation_email(
                &email_client,
                new_subscriber,
                &base_url.0,
                &subscription_token,
            )
            .await?;

            Ok(HttpResponse::Ok().finish())
        }
        Err(e) => match e {
            sqlx::Error::Database(err) => {
                if err.constraint() == Some("subscriptions_email_key") {
                    tracing::info!("User is already subscribed");
                    Ok(HttpResponse::NoContent().finish())
                } else {
                    Ok(HttpResponse::InternalServerError().finish())
                }
            }
            _ => Ok(HttpResponse::InternalServerError().finish()),
        },
    }
}

#[derive(thiserror::Error)]
pub enum SubscribeError {
    #[error("{0}")]
    ValidationError(String),

    #[error("Failed to acquire a Postgres connection from the pool")]
    PoolError(#[source] sqlx::Error),

    #[error("Failed to inser new subscriber into the database")]
    InsertSubscriberError(#[source] sqlx::Error),

    #[error("Failed to commit the SQL transaction to store a new subscriber")]
    TransactionCommitError(#[source] sqlx::Error),

    #[error("Failed to store the confirmation token for the subscriber")]
    StoreTokenError(#[from] StoreTokenError),

    #[error("Failed to send confirmation email")]
    SendEmailError(#[from] reqwest::Error),
}
impl std::fmt::Debug for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for SubscribeError {
    fn status_code(&self) -> StatusCode {
        match self {
            SubscribeError::ValidationError(_) => StatusCode::BAD_REQUEST,
            SubscribeError::PoolError(_)
            | SubscribeError::InsertSubscriberError(_)
            | SubscribeError::TransactionCommitError(_)
            | SubscribeError::StoreTokenError(_)
            | SubscribeError::SendEmailError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[tracing::instrument(
    name = "Saving new subscriber details in db",
    skip(new_subscriber, transaction)
)]
pub async fn insert_subscriber(
    transaction: &mut Transaction<'_, Postgres>,
    new_subscriber: &NewSubscriber,
) -> Result<Uuid, sqlx::Error> {
    let subscriber_id = Uuid::new_v4();
    sqlx::query!(
        r#"
    INSERT INTO subscriptions (id, email, name, subscribed_at, status)
    VALUES ($1, $2, $3, $4, 'pending_confirmation')
    "#,
        &subscriber_id,
        &new_subscriber.email.as_ref(),
        &new_subscriber.name.as_ref(),
        Utc::now()
    )
    .execute(transaction)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(subscriber_id)
}

#[tracing::instrument(
    name = "send a confirmation email to new subscriber",
    skip(email_client, new_subscriber)
)]
pub async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
    base_url: &str,
    subscription_token: &str,
) -> Result<(), reqwest::Error> {
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url, subscription_token,
    );
    let plain_body = format!(
        "Welcome to our newsletter!\nVisit {} to confirm your subscription.",
        confirmation_link
    );
    let html_body = format!("Welcome to our newsletter!<br />Click <a href=\"{}\">here</a> to confirm your subscription",confirmation_link);

    email_client
        .send_email(new_subscriber.email, "Welcome!", &html_body, &plain_body)
        .await
}

fn generate_subscription_token() -> String {
    let mut rng = thread_rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}

#[tracing::instrument(
    name = "store subscriber token in database",
    skip(subscription_token, transaction)
)]
pub async fn store_token(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber_id: Uuid,
    subscription_token: &str,
) -> Result<(), StoreTokenError> {
    sqlx::query!(
        r#"INSERT INTO subscription_tokens 
    (subscription_token, subscriber_id)
    VALUES 
    ($1, $2)"#,
        subscription_token,
        subscriber_id
    )
    .execute(transaction)
    .await
    .map_err(|e| {
        tracing::error!("Failed to insert token: {:?}", e);
        StoreTokenError(e)
    })?;
    Ok(())
}

pub struct StoreTokenError(sqlx::Error);

impl std::error::Error for StoreTokenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl std::fmt::Debug for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl std::fmt::Display for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "a database error was encountered while trying to store a subscription token"
        )
    }
}

fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        current = cause.source();
    }
    Ok(())
}

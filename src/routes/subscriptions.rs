use actix_web::{web, HttpResponse};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::EmailClient,
};

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

#[tracing::instrument(name="Adding a new subscriber", skip(form, pool, email_client), fields(subscriber_email=%form.email, subscriber_name=%form.name))]
pub async fn subscribe(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
) -> HttpResponse {
    // `web::Form` is a wrapper around `FormData`
    // `form.0` gives us access to the underlying `FormData`
    // since we implemented 'TryFrom<FormData> for NewSubscriber', we can just use try_into()
    let new_subscriber = match form.0.try_into() {
        Ok(subscriber) => subscriber,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };
    match insert_subscriber(&pool, &new_subscriber).await {
        Ok(_) => {
            // send a (useless for now) email to the new subscriber
            let send_result = send_confirmation_email(&email_client, new_subscriber).await;
            if send_result.is_err() {
                tracing::error!("{:?}", send_result);
                return HttpResponse::InternalServerError().finish();
            }
            HttpResponse::Ok().finish()
        }
        Err(e) => match e {
            sqlx::Error::Database(err) => {
                if err.constraint() == Some("subscriptions_email_key") {
                    tracing::info!("User is already subscribed");
                    HttpResponse::NoContent().finish()
                } else {
                    HttpResponse::InternalServerError().finish()
                }
            }
            _ => HttpResponse::InternalServerError().finish(),
        },
    }
}

#[tracing::instrument(
    name = "Saving new subscriber details in db",
    skip(new_subscriber, pool)
)]
pub async fn insert_subscriber(
    pool: &PgPool,
    new_subscriber: &NewSubscriber,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
    INSERT INTO subscriptions (id, email, name, subscribed_at, status)
    VALUES ($1, $2, $3, $4, 'pending_confirmation')
    "#,
        Uuid::new_v4(),
        &new_subscriber.email.as_ref(),
        &new_subscriber.name.as_ref(),
        Utc::now()
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(())
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = String;

    fn try_from(form: FormData) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(form.name)?;
        let email = SubscriberEmail::parse(form.email)?;
        Ok(NewSubscriber { email, name })
    }
}

#[tracing::instrument(
    name = "send a confirmation email to new subscriber",
    skip(email_client, new_subscriber)
)]
pub async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
) -> Result<(), reqwest::Error> {
    let confirmation_link = "https://example.com/subscriptions/confirm";
    let plain_body = format!(
        "Welcome to our newsletter!\nVisit {} to confirm your subscription.",
        confirmation_link
    );
    let html_body = format!("Welcome to our newsletter!<br />Click <a href=\"{}\">here</a> to confirm your subscription",confirmation_link);

    email_client
        .send_email(new_subscriber.email, "Welcome!", &html_body, &plain_body)
        .await
}

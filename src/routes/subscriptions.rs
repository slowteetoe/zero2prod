use actix_web::{web, HttpResponse};
use chrono::Utc;
use sqlx::PgPool;
use tracing::{Instrument};
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

pub async fn subscribe(form: web::Form<FormData>, pool: web::Data<PgPool>) -> HttpResponse {
    let request_id = Uuid::new_v4().to_string();
    let request_span = tracing::info_span!(
        "Adding a new subscriber",
        %request_id,
        subscriber_email = %form.email,
        subscriber_name = %form.name
    );
    let _request_span_guard = request_span.enter();

    let query_span = tracing::info_span!("Saving new subscriber details in the database");

    match sqlx::query!(
        r#"
    INSERT INTO subscriptions (id, email, name, subscribed_at)
    VALUES ($1, $2, $3, $4)
    "#,
        Uuid::new_v4(),
        form.email,
        form.name,
        Utc::now()
    )
    .execute(pool.get_ref())
    .instrument(query_span)
    .await
    {
        Ok(_) => {
            tracing::info!("Details saved");
            HttpResponse::Ok().finish()
        }
        Err(e) => match e {
            sqlx::Error::Database(err) => {
                if err.constraint() == Some("subscriptions_email_key") {
                    tracing::info!("request_id {} User is already subscribed", request_id);
                    HttpResponse::NoContent().finish()
                } else {
                    tracing::error!(
                        "request_id {} Failed to insert subscription due to database error: {:?}",
                        request_id,
                        err
                    );
                    HttpResponse::InternalServerError().finish()
                }
            }
            _ => {
                tracing::error!(
                    "request_id {} Failed to insert subscription: {:?}",
                    request_id,
                    e
                );
                HttpResponse::InternalServerError().finish()
            }
        },
    }
}

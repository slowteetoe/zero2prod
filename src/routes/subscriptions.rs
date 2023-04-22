use actix_web::{web, HttpResponse};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

pub async fn subscribe(form: web::Form<FormData>, pool: web::Data<PgPool>) -> HttpResponse {
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
    .await
    {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(e) => match e {
            sqlx::Error::Database(err) => {
                if err.constraint() == Some("subscriptions_email_key") {
                    println!("User is already subscribed");
                    HttpResponse::NoContent().finish()
                } else {
                    println!(
                        "Failed to insert subscription due to database error: {}",
                        err
                    );
                    HttpResponse::InternalServerError().finish()
                }
            }
            _ => {
                println!("Failed to insert subscription: {}", e);
                HttpResponse::InternalServerError().finish()
            }
        },
    }
}

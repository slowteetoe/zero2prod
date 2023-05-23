use std::net::TcpListener;

use crate::authentication::reject_anonymous_users;
use crate::configuration::{DatabaseSettings, Settings};
use crate::domain::SubscriberEmail;
use crate::email_client::EmailClient;
use crate::routes::newsletter::{publish_newsletter, publish_newsletter_form};
use crate::routes::{
    admin_dashboard, change_password, change_password_form, confirm, health_check, home, log_out,
    login, login_form, subscribe,
};

use actix_session::storage::RedisSessionStore;
use actix_session::SessionMiddleware;
use actix_web::cookie::Key;
use actix_web::dev::Server;
use actix_web::web::Data;
use actix_web::{web, App, HttpServer};
use actix_web_flash_messages::storage::CookieMessageStore;
use actix_web_flash_messages::FlashMessagesFramework;
use actix_web_lab::middleware::from_fn;
use secrecy::{ExposeSecret, Secret};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tracing_actix_web::TracingLogger;

pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    pub async fn build(configuration: Settings) -> Result<Self, anyhow::Error> {
        let connection_pool = get_connection_pool(&configuration.database);
        let sender_email = SubscriberEmail::parse(configuration.email_client.sender_email.clone())
            .expect("invalid sender address in config");
        let timeout = configuration.email_client.timeout();
        let email_client = EmailClient::new(
            configuration.email_client.base_url,
            sender_email,
            configuration.email_client.authorization_token,
            timeout,
        );

        let server_address = format!(
            "{}:{}",
            configuration.application.host, configuration.application.port
        );
        let listener = TcpListener::bind(server_address)?;
        let port = listener.local_addr().unwrap().port();
        let server = run(
            listener,
            connection_pool,
            email_client,
            configuration.application.base_url,
            configuration.application.hmac_secret,
            configuration.redis_uri,
        )
        .await?;
        Ok(Self { port, server })
    }
    pub fn port(&self) -> u16 {
        self.port
    }
    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> {
        self.server.await
    }
}

// We need to define a wrapper type in order to retrieve the URL in the `subscribe` handler.
// Retrieval from the context _in actix-web_ is type-based, so using a raw String would expose conflicts
pub struct ApplicationBaseUrl(pub String);

// now that we added the redis session middleware, this is now async
async fn run(
    listener: TcpListener,
    connection_pool: PgPool,
    email_client: EmailClient,
    base_url: String,
    hmac_secret: Secret<String>,
    redis_uri: Secret<String>,
) -> Result<Server, anyhow::Error> {
    // web::Data wraps this as an Arc so that each worker can get a pointer to the PgConnection
    let connection_pool = web::Data::new(connection_pool);
    let email_client = web::Data::new(email_client);
    let base_url = web::Data::new(ApplicationBaseUrl(base_url));
    let secret_key = Key::from(hmac_secret.expose_secret().as_bytes());
    let message_store = CookieMessageStore::builder(secret_key.clone()).build();
    let message_framework = FlashMessagesFramework::builder(message_store).build();
    let redis_store = RedisSessionStore::new(redis_uri.expose_secret())
        .await
        .map_err(|e| {
            tracing::error!("Failed to connect to redis: {:?}", e);
            e
        })?;

    let server = HttpServer::new(move || {
        App::new()
            .wrap(message_framework.clone())
            .wrap(SessionMiddleware::new(
                redis_store.clone(),
                secret_key.clone(),
            ))
            .wrap(TracingLogger::default())
            .route("/health", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
            .route("/subscriptions/confirm", web::get().to(confirm))
            .route("/login", web::get().to(login_form))
            .route("/", web::get().to(home))
            .route("/login", web::post().to(login))
            .service(
                web::scope("/admin")
                    .wrap(from_fn(reject_anonymous_users))
                    .route("/dashboard", web::get().to(admin_dashboard))
                    .route("/password", web::get().to(change_password_form))
                    .route("/password", web::post().to(change_password))
                    .route("/logout", web::post().to(log_out))
                    .route("/newsletters", web::get().to(publish_newsletter_form))
                    .route("/newsletters", web::post().to(publish_newsletter)),
            )
            .app_data(connection_pool.clone())
            .app_data(email_client.clone())
            .app_data(base_url.clone())
            .app_data(Data::new(HmacSecret(hmac_secret.clone())))
    })
    .listen(listener)?
    .run();
    Ok(server)
}

#[derive(Clone)]
pub struct HmacSecret(pub Secret<String>);

pub fn get_connection_pool(configuration: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy_with(configuration.with_db())
}

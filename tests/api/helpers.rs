use std::net::TcpListener;

use fake::{Fake, Faker};
use once_cell::sync::Lazy;
use secrecy::Secret;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use uuid::Uuid;
use zero2prod::{
    configuration::{get_configuration, DatabaseSettings},
    domain::SubscriberEmail,
    email_client::EmailClient,
    telemetry::{get_subscriber, init_subscriber},
};

// Ensure that the `tracing` stack is only initialized once
static TRACING: Lazy<()> = Lazy::new(|| {
    let default_filter_level = "info".to_string();
    let subscriber_name = "test".to_string();
    // We cannot assign the output of `get_subscriber` to a variable based on the value TEST_LOG because the sink is part of the type
    // returned by `get_subscriber` so they are not the same type.  We could work around it, but this is the most straight-forward way of moving forward
    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::stdout);
        init_subscriber(subscriber);
    } else {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::sink);
        init_subscriber(subscriber);
    }
});

#[derive(Debug)]
pub struct TestApp {
    pub server_address: String,
    pub db_pool: PgPool,
}

pub async fn spawn_app() -> TestApp {
    // The first time this is invoked, the code in `TRACING` will be executed. All other invocations will skip execution
    Lazy::force(&TRACING);
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind a random port");
    let port = listener.local_addr().unwrap().port();

    let mut configuration = get_configuration().expect("Failed to read configuration");
    // prefix DB name so we can drop more easily
    configuration.database.database_name = format!("z2p-{}", Uuid::new_v4());
    let connection_pool = configure_database_for_tests(&configuration.database).await;
    let sender = SubscriberEmail::parse(configuration.email_client.sender_email.clone())
        .expect("invalid sender address in config");
    let timeout = configuration.email_client.timeout();
    let email_client = EmailClient::new(
        configuration.email_client.base_url,
        sender,
        Secret::new(Faker.fake()),
        timeout,
    );

    let server = zero2prod::startup::run(listener, connection_pool.clone(), email_client)
        .expect("failed to bind address");
    let _ = tokio::spawn(server);
    TestApp {
        server_address: format!("http://127.0.0.1:{}", port),
        db_pool: connection_pool,
    }
}

// A little hacky, but we'll create a unique DB for every test so that we don't have to deal with transactions and rollback
pub async fn configure_database_for_tests(config: &DatabaseSettings) -> PgPool {
    let mut connection = PgConnection::connect_with(&config.without_db())
        .await
        .expect("failed to connect to Postgres");

    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, config.database_name).as_str())
        .await
        .expect("failed to create ephemeral database");

    let connection_pool = PgPool::connect_with(config.with_db())
        .await
        .expect("Failed to connect to ephemeral database");

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate the DB");

    connection_pool
}

use std::net::TcpListener;

use once_cell::sync::Lazy;
use secrecy::{ExposeSecret, Secret};
use sqlx::{Connection, Executor, PgConnection, PgPool};
use uuid::Uuid;
use zero2prod::{
    configuration::{get_configuration, DatabaseSettings},
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
struct TestApp {
    server_address: String,
    db_connection_url: Secret<String>,
}

#[tokio::test]
async fn health_check_works() {
    let test_app = spawn_app().await;
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/health", &test_app.server_address))
        .send()
        .await
        .expect("Failed to execute request");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}

#[tokio::test]
async fn subscribe_returns_200_for_valid_form_data() {
    let test_app = spawn_app().await;

    let mut connection = PgConnection::connect(&test_app.db_connection_url.expose_secret())
        .await
        .expect("failed to connect to postgres");
    let client = reqwest::Client::new();

    let body = "name=jose%20cuervo&email=josecuervo%40test.com";

    let response = client
        .post(&format!("{}/subscriptions", &test_app.server_address))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("failed to execute request");
    assert_eq!(200, response.status().as_u16());

    let saved = sqlx::query!("SELECT email, name from subscriptions",)
        .fetch_one(&mut connection)
        .await
        .expect("failed to fetch saved subscription");

    assert_eq!(saved.email, "josecuervo@test.com");
    assert_eq!(saved.name, "jose cuervo");
}

#[tokio::test]
async fn subscribe_returns_201_when_already_subscribed() {
    let test_app = spawn_app().await;
    let mut connection = PgConnection::connect(&test_app.db_connection_url.expose_secret())
        .await
        .expect("failed to connect to postgres");
    let client = reqwest::Client::new();

    let body = "name=Imma%20Dup&email=immadup%40test.com";

    let response = client
        .post(&format!("{}/subscriptions", &test_app.server_address))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("failed to execute request");
    assert_eq!(200, response.status().as_u16());

    let saved = sqlx::query!("SELECT email, name from subscriptions",)
        .fetch_one(&mut connection)
        .await
        .expect("failed to fetch saved subscription");

    assert_eq!(saved.email, "immadup@test.com");
    assert_eq!(saved.name, "Imma Dup");

    // resend the same info
    let response = client
        .post(&format!("{}/subscriptions", &test_app.server_address))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("failed to execute request");

    // somewhat controversial as to what the correct response to this would be, we'll go with 204 No Content since this isn't a real app
    assert_eq!(204, response.status().as_u16());
}

#[tokio::test]
async fn subscribe_returns_400_when_data_missing() {
    let test_app = spawn_app().await;
    let client = reqwest::Client::new();
    let test_cases = vec![
        ("name=jane%20doe", "missing the email"),
        ("email=janedoe%40test.com", "missing the name"),
        ("", "both fields missing"),
    ];

    for (invalid_body, error_message) in test_cases {
        let response = client
            .post(&format!("{}/subscriptions", &test_app.server_address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(invalid_body)
            .send()
            .await
            .expect("failed to execute request");
        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not fail with 400 Bad Request when payload was {}.",
            error_message
        );
    }
}

async fn spawn_app() -> TestApp {
    // The first time this is invoked, the code in `TRACING` will be executed. All other invocations will skip execution
    Lazy::force(&TRACING);
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind a random port");
    let port = listener.local_addr().unwrap().port();

    let mut configuration = get_configuration().expect("Failed to read configuration");
    // prefix DB name so we can drop more easily
    configuration.database.database_name = format!("z2p-{}", Uuid::new_v4().to_string());
    let connection_pool = configure_database_for_tests(&configuration.database).await;
    let server =
        zero2prod::startup::run(listener, connection_pool).expect("failed to bind address");
    let _ = tokio::spawn(server);
    TestApp {
        server_address: format!("http://127.0.0.1:{}", port),
        db_connection_url: configuration.database.connection_string(),
    }
}

// A little hacky, but we'll create a unique DB for every test so that we don't have to deal with transactions and rollback
pub async fn configure_database_for_tests(config: &DatabaseSettings) -> PgPool {
    let mut connection =
        PgConnection::connect(&config.connection_string_without_db().expose_secret())
            .await
            .expect("failed to connect to Postgres");

    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, config.database_name).as_str())
        .await
        .expect("failed to create ephemeral database");

    let connection_pool = PgPool::connect(&config.connection_string().expose_secret())
        .await
        .expect("Failed to connect to ephemeral database");

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate the DB");

    connection_pool
}

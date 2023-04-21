use std::net::TcpListener;

use sqlx::{Connection, PgConnection, PgPool};
use zero2prod::configuration::get_configuration;

#[tokio::test]
async fn health_check_works() {
    let server_address = spawn_app().await;
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/health", &server_address))
        .send()
        .await
        .expect("Failed to execute request");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}

#[tokio::test]
async fn subscribe_returns_200_for_valid_form_data() {
    let address = spawn_app().await;
    let configuration = get_configuration().expect("failed to read configuration");

    let mut connection = PgConnection::connect(&configuration.database.connection_string())
        .await
        .expect("failed to connect to postgres");
    let client = reqwest::Client::new();

    let body = "name=jose%20cuervo&email=josecuervo%40test.com";

    let response = client
        .post(&format!("{}/subscriptions", &address))
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
async fn subscribe_returns_400_when_data_missing() {
    let address = spawn_app().await;
    let client = reqwest::Client::new();
    let test_cases = vec![
        ("name=jane%20doe", "missing the email"),
        ("email=janedoe%40test.com", "missing the name"),
        ("", "both fields missing"),
    ];

    for (invalid_body, error_message) in test_cases {
        let response = client
            .post(&format!("{}/subscriptions", &address))
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

async fn spawn_app() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind a random port");
    let port = listener.local_addr().unwrap().port();

    let configuration = get_configuration().expect("Failed to read configuration");
    let connection_pool = PgPool::connect(&configuration.database.connection_string())
        .await
        .expect("failed to connect to postgres");
    let server =
        zero2prod::startup::run(listener, connection_pool).expect("failed to bind address");
    let _ = tokio::spawn(server);
    format!("http://127.0.0.1:{}", port)
}

use std::net::TcpListener;

#[tokio::test]
async fn health_check_works() {
    let server_address = spawn_app();
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/health", &server_address))
        .send()
        .await
        .expect("Failed to execute request");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}

// launch app in background, somehow
fn spawn_app() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind a random port");
    let port = listener.local_addr().unwrap().port();
    let server = zero2prod::run(listener).expect("failed to bind address");
    let _ = tokio::spawn(server);
    format!("http://127.0.0.1:{}", port)
}

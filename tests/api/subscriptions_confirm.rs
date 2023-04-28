use reqwest::Url;
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

use crate::helpers::spawn_app;

#[tokio::test]
async fn confirmations_without_token_are_rejected_with_a_400() {
    let app = spawn_app().await;

    let response = reqwest::get(&format!("{}/subscriptions/confirm", app.server_address))
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test]
async fn the_link_returned_by_subscribe_returns_a_200_if_called() {
    let app = spawn_app().await;
    let body = "name=jose&email=josecuervo%40example.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body.into()).await;

    let email_request = &app.email_server.received_requests().await.unwrap()[0];
    let body: serde_json::Value = serde_json::from_slice(&email_request.body).unwrap();

    let get_link = |s: &str| {
        let links: Vec<_> = linkify::LinkFinder::new()
            .links(s)
            .filter(|link| *link.kind() == linkify::LinkKind::Url)
            .collect();
        assert_eq!(links.len(), 1, "should have been a link in {}", s);
        links[0].as_str().to_owned()
    };

    let raw_confirmation_link = &get_link(&body["HtmlBody"].as_str().unwrap());
    let mut confirmation_link = Url::parse(raw_confirmation_link).unwrap();
    // make sure we don't call random APIs on the web
    assert_eq!(confirmation_link.host_str().unwrap(), "127.0.0.1");

    // for tests, we have to add the port back in =/
    confirmation_link.set_port(Some(app.port)).unwrap();

    let response = reqwest::get(confirmation_link).await.unwrap();

    assert_eq!(response.status().as_u16(), 200);
}

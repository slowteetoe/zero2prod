use crate::helpers::{assert_is_redirected_to, spawn_app, ConfirmationLinks, TestApp};

use fake::{
    faker::{internet::en::SafeEmail, name::en::Name},
    Fake,
};
use std::time::Duration;
use wiremock::{
    matchers::{any, method, path},
    Mock, MockBuilder, ResponseTemplate,
};

#[tokio::test]
async fn newsletters_are_not_delivered_to_unconfirmed_subscribers() {
    let app = spawn_app().await;
    create_unconfirmed_subscriber(&app).await;

    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        // there shouldn't be any requests to Postmark
        .expect(0)
        .mount(&app.email_server)
        .await;

    let newsletter_request_body = serde_json::json!({
        "title":"Newsletter title",
        "content": {
            "text": "Newsletter body as text",
            "html": "<p>Newsletter body as html</p>",
        },
        "idempotency_key": uuid::Uuid::new_v4().to_string(),
    });

    // login since we're not using basic auth any longer
    app.post_login(&serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    }))
    .await;

    let response = app.post_newsletters(newsletter_request_body).await;

    assert_eq!(response.status().as_u16(), 200);
    // Mock verifies on drop that we haven't sent the newsletter email
}

#[tokio::test]
async fn newsletters_are_delivered_to_confirmed_subscribers() {
    let app = spawn_app().await;

    create_confirmed_subscriber(&app).await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
        "content": {
            "text": "Newsletter body as plain text",
            "html": "<p>Newsletter body as html</p>",
        },
        "idempotency_key": uuid::Uuid::new_v4().to_string(),
    });

    // login
    app.post_login(&serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    }))
    .await;

    let response = app.post_newsletters(newsletter_request_body).await;

    assert_eq!(response.status().as_u16(), 200);
    // mock verifies on Drop that we have sent the newsletter email
}

#[tokio::test]
async fn newletters_return_400_for_invalid_data() {
    let app = spawn_app().await;
    let test_cases = vec![
        (
            serde_json::json!({
                "title": "Newsletter title",
            }),
            "Missing content",
        ),
        (
            serde_json::json!({
                "content": {
                    "text": "Newsletter body as plain text",
                    "html": "<p>Newsletter body as html</p>",
                }
            }),
            "Missing title",
        ),
    ];

    // login
    app.post_login(&serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    }))
    .await;

    for (invalid_body, error_msg) in test_cases {
        let response = app.post_newsletters(invalid_body).await;

        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not fail with 400 Bad Request when the payload was {}",
            error_msg
        );
    }
}

#[tokio::test]
async fn unauthenticated_requests_are_directed_to_login() {
    let app = spawn_app().await;

    let response = app
        .post_newsletters(serde_json::json!({
            "title": "Newletter title",
            "content": {
                "text": "Newsletter body as plain text",
                "html": "<p>Newsletter body as html</p>",
            }
        }))
        .await;

    assert_is_redirected_to(&response, "/login");
}

#[tokio::test]
async fn you_must_be_logged_in_to_publish_a_newsletter() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
        "text_content": "Newsletter body as plain text",
        "html_content": "<p>Newsletter body as HTML</p>",
        "idempotency_key": uuid::Uuid::new_v4().to_string()
    });
    let response = app.post_newsletters_form(&newsletter_request_body).await;

    // Assert
    assert_is_redirected_to(&response, "/login");
}

#[tokio::test]
async fn newsletter_creation_is_idempotent() {
    let app = spawn_app().await;
    create_confirmed_subscriber(&app).await;
    app.test_user.login(&app).await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    // submit the newsletter form
    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
            "text_content": "Newsletter body as plain text",
            "html_content": "<p>Newsletter body as html</p>",
        "idempotency_key": uuid::Uuid::new_v4().to_string(),
    });

    let response = app.post_newsletters_form(&newsletter_request_body).await;
    assert_is_redirected_to(&response, "/admin/dashboard");
    // follow the redirect
    let html_page = app.get_admin_dashboard_html().await;
    assert!(
        html_page.contains("The newsletter issue has been published"),
        "{}",
        html_page
    );

    // submit the newsletter form AGAIN with the same data

    let response = app.post_newsletters_form(&newsletter_request_body).await;
    assert_is_redirected_to(&response, "/admin/dashboard");
    // follow the redirect
    let html_page = app.get_admin_dashboard_html().await;
    assert!(
        html_page.contains("The newsletter issue has been published"),
        "{}",
        html_page
    );
}

#[tokio::test]
async fn concurrent_form_submission_is_handled_gracefully() {
    let app = spawn_app().await;
    create_confirmed_subscriber(&app).await;
    app.test_user.login(&app).await;

    Mock::given(path("/email"))
        .and(method("POST"))
        // set a long delay to ensure that the second request arrives before the first one completes
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(2)))
        .expect(1)
        .mount(&app.email_server)
        .await;

    // Submit two newsletter forms concurrently
    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
            "text_content": "Newsletter body as plain text",
            "html_content": "<p>Newsletter body as html</p>",
        "idempotency_key": uuid::Uuid::new_v4().to_string(),
    });

    let response1 = app.post_newsletters_form(&newsletter_request_body);
    let response2 = app.post_newsletters_form(&newsletter_request_body);

    let (response1, response2) = tokio::join!(response1, response2);

    assert_eq!(response1.status(), response2.status());
    assert_eq!(
        response1.text().await.unwrap(),
        response2.text().await.unwrap()
    );
}

fn when_sending_an_email() -> MockBuilder {
    Mock::given(path("/email")).and(method("POST"))
}

#[tokio::test]
async fn transient_errors_do_not_cause_duplicate_deliveries_on_retry() {
    let app = spawn_app().await;
    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
            "text_content": "Newsletter body as plain text",
            "html_content": "<p>Newsletter body as html</p>",
        "idempotency_key": uuid::Uuid::new_v4().to_string(),
    });
    // Two subscribers instead of just one
    create_confirmed_subscriber(&app).await;
    create_confirmed_subscriber(&app).await;
    app.test_user.login(&app).await;

    // submit newsletter form, delivery fails for the second subscriber
    when_sending_an_email()
        .respond_with(ResponseTemplate::new(200))
        .up_to_n_times(1)
        .expect(1)
        .mount(&app.email_server)
        .await;
    when_sending_an_email()
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .expect(1)
        .mount(&app.email_server)
        .await;

    let response = app.post_newsletters_form(&newsletter_request_body).await;
    assert_eq!(response.status().as_u16(), 500);

    // Now retry sending the form, email delivery will succeed for both users now
    when_sending_an_email()
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .named("Delivery Retry")
        .mount(&app.email_server)
        .await;

    let response = app.post_newsletters_form(&newsletter_request_body).await;
    assert_eq!(response.status().as_u16(), 303);

    // Mock verifies on Drop that we did not send out duplicates
}

async fn create_unconfirmed_subscriber(app: &TestApp) -> ConfirmationLinks {
    let name: String = Name().fake();
    let email: String = SafeEmail().fake();
    let body = serde_urlencoded::to_string(&serde_json::json!({
        "name": name,
        "email": email,
    }))
    .unwrap();

    // since we are using 'mount_as_scoped', we get back a MockGuard, when that goes out of scope
    // the Drop impl causes the underlying MockServer to stop supporting this route AND check the expectation(s)
    let _mock_guard = Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .named("Create unconfirmed subscriber")
        .expect(1)
        .mount_as_scoped(&app.email_server)
        .await;

    app.post_subscriptions(body.into())
        .await
        .error_for_status()
        .unwrap();

    let email_request = &app
        .email_server
        .received_requests()
        .await
        .unwrap()
        .pop()
        .unwrap();

    app.get_confirmation_links(email_request)
}

async fn create_confirmed_subscriber(app: &TestApp) {
    // we can re-use the existing helper and just add the extra step to call the confirmation link
    let confirmation_link = create_unconfirmed_subscriber(app).await;

    reqwest::get(confirmation_link.html)
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}

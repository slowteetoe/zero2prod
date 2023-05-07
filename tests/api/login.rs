use crate::helpers::{assert_is_redirected_to, spawn_app};

#[tokio::test]
async fn an_error_flash_message_is_set_on_failure() {
    let app = spawn_app().await;

    let login_body = serde_json::json!(
        {
            "username": "random-username",
            "password": "random-password"
        }
    );

    let response = app.post_login(&login_body).await;
    // we got redirected
    assert_is_redirected_to(&response, "/login");

    // page shows error message
    let html_page = app.get_login_html().await;
    assert!(html_page.contains(r#"Authentication failed"#));

    // reload the page, and verify that the flash error message is gone
    let html_page = app.get_login_html().await;
    assert!(!html_page.contains(r#"Authentication failed"#));
}

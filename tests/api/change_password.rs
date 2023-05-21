use crate::helpers::{assert_is_redirected_to, spawn_app};
use uuid::Uuid;

#[tokio::test]
async fn you_must_be_logged_in_to_see_the_change_password_form() {
    let app = spawn_app().await;
    let response = app.get_change_password().await;

    assert_is_redirected_to(&response, "/login")
}

#[tokio::test]
async fn you_must_be_logged_in_to_change_your_password() {
    let app = spawn_app().await;
    let new_password = Uuid::new_v4().to_string();
    let response = app
        .post_change_password(&serde_json::json!({
            "current_password": Uuid::new_v4().to_string(),
            "new_password": &new_password,
            "new_password_check": &new_password,
        }))
        .await;

    assert_is_redirected_to(&response, "/login");
}

#[tokio::test]
async fn new_password_fields_must_match() {
    let app = spawn_app().await;
    let new_password = Uuid::new_v4().to_string();
    let another_new_password = Uuid::new_v4().to_string();

    // login
    app.post_login(&serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    }))
    .await;

    // try to change password
    let response = app
        .post_change_password(&serde_json::json!({
            "current_password": &app.test_user.password,
            "new_password": new_password,
            "new_password_check": another_new_password,
        }))
        .await;

    assert_is_redirected_to(&response, "/admin/password");

    // follow redirect
    let html_page = app.get_change_password_html().await;
    assert!(html_page.contains(
        "<p><i>You entered two different new passwords - the field values must match.</i></p>"
    ));
}

#[tokio::test]
async fn new_password_must_be_over_12_chars() {
    let app = spawn_app().await;
    let too_short_password = "tooshort".to_owned();

    // login
    app.post_login(&serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    }))
    .await;

    // try to change password
    let response = app
        .post_change_password(&serde_json::json!({
            "current_password": &app.test_user.password,
            "new_password": too_short_password,
            "new_password_check": too_short_password,
        }))
        .await;

    assert_is_redirected_to(&response, "/admin/password");

    // follow redirect
    let html_page = app.get_change_password_html().await;
    assert!(
        &html_page.contains("<p><i>Password length must be between 12 and 128 characters.</i></p>"),
        "html was {}",
        &html_page,
    );
}

#[tokio::test]
async fn current_password_must_be_valid() {
    let app = spawn_app().await;

    let wrong_password = Uuid::new_v4().to_string();
    let new_password = Uuid::new_v4().to_string();

    // login correctly
    app.post_login(&serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    }))
    .await;

    // try to change password
    let response = app
        .post_change_password(&serde_json::json!({
            "current_password": wrong_password,
            "new_password": new_password,
            "new_password_check": new_password,
        }))
        .await;

    assert_is_redirected_to(&response, "/admin/password");

    // follow redirect
    let html_page = app.get_change_password_html().await;
    assert!(html_page.contains("<p><i>The current password is incorrect.</i></p>"));
}

#[tokio::test]
async fn changing_password_works() {
    let app = spawn_app().await;

    let new_password = Uuid::new_v4().to_string();

    // login correctly
    app.post_login(&serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    }))
    .await;

    // try to change password
    let response = app
        .post_change_password(&serde_json::json!({
            "current_password": app.test_user.password,
            "new_password": new_password,
            "new_password_check": new_password,
        }))
        .await;

    assert_is_redirected_to(&response, "/admin/dashboard");

    // follow the redirect
    let html_page = app.get_change_password_html().await;
    assert!(
        html_page.contains("Your password has been changed."),
        "{}",
        html_page
    );

    // logout
    let response = app.post_logout().await;
    assert_is_redirected_to(&response, "/login");

    // follow redirect
    let html_page = app.get_login_html().await;
    assert!(html_page.contains("<p><i>You have successfully logged out.</i></p>"));

    // login using the newly changed password
    let response = app
        .post_login(&serde_json::json!({
            "username": &app.test_user.username,
            "password": new_password,
        }))
        .await;
    assert_is_redirected_to(&response, "/admin/dashboard")
}

use crate::helpers::{assert_is_redirected_to, spawn_app};

#[tokio::test]
async fn you_must_be_logged_in_to_access_the_admin_dashboard() {
    let app = spawn_app().await;
    let response = app.get_admin_dashboard().await;
    assert_is_redirected_to(&response, "/login");
}

#[tokio::test]
async fn logout_clears_session_state() {
    let app = spawn_app().await;

    // login
    let response = app
        .post_login(&serde_json::json!({
            "username": &app.test_user.username,
            "password": &app.test_user.password,
        }))
        .await;
    assert_is_redirected_to(&response, "/admin/dashboard");

    // follow redirect
    let html_page = app.get_admin_dashboard_html().await;
    assert!(html_page.contains(&format!("Welcome {}", app.test_user.username)));

    // logout
    let response = app.post_logout().await;
    assert_is_redirected_to(&response, "/login");

    // follow redirect
    let html_page = app.get_login_html().await;
    assert!(
        html_page.contains(&format!("You have successfully logged out.")),
        "{}",
        html_page
    );

    // attempt to load admin panel
    let response = app.get_admin_dashboard().await;
    assert_is_redirected_to(&response, "/login");
}

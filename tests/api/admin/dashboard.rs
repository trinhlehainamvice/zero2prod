use crate::helpers::{assert_redirects_to, spawn_app};

#[tokio::test]
async fn invalid_credentials_redirects_to_login() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let login_form = serde_json::json!({
        "username": &app.test_user.username,
        "password": "invalid_password"
    });

    // Act 1 login
    let response = app.post_login(login_form).await;

    assert_redirects_to(&response, "/login");

    // Act 2 go to admin dashboard
    let response = app.get("/admin/dashboard").await;

    assert_redirects_to(&response, "/login");
}

#[tokio::test]
async fn admin_dashboard_is_accessible_when_logged_in() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let login_form = serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    });

    // Act 1 login
    let response = app.post_login(login_form).await;
    assert_redirects_to(&response, "/admin/dashboard");

    // Act 2 go to admin dashboard
    let response = app.get("/admin/dashboard").await;
    assert!(response.status().is_success());
}

#[tokio::test]
async fn clink_on_change_password_link_in_admin_dashboard_html() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let login_form = serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    });

    // Act 1 login
    let response = app.post_login(login_form).await;
    assert_redirects_to(&response, "/admin/dashboard");

    // Act 2 admin dashboard contain link to change password
    let dashboard_html = app.get_html("/admin/dashboard").await;
    assert!(dashboard_html.contains(r#"<a href="/admin/password">Change Password</a>"#));

    let response = app.get("/admin/password").await;
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn click_on_logout_link_in_admin_dashboard_redirects_to_login() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let login_form = serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    });

    // Act 1 login
    let response = app.post_login(login_form).await;
    assert_redirects_to(&response, "/admin/dashboard");

    // Act 2 logout
    let response = app.get("/admin/logout").await;
    assert_redirects_to(&response, "/login");
}

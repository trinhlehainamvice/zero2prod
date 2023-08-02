use crate::helpers::{assert_redirects_to, spawn_app};
use uuid::Uuid;

#[tokio::test]
async fn login_failed_redirects_to_login() {
    // Arrange
    let app = spawn_app().await.expect("Failed to spawn app");
    let username = Uuid::new_v4().to_string();
    let password = Uuid::new_v4().to_string();

    let login_form = serde_json::json!({
        "username": &username,
        "password": &password
    });

    // Act 1
    let response = app.post_login(login_form).await;

    // Assert
    assert_redirects_to(&response, "/login");

    // Act 2
    let login_html = app.get_login_html().await;

    // Assert
    assert!(login_html.contains(r#"<p><i>Invalid Username or Password</i></p>"#));
}

#[tokio::test]
async fn login_successfully_redirects_to_home() {
    // Arrange
    let app = spawn_app().await.expect("Failed to spawn app");

    let login_form = serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    });

    // Act
    let response = app.post_login(login_form).await;

    // Assert
    assert_redirects_to(&response, "/admin/dashboard");
}

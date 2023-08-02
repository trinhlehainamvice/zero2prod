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

    let cookie = response.cookies().find(|c| c.name() == "_flash").unwrap();

    let flash_message: serde_json::Value =
        serde_json::from_str(cookie.value()).expect("Failed to parse flash message");

    assert_eq!(flash_message["error"], "Invalid Username or Password");

    // Act 2
    let location = response.headers().get("Location").unwrap();
    let response = reqwest::Client::new()
        .get(&format!("{}{}", app.addr, location.to_str().unwrap()))
        .send()
        .await
        .expect("Failed to execute request.");

    // Assert
    assert_eq!(response.status().as_u16(), 200);
    assert!(response
        .text()
        .await
        .expect("Failed to read response body")
        .contains("<p><i>Invalid Username or Password</i></p>"));
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
    assert_redirects_to(&response, "/");
}

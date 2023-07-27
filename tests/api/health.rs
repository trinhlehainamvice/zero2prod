use crate::helpers::spawn_app;

#[tokio::test]
async fn check_health_check() {
    // Arrange
    let app = spawn_app().await.unwrap();

    // Act
    let response = reqwest::Client::new()
        .get(&format!("{}/health", app.addr))
        .send()
        .await
        .expect("Failed to execute request");

    // Assert
    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
} // _app_thread is dropped here after all tests are successful

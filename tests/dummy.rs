use std::net::TcpListener;
use zero2prod::run;

#[tokio::test]
async fn check_health_check() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
    let port = listener.local_addr().unwrap().port();
    let app = spawn_app(listener);

    // tokio spawn background thread an run app
    // We want to hold thread instance until tests finish (or end of function)
    // Then background thread will be dropped or terminated
    let _app_thread = tokio::spawn(app);

    let client = reqwest::Client::new();
    let response = client
        .get(&format!("http://127.0.0.1:{}/health_check", port))
        .send()
        .await
        .expect("Failed to execute request");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
} // _app_thread is dropped here after all tests are successful

#[tokio::test]
async fn test_200_success_post_subscribe_in_urlencoded_format() {
    // Arrange
    let listener = TcpListener::bind("127.0.0.1:1").expect("Failed to bind random port");
    let port = listener.local_addr().unwrap().port();
    let app = spawn_app(listener);
    let _app_thread = tokio::spawn(app);

    // Act
    let body = "name=Foo%20Bar&email=foobar%40example.com";
    let response = reqwest::Client::new()
        .post(&format!("http://127.0.0.1:{}/subscriptions", port))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("Failed to execute request");

    // Assert
    assert!(response.status().is_success());
}

#[tokio::test]
async fn test_400_fail_post_subscribe_in_urlencoded_format_when_missing_data() {
    // Arrange
    let listener = TcpListener::bind("127.0.0.1:2").expect("Failed to bind random port");
    let port = listener.local_addr().unwrap().port();
    let app = spawn_app(listener);
    let _app_thread = tokio::spawn(app);
    let test_cases = vec![
        ("email=foobar%40example.com", "Missing the name"),
        ("name=Foo%20Bar", "Missing the email"),
        ("", "Missing both name and email aka data form is empty"),
    ];

    // Act
    let req_builder = reqwest::Client::new()
        .post(&format!("http://127.0.0.1:{}/subscriptions", port))
        .header("Content-Type", "application/x-www-form-urlencoded");
    for (body, error) in test_cases {
        let response = req_builder
            .try_clone()
            .unwrap()
            .body(body)
            .send()
            .await
            .expect("Failed to execute request");

        // Assert
        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not fail 400 Bad Request with payload {}",
            error
        );
    }
}

async fn spawn_app(listener: TcpListener) -> std::io::Result<()> {
    run(listener)?.await
}

use zero2prod::run;

#[tokio::test]
async fn check_health_check() {
    let app = spawn_app();

    // tokio spawn background thread an run app
    // We want to hold thread instance until tests finish (or end of function)
    // Then background thread will be dropped or terminated
    let _app_thread = tokio::spawn(app);

    let client = reqwest::Client::new();
    let response = client
        .get("http://127.0.0.1:8000/health_check")
        .send()
        .await
        .expect("Failed to execute request");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
} // _app_thread is dropped here after all tests are successful

async fn spawn_app() -> std::io::Result<()> {
    run("127.0.0.1:8000")?.await
}

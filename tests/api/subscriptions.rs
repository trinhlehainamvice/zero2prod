use crate::helpers::spawn_app;

#[tokio::test]
async fn test_200_success_post_subscribe_in_urlencoded_format() {
    // Arrange
    let app = spawn_app().await.unwrap();

    // Act
    let body = "name=Foo%20Bar&email=foobar%40example.com";
    let response = app.post_subscriptions(body.into()).await;

    // Assert
    assert!(response.status().is_success());
}

#[tokio::test]
async fn test_400_fail_post_subscribe_in_urlencoded_format_when_missing_data() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let test_cases = vec![
        ("email=foobar%40example.com", "Missing the name"),
        ("name=Foo%20Bar", "Missing the email"),
        ("", "Missing both name and email aka data form is empty"),
    ];

    // Act
    for (body, error) in test_cases {
        let response = app.post_subscriptions(body.into()).await;

        // Assert
        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not fail 400 Bad Request with payload {}",
            error
        );
    }
}

#[tokio::test]
async fn test_200_success_connect_to_database_and_subscribe_valid_data_in_urlencoded_format() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let body = "name=Foo%20Bar&email=foobar%40example.com";

    // Act
    let response = app.post_subscriptions(body.into()).await;

    // Assert
    assert!(response.status().is_success());
}

#[tokio::test]
async fn test_query_subscriptions_name_from_database() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let body = "name=Foo%20Bar&email=foobar%40example.com";

    // Act
    let response = app.post_subscriptions(body.into()).await;

    // Assert
    assert!(response.status().is_success());

    // Act
    let subscriber = sqlx::query!("SELECT email, name FROM subscriptions")
        .fetch_one(&app.db_connection_pool)
        .await
        .expect("Failed to fetch saved subscriptions");

    // Assert
    assert_eq!("foobar@example.com", subscriber.email);
    assert_eq!("Foo Bar", subscriber.name);
}

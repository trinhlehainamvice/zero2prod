use crate::helpers::TestApp;
use fake::faker::internet::en::SafeEmail;
use fake::faker::name::en::Name;
use fake::Fake;

#[tokio::test]
async fn post_subscribe_in_urlencoded_valid_format_ret_200() {
    // Arrange
    let app = TestApp::builder().build().await.unwrap();

    // Act
    let body = "name=Foo%20Bar&email=foobar%40example.com";
    let response = app.post_subscriptions(body.into()).await;

    // Assert
    assert!(response.status().is_success());
}

#[tokio::test]
async fn post_subscribe_in_urlencoded_format_with_missing_data_ret_400() {
    // Arrange
    let app = TestApp::builder().build().await.unwrap();
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
async fn query_pending_confirmation_subscriber_after_user_send_subscription_form_ret_200() {
    // Arrange
    let app = TestApp::builder().build().await.unwrap();
    let name: String = Name().fake();
    let email: String = SafeEmail().fake();
    let body = serde_json::json!({
        "name": name,
        "email": email
    });

    // Act
    let response = app
        .post_subscriptions(serde_urlencoded::to_string(body.clone()).unwrap())
        .await;

    // Assert
    assert!(response.status().is_success());

    // Act
    let subscriber = sqlx::query!("SELECT email, name, status FROM subscriptions")
        .fetch_one(&app.pg_pool)
        .await
        .expect("Failed to fetch saved subscriptions");

    // Assert
    assert_eq!(body["email"].as_str().unwrap(), subscriber.email);
    assert_eq!(body["name"].as_str().unwrap(), subscriber.name);
    assert_eq!("pending", subscriber.status);
}

#[tokio::test]
async fn click_confirmation_link_in_email_and_query_subscriber_status_as_confirmed_ret_200() {
    // Arrange
    let app = TestApp::builder().build().await.unwrap();
    let name: String = Name().fake();
    let email: String = SafeEmail().fake();
    let body = serde_json::json!({
        "name": name,
        "email": email
    });

    // Act
    let response = app
        .post_subscriptions(serde_urlencoded::to_string(body.clone()).unwrap())
        .await;

    // Assert
    assert!(response.status().is_success());

    // Assert
    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions")
        .fetch_one(&app.pg_pool)
        .await
        .expect("Failed to fetch saved subscriptions");

    assert_eq!(body["email"].as_str().unwrap(), saved.email);
    assert_eq!(body["name"].as_str().unwrap(), saved.name);
    assert_eq!("pending", saved.status);

    let confirmation_link = app
        .get_confirmation_links(body["email"].as_str().unwrap())
        .await;
    app.click_confirmation_link(&confirmation_link).await;

    // Assert
    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions")
        .fetch_one(&app.pg_pool)
        .await
        .expect("Failed to fetch saved subscriptions");

    assert_eq!(body["email"].as_str().unwrap(), saved.email);
    assert_eq!(body["name"].as_str().unwrap(), saved.name);
    assert_eq!("confirmed", saved.status);
}

#[tokio::test]
async fn drop_subscription_token_column_to_cause_internal_error_when_send_subscription() {
    // Arrange
    let app = TestApp::builder().build().await.unwrap();
    let name: String = Name().fake();
    let email: String = SafeEmail().fake();
    let body = serde_json::json!({
        "name": name,
        "email": email
    });

    sqlx::query!(
        r#"
        ALTER TABLE subscription_tokens
        DROP COLUMN subscription_token;
        "#,
    )
    .execute(&app.pg_pool)
    .await
    .unwrap();

    // Act
    let response = app
        .post_subscriptions(serde_urlencoded::to_string(body).unwrap())
        .await;

    // Assert
    assert_eq!(500, response.status().as_u16());
}

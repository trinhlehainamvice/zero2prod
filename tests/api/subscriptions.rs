use crate::helpers::{spawn_app, ConfirmationLinks};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn post_subscribe_in_urlencoded_valid_format_ret_200() {
    // Arrange
    let app = spawn_app().await.unwrap();

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

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

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    // Act
    let response = app.post_subscriptions(body.into()).await;

    // Assert
    assert!(response.status().is_success());
}

#[tokio::test]
async fn query_pending_confirmation_subscriber_after_user_send_subscription_form_ret_200() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let body = "name=Foo%20Bar&email=foobar%40example.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    // Act
    let response = app.post_subscriptions(body.into()).await;

    // Assert
    assert!(response.status().is_success());

    // Act
    let subscriber = sqlx::query!("SELECT email, name, status FROM subscriptions")
        .fetch_one(&app.pg_pool)
        .await
        .expect("Failed to fetch saved subscriptions");

    // Assert
    assert_eq!("foobar@example.com", subscriber.email);
    assert_eq!("Foo Bar", subscriber.name);
    assert_eq!("pending_confirmation", subscriber.status);
}

#[tokio::test]
async fn send_confirmation_to_subscriber_email_with_link_return_200() {
    // Arrange
    let app = spawn_app().await.unwrap();

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    // Act
    let body = "name=Foo%20Bar&email=foobar%40example.com";
    let response = app.post_subscriptions(body.into()).await;

    let email_request = &app.email_server.received_requests().await.unwrap()[0];
    let confirmation_links = ConfirmationLinks::get_confirmation_link(&email_request);
    let html_link = confirmation_links.html;
    let text_link = confirmation_links.plain_text;

    // Confirmation link in HTML body and plain text body need to be the same
    assert_eq!(html_link, text_link);

    // Assert
    assert!(response.status().is_success());
}

#[tokio::test]
async fn click_confirmation_link_in_email_and_query_subscriber_status_as_confirmed_ret_200() {
    // Arrange
    let app = spawn_app().await.unwrap();
    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;
    let body = "name=Foo%20Bar&email=foobar%40example.com";

    // Act
    let response = app.post_subscriptions(body.into()).await;

    // Assert
    assert!(response.status().is_success());

    // Assert
    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions")
        .fetch_one(&app.pg_pool)
        .await
        .expect("Failed to fetch saved subscriptions");

    assert_eq!("foobar@example.com", saved.email);
    assert_eq!("Foo Bar", saved.name);
    assert_eq!("pending_confirmation", saved.status);

    // Arrange
    let email_request = &app.email_server.received_requests().await.unwrap()[0];
    let confirmation_links = ConfirmationLinks::get_confirmation_link(email_request);
    let mut confirmation_link = reqwest::Url::parse(&confirmation_links.html).unwrap();
    confirmation_link.set_port(Some(app.port)).unwrap();

    // Act
    reqwest::get(confirmation_link)
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    // Assert
    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions")
        .fetch_one(&app.pg_pool)
        .await
        .expect("Failed to fetch saved subscriptions");

    assert_eq!("foobar@example.com", saved.email);
    assert_eq!("Foo Bar", saved.name);
    assert_eq!("confirmed", saved.status);
}

#[tokio::test]
async fn internal_query_error_ret_500() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let body = "name=Foo%20Bar&email=foobar%40example.com";

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
    let response = app.post_subscriptions(body.into()).await;

    // Assert
    assert_eq!(500, response.status().as_u16());
}

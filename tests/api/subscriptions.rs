use crate::helpers::{get_confirmation_link, spawn_app};
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
        .mount(&app.email_client)
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
        .mount(&app.email_client)
        .await;

    // Act
    let response = app.post_subscriptions(body.into()).await;

    // Assert
    assert!(response.status().is_success());

    // Act
    let subscriber = sqlx::query!("SELECT email, name, status FROM subscriptions")
        .fetch_one(&app.db_connection_pool)
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
        .mount(&app.email_client)
        .await;

    // Act
    let body = "name=Foo%20Bar&email=foobar%40example.com";
    let response = app.post_subscriptions(body.into()).await;

    let email_request = &app.email_client.received_requests().await.unwrap()[0];
    let html_link = get_confirmation_link(email_request, "HtmlBody");
    let text_link = get_confirmation_link(email_request, "TextBody");

    // Confirmation link in HTML body and plain text body need to be the same
    assert_eq!(html_link, text_link);

    // Assert
    assert!(response.status().is_success());
}

#[tokio::test]
async fn get_confirm_without_check_token_ret_200() {
    // Arrange
    let app = spawn_app().await.unwrap();

    // Act
    let response = app.get_confirmation("abc").await;

    // Assert
    assert!(response.status().is_success());
}

#[tokio::test]
async fn post_subscriber_and_get_confirm_with_check_token_as_app_base_url_ret_200() {
    // Arrange
    let app = spawn_app().await.unwrap();
    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_client)
        .await;
    let body = "name=Foo%20Bar&email=foobar%40example.com";

    // Act
    let response = app.post_subscriptions(body.into()).await;

    // Assert
    assert!(response.status().is_success());

    // Act
    let email_request = &app.email_client.received_requests().await.unwrap()[0];
    let raw_confirmation_link = get_confirmation_link(email_request, "HtmlBody");
    let mut confirmation_link = reqwest::Url::parse(&raw_confirmation_link).unwrap();

    // NOTE: If app_base_url is a localhost, we need to add the port to access the confirmation link locally
    confirmation_link.set_port(Some(app.port)).unwrap();

    // Assert
    assert_eq!(confirmation_link.host_str().unwrap(), "127.0.0.1");

    // Act
    let response = reqwest::get(confirmation_link).await.unwrap();

    // Assert
    assert!(response.status().is_success());
}

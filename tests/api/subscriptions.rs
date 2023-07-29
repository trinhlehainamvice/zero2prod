use crate::helpers::spawn_app;
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

    let get_link = |s: &str| {
        let links: Vec<_> = linkify::LinkFinder::new()
            .links(s)
            .filter(|l| *l.kind() == linkify::LinkKind::Url)
            .collect();
        assert_eq!(links.len(), 1);
        links[0].as_str().to_owned()
    };

    let email_request = &app.email_client.received_requests().await.unwrap()[0];
    let body: serde_json::Value = serde_json::from_slice(&email_request.body).unwrap();
    let html_link = get_link(body["HtmlBody"].as_str().unwrap());
    let text_link = get_link(body["TextBody"].as_str().unwrap());

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

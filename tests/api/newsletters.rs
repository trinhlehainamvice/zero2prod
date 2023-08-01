use crate::helpers::spawn_app;
use uuid::Uuid;

#[tokio::test]
async fn publish_newsletters_unconfirmed_email_ret_500() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let body = "name=Foo%20Bar&email=foobar%40example.com";

    let _confirmed_links = app.create_unconfirmed_subscriber(body).await;

    // TODO:
}

#[tokio::test]
async fn publish_newsletters_confirmed_email_ret_200() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let body = "name=Foo%20Bar&email=foobar%40example.com";

    app.create_confirmed_subscriber(body).await;

    // TODO:
}

#[tokio::test]
async fn publish_newsletters_invalid_form_data_ret_400() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let body = "name=Foo%20Bar&email=foobar%40example.com";

    app.create_confirmed_subscriber(body).await;

    let newsletter_bodies = vec![
        (
            serde_json::json!({
                "content": {
                    "text": "Newsletter body as plain text",
                    "html": "<p>Newsletter body as HTML</p>"
                }
            }),
            "Missing title",
        ),
        (
            serde_json::json!({
                "title": "Newsletter title",
            }),
            "Missing content",
        ),
    ];

    // Act
    for (body, error_message) in newsletter_bodies {
        let response = app.post_newsletters(body).await;

        println!("error message: {}", error_message);
        assert_eq!(response.status().as_u16(), 400);
    }
}

#[tokio::test]
async fn publish_newsletters_without_authentication_header_ret_401() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let newsletter_body = serde_json::json!({
        "title": "Newsletter title",
        "content": {
            "text": "Newsletter body as plain text",
            "html": "<p>Newsletter body as HTML</p>"
        }
    });

    // Act
    let response = reqwest::Client::new()
        .post(&format!("{}/newsletters", app.addr))
        .json(&newsletter_body)
        .send()
        .await
        .expect("Failed to execute request");

    // Assert
    assert_eq!(response.status().as_u16(), 401);
    assert_eq!(
        response.headers()["WWW-Authenticate"],
        r#"Basic realm="publish""#
    );
}

#[tokio::test]
async fn publish_newsletters_as_valid_user_ret_200() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let newsletter_body = serde_json::json!({
        "title": "Newsletter title",
        "content": {
            "text": "Newsletter body as plain text",
            "html": "<p>Newsletter body as HTML</p>"
        }
    });

    // Act
    let response = app.post_newsletters(newsletter_body).await;

    // Assert
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn publish_newsletters_as_invalid_username_ret_401() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let newsletter_body = serde_json::json!({
        "title": "Newsletter title",
        "content": {
            "text": "Newsletter body as plain text",
            "html": "<p>Newsletter body as HTML</p>"
        }
    });

    let username = Uuid::new_v4().to_string();
    let password = Uuid::new_v4().to_string();

    // Act
    let response = reqwest::Client::new()
        .post(&format!("{}/newsletters", app.addr))
        .json(&newsletter_body)
        .basic_auth(username, Some(password))
        .send()
        .await
        .expect("Failed to execute request");

    // Assert
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn publish_newsletters_as_invalid_password_ret_401() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let newsletter_body = serde_json::json!({
        "title": "Newsletter title",
        "content": {
            "text": "Newsletter body as plain text",
            "html": "<p>Newsletter body as HTML</p>"
        }
    });

    let username = app.test_user.username;
    let password = Uuid::new_v4().to_string();

    // Act
    let response = reqwest::Client::new()
        .post(&format!("{}/newsletters", app.addr))
        .json(&newsletter_body)
        .basic_auth(username, Some(password))
        .send()
        .await
        .expect("Failed to execute request");

    // Assert
    assert_eq!(response.status().as_u16(), 401);
}

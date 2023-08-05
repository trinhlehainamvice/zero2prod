use crate::helpers::{assert_redirects_to, spawn_app};
use uuid::Uuid;

#[tokio::test]
async fn publish_newsletters_invalid_form_data_ret_400() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let body = "name=Foo%20Bar&email=foobar%40example.com";

    app.create_confirmed_subscriber(body).await;

    let login_form = serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    });

    // Act 1 login
    let response = app.post_login(login_form).await;
    assert_redirects_to(&response, "/admin/dashboard");

    let idempotency_key = Uuid::new_v4().to_string();
    let newsletter_bodies = vec![
        (
            serde_json::json!({
                "text_content": "Newsletter body as plain text",
                "html_content": "<p>Newsletter body as HTML</p>",
                "idempotency_key": &idempotency_key
            }),
            "Missing title",
        ),
        (
            serde_json::json!({
                "title": "Newsletter title",
                "idempotency_key": &idempotency_key
            }),
            "Missing content",
        ),
    ];

    // Act 2 publish newsletters
    for (body, error_message) in newsletter_bodies {
        let response = app.post_newsletters(&body).await;

        println!("error message: {}", error_message);
        assert_eq!(response.status().as_u16(), 400);
    }
}

#[tokio::test]
async fn publish_newsletters_without_login_redirects_to_login() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let idempotency_key = Uuid::new_v4().to_string();
    let newsletter_body = serde_json::json!({
        "title": "Newsletter title",
        "text_content": "Newsletter body as plain text",
        "html_content": "<p>Newsletter body as HTML</p>",
        "idempotency_key": idempotency_key
    });

    // Act
    let response = app.post_newsletters(&newsletter_body).await;
    // Assert
    assert_redirects_to(&response, "/login");
}

#[tokio::test]
async fn publish_newsletters_as_valid_user_redirects_to_newsletters_with_success_message() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let body = "name=Foo%20Bar&email=foobar%40example.com";

    app.create_confirmed_subscriber(body).await;

    // Act 1 login
    let login_form = serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    });
    let response = app.post_login(login_form).await;
    assert_redirects_to(&response, "/admin/dashboard");

    let idempotency_key = Uuid::new_v4().to_string();
    let newsletter_body = serde_json::json!({
        "title": "Newsletter title",
        "text_content": "Newsletter body as plain text",
        "html_content": "<p>Newsletter body as HTML</p>",
        "idempotency_key": idempotency_key
    });

    // Act 2 send newsletter
    let response = app.post_newsletters(&newsletter_body).await;
    assert_redirects_to(&response, "/admin/newsletters");

    // Act 3 check the success message
    let html = app.get_html("/admin/newsletters").await;
    assert!(html.contains(r#"<p><i>Published newsletter successfully!</i></p>"#));
}

#[tokio::test]
async fn publish_newsletters_as_invalid_user_redirects_to_login() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let idempotency_key = Uuid::new_v4().to_string();
    let newsletter_body = serde_json::json!({
        "title": "Newsletter title",
        "text_content": "Newsletter body as plain text",
        "html_content": "<p>Newsletter body as HTML</p>",
        "idempotency_key": idempotency_key
    });

    // Server will response "Invalid Username or Password" in both cases
    let login_form: Vec<_> = vec![
        (
            serde_json::json!({
            "username": &app.test_user.username,
            "password": "wrong_password"
            }),
            r#"<p><i>Invalid Username or Password</i></p>"#,
        ),
        (
            serde_json::json!({
            "username": "wrong_user",
            "password": &app.test_user.password
            }),
            r#"<p><i>Invalid Username or Password</i></p>"#,
        ),
    ];

    for (login_form, error_message) in login_form {
        // Act 1 login
        let response = app.post_login(login_form).await;
        println!("error message: {}", error_message);
        assert_redirects_to(&response, "/login");

        // Act 2 send newsletter
        let response = app.post_newsletters(&newsletter_body).await;
        assert_redirects_to(&response, "/login");
    }
}

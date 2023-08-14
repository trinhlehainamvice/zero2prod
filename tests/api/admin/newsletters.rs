use crate::helpers::{assert_redirects_to, create_confirmed_subscriber, TestApp};
use fake::faker::lorem::en::{Paragraph, Sentence};
use fake::Fake;
use std::time::Duration;
use uuid::Uuid;

#[tokio::test]
async fn publish_newsletters_invalid_form_data_ret_400() {
    // Arrange
    let app = TestApp::builder().build().await.unwrap();
    create_confirmed_subscriber(&app).await;

    // Act 1 login
    let response = app.login().await;
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
    let app = TestApp::builder().build().await.unwrap();
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
    let app = TestApp::builder().build().await.unwrap();
    create_confirmed_subscriber(&app).await;

    // Act 1 login
    let response = app.login().await;
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
    let app = TestApp::builder().build().await.unwrap();
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

#[tokio::test]
async fn publish_duplicate_newsletters_ret_same_response() {
    // Arrange
    let app = TestApp::builder()
        .spawn_newsletters_issues_delivery_worker()
        .build()
        .await
        .unwrap();

    create_confirmed_subscriber(&app).await;

    let newsletter_body = serde_json::json!({
        "title": "Newsletter title",
        "text_content": "Newsletter body as plain text",
        "html_content": "<p>Newsletter body as HTML</p>",
        "idempotency_key": Uuid::new_v4().to_string()
    });

    // Act 1 login
    let response = app.login().await;
    assert_redirects_to(&response, "/admin/dashboard");

    // Act 2 publish newsletter
    let response_1 = app.post_newsletters(&newsletter_body).await;
    assert_redirects_to(&response_1, "/admin/newsletters");

    // Act 3 publish newsletter **again**
    let response_2 = app.post_newsletters(&newsletter_body).await;
    assert_redirects_to(&response_2, "/admin/newsletters");

    // Assert expect to be the same
    assert_eq!(
        response_1.text().await.unwrap(),
        response_2.text().await.unwrap()
    );

    app.send_remaining_emails()
        .await
        .expect("Failed to send newsletters to subscriber emails");
}

#[tokio::test(flavor = "multi_thread")]
async fn publish_duplicate_newsletters_in_parallel_ret_same_response() {
    // Arrange
    let app = TestApp::builder()
        .spawn_newsletters_issues_delivery_worker()
        .build()
        .await
        .unwrap();
    create_confirmed_subscriber(&app).await;

    let newsletter_body = serde_json::json!({
        "title": "Newsletter title",
        "text_content": "Newsletter body as plain text",
        "html_content": "<p>Newsletter body as HTML</p>",
        "idempotency_key": Uuid::new_v4().to_string()
    });

    // Act 1 login
    let response = app.login().await;
    assert_redirects_to(&response, "/admin/dashboard");

    // Act 2 publish newsletters in parallel
    let mut responses = vec![];
    for _ in 0..(5..10).fake() {
        responses.push(app.post_newsletters(&newsletter_body));
    }

    let responses = futures::future::join_all(responses).await;
    app.send_remaining_emails()
        .await
        .expect("failed to send emails");

    let mut texts = vec![];
    for response in responses {
        texts.push(response.text().await.unwrap());
    }

    // Assert expect all response contents to be the same
    assert!(texts.windows(2).all(|text| text[0] == text[1]));
}

#[tokio::test(flavor = "multi_thread")]
async fn forward_recovery_send_emails_when_user_post_newsletter() {
    // TODO: mock email server now is in docker
    // so it's really hard to simulate error or processing requests in sequence
    // may need to find better way
}

#[tokio::test(flavor = "multi_thread")]
async fn publish_multiple_newsletters() {
    // Arrange
    let app = TestApp::builder()
        .spawn_newsletters_issues_delivery_worker()
        .build()
        .await
        .unwrap();
    app.login().await;

    let n_subscribers: u64 = (5..10).fake();
    for _ in 0..n_subscribers {
        create_confirmed_subscriber(&app).await;
    }

    let n_publish: u64 = (5..10).fake();

    // Because each test case is executed once at a time and in order
    // So we can believe that the number of messages are not affected by another test
    // We can cache the number of messages in mock email server before publish newsletters
    let msg_count_before_publish = app
        .get_email_messages_json()
        .await
        .as_array()
        .unwrap()
        .len();

    for _ in 0..n_publish {
        let title: String = Sentence(10..20).fake();
        let text: String = Paragraph(50..100).fake();
        let html: String = format!("<p>{}</p>", &text);
        let newsletter_body = serde_json::json!({
            "title": title,
            "text_content": text,
            "html_content": html,
            "idempotency_key": Uuid::new_v4().to_string()
        });
        let response = app.post_newsletters(&newsletter_body).await;
        assert_redirects_to(&response, "/admin/newsletters");
    }

    tokio::time::timeout(
        Duration::from_secs(1),
        app.wait_until_email_messages_match(msg_count_before_publish, n_publish as usize),
    )
    .await
    .expect("Failed to wait until email server receive expected number of requests");

    let current_msg_count = app
        .get_email_messages_json()
        .await
        .as_array()
        .unwrap()
        .len();

    assert_eq!(
        current_msg_count - msg_count_before_publish,
        n_publish as usize
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn idempotency_expired_and_republish_newsletter() {
    // Arrange
    let app = TestApp::builder()
        .spawn_newsletters_issues_delivery_worker()
        .spawn_delete_expired_idempotency_worker()
        .idempotency_expiration_time_millis(10)
        .build()
        .await
        .unwrap();

    app.login().await;

    create_confirmed_subscriber(&app).await;
    create_confirmed_subscriber(&app).await;

    let newsletter_body = serde_json::json!({
        "title": "Newsletter title",
        "text_content": "Newsletter body as plain text",
        "html_content": "<p>Newsletter body as HTML</p>",
        "idempotency_key": Uuid::new_v4().to_string()
    });

    let msg_count_before_publish = app
        .get_email_messages_json()
        .await
        .as_array()
        .unwrap()
        .len();

    // Act 1 publish newsletters
    let response = app.post_newsletters(&newsletter_body).await;
    assert_redirects_to(&response, "/admin/newsletters");

    // Act 2 wait until idempotency is expired, then check idempotency key is deleted in database
    tokio::time::sleep(Duration::from_millis(100)).await;

    let result = sqlx::query!(
        r#"
        SELECT user_id FROM idempotency WHERE idempotency_key = $1
        "#,
        newsletter_body
            .get("idempotency_key")
            .unwrap()
            .as_str()
            .unwrap()
    )
    .fetch_optional(&app.pg_pool)
    .await
    .expect("Failed to fetch idempotency");

    assert!(result.is_none());

    let response = app.post_newsletters(&newsletter_body).await;
    assert_redirects_to(&response, "/admin/newsletters");

    tokio::time::timeout(
        Duration::from_secs(1),
        app.wait_until_email_messages_match(msg_count_before_publish, 4),
    )
    .await
    .expect("Failed to wait until email server receive expected number of requests");

    let current_msg_count = app
        .get_email_messages_json()
        .await
        .as_array()
        .unwrap()
        .len();

    assert_eq!(current_msg_count - msg_count_before_publish, 4);
}

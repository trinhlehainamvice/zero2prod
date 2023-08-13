use crate::helpers::{assert_redirects_to, create_confirmed_subscriber, TestApp};
use fake::faker::lorem::en::{Paragraph, Sentence};
use fake::Fake;
use std::time::Duration;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::ResponseTemplate;

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

    wiremock::Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

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

    wiremock::Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

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

    // Assert expect all responses' contents to be the same
    assert!(texts.windows(2).all(|text| text[0] == text[1]));
}

#[tokio::test(flavor = "multi_thread")]
async fn forward_recovery_send_emails_when_user_post_newsletter() {
    // Arrange
    let app = TestApp::builder()
        .spawn_newsletters_issues_delivery_worker()
        .build()
        .await
        .unwrap();

    // Act 1 login
    app.login().await;

    create_confirmed_subscriber(&app).await;
    create_confirmed_subscriber(&app).await;

    // Notify newsletters successfully send to first subscriber
    wiremock::Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        // Mock server receives 1 request and drop
        .up_to_n_times(1)
        .expect(1)
        .mount(&app.email_server)
        .await;
    // Then fail to send newsletters to second subscriber
    wiremock::Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        // Mock server receives 1 request and drop
        .up_to_n_times(1)
        .expect(1)
        .mount(&app.email_server)
        .await;

    let newsletter_body = serde_json::json!({
        "title": "Newsletter title",
        "text_content": "Newsletter body as plain text",
        "html_content": "<p>Newsletter body as HTML</p>",
        "idempotency_key": Uuid::new_v4().to_string()
    });

    // Act 2 publish newsletters expect to success when trigger issue
    // Even if server failed
    let response = app.post_newsletters(&newsletter_body).await;
    assert_redirects_to(&response, "/admin/newsletters");

    // Act 3 retry publish newsletters expect to success to send newsletter to only one subscriber's email
    let mock = wiremock::Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount_as_scoped(&app.email_server)
        .await;

    let response = app.post_newsletters(&newsletter_body).await;
    assert_redirects_to(&response, "/admin/newsletters");

    // Newsletters Issue Delivery Worker will wait about 1 secs when failed to dequeue issue task and send email
    // Need to wait more than 1 secs to make sure Worker is back to process
    let _ = tokio::time::timeout(Duration::from_secs(2), mock.wait_until_satisfied()).await;
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

    let n_expected_requests = n_publish * n_subscribers;

    let mock = wiremock::Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(n_expected_requests)
        .mount_as_scoped(&app.email_server)
        .await;

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

    let _ = tokio::time::timeout(Duration::from_secs(1), mock.wait_until_satisfied()).await;
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

    wiremock::Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        // Limit number of requests this Matcher can handle
        // Then free to another Matcher to handle
        .up_to_n_times(2)
        .expect(2)
        .mount(&app.email_server)
        .await;

    let newsletter_body = serde_json::json!({
        "title": "Newsletter title",
        "text_content": "Newsletter body as plain text",
        "html_content": "<p>Newsletter body as HTML</p>",
        "idempotency_key": Uuid::new_v4().to_string()
    });

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

    // Act 3 republish newsletters after idempotency is expired and deleted
    let mock_guard = wiremock::Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount_as_scoped(&app.email_server)
        .await;

    let response = app.post_newsletters(&newsletter_body).await;
    assert_redirects_to(&response, "/admin/newsletters");

    let _ = tokio::time::timeout(
        Duration::from_millis(200),
        mock_guard.wait_until_satisfied(),
    )
    .await;
}

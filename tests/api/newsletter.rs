use crate::helpers::spawn_app;

#[tokio::test]
async fn newsletter_invalid_form_data_ret_400() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let body = "name=Foo%20Bar&email=foobar%40example.com";

    app.create_confirmed_subscriber(body).await;

    let newsletter_body = vec![
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
    for (body, error_message) in newsletter_body {
        let response = app.post_newsletters(body).await;

        println!("error message: {}", error_message);
        assert_eq!(response.status().as_u16(), 400);
    }
}

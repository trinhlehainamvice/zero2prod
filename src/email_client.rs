use crate::routes::SubscriberEmail;
use secrecy::{ExposeSecret, Secret};

pub struct EmailClient {
    http_client: reqwest::Client,
    api_base_url: String,
    sender_email: SubscriberEmail,
    auth_header: Secret<String>,
    auth_token: Secret<String>,
}

impl EmailClient {
    pub fn new(
        api_base_url: String,
        sender_email: SubscriberEmail,
        // TODO: API authentication not only depend on header
        auth_header: Secret<String>,
        auth_token: Secret<String>,
        request_timeout_millis: u64,
    ) -> Self {
        Self {
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(request_timeout_millis))
                .build()
                .unwrap(),
            api_base_url,
            sender_email,
            auth_header,
            auth_token,
        }
    }

    pub async fn send_email(
        &self,
        recipient_email: &SubscriberEmail,
        subject: &str,
        text_body: &str,
        html_body: &str,
    ) -> Result<(), reqwest::Error> {
        let url = reqwest::Url::parse(&self.api_base_url)
            .expect("Invalid api base url")
            .join("email")
            .expect("Invalid api endpoint");

        let request_body = SendEmailRequest {
            from: self.sender_email.as_ref(),
            to: recipient_email.as_ref(),
            subject,
            text_body,
            html_body,
        };

        // TODO: depend on how API server requires authentication to setup
        self.http_client
            .post(url.as_str())
            .header(
                self.auth_header.expose_secret(),
                self.auth_token.expose_secret(),
            )
            .json(&request_body)
            .send()
            .await?;

        Ok(())
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct SendEmailRequest<'a> {
    from: &'a str,
    to: &'a str,
    subject: &'a str,
    text_body: &'a str,
    html_body: &'a str,
}

#[cfg(test)]
mod tests {
    use crate::email_client::EmailClient;
    use crate::routes::SubscriberEmail;
    use fake::faker::internet::en::SafeEmail;
    use fake::faker::lorem::en::{Paragraph, Sentence};
    use fake::{Fake, Faker};
    use secrecy::Secret;
    use std::time::Duration;
    use wiremock::matchers::{any, header, header_exists, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    struct SendPostmarkEmailRequestBodyMatcher;

    impl wiremock::Match for SendPostmarkEmailRequestBodyMatcher {
        fn matches(&self, request: &Request) -> bool {
            match serde_json::from_slice::<serde_json::Value>(&request.body) {
                Ok(body) => {
                    body.get("From").is_some()
                        && body.get("To").is_some()
                        && body.get("Subject").is_some()
                        && body.get("TextBody").is_some()
                        && body.get("HtmlBody").is_some()
                }
                _ => false,
            }
        }
    }

    fn subject() -> String {
        Sentence(1..2).fake()
    }

    fn content() -> String {
        Paragraph(1..10).fake()
    }

    fn subscriber_email() -> SubscriberEmail {
        SubscriberEmail::parse(SafeEmail().fake()).unwrap()
    }

    fn sender_email() -> SubscriberEmail {
        SubscriberEmail::parse(SafeEmail().fake()).unwrap()
    }

    fn auth_header() -> Secret<String> {
        Secret::new("X-Mail-Server-Token".to_string())
    }

    fn auth_token() -> Secret<String> {
        Secret::new(Faker.fake())
    }

    fn timeout_millis() -> u64 {
        100
    }

    #[tokio::test]
    async fn match_postmark_send_email_request() {
        // Arrange
        // Make a mock http server on random port on local machine
        let mock_server = MockServer::start().await;
        // Retrieve mock server url with `mock_server.uri()`
        let email_client = EmailClient::new(
            mock_server.uri(),
            sender_email(),
            auth_header(),
            auth_token(),
            timeout_millis(),
        );

        // Set up expected request requirements for the mock server inside `Mock::given`
        Mock::given(header_exists("X-Mail-Server-Token"))
            .and(header("Content-Type", "application/json"))
            .and(path("/email"))
            .and(method("POST"))
            .and(SendPostmarkEmailRequestBodyMatcher)
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Act
        let _ = email_client
            .send_email(&subscriber_email(), &subject(), &content(), &content())
            .await;
    }

    #[tokio::test]
    async fn send_expected_email_client_request_fail_return_400_timeout() {
        // Arrange
        // Make a mock http server on random port on local machine
        let mock_server = MockServer::start().await;
        // Retrieve mock server url with `mock_server.uri()`
        let email_client = EmailClient::new(
            mock_server.uri(),
            sender_email(),
            auth_header(),
            auth_token(),
            timeout_millis(),
        );

        // Set up expected request requirements for the mock server inside `Mock::given`
        Mock::given(any())
            .respond_with(ResponseTemplate::new(400).set_delay(Duration::from_millis(110)))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Act
        let _ = email_client
            .send_email(&subscriber_email(), &subject(), &content(), &content())
            .await;
    }
}

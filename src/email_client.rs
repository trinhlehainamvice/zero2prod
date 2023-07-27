use crate::routes::SubscriberEmail;
use secrecy::{ExposeSecret, Secret};

pub struct EmailClient {
    http_client: reqwest::Client,
    api_base_url: String,
    sender_email: SubscriberEmail,
    auth_token: Secret<String>,
}

impl EmailClient {
    pub fn new(
        api_base_url: String,
        sender_email: SubscriberEmail,
        auth_token: Secret<String>,
    ) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            api_base_url,
            sender_email,
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

        self.http_client
            .post(url.as_str())
            .header("X-Postmark-Server-Token", self.auth_token.expose_secret())
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
    use wiremock::matchers::{header, header_exists, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    struct SendEmailRequestBodyMatcher;

    impl wiremock::Match for SendEmailRequestBodyMatcher {
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

    #[tokio::test]
    async fn send_expected_email_client_request() {
        // Arrange
        // Make a mock http server on random port on local machine
        let mock_server = MockServer::start().await;
        let sender_email = SubscriberEmail::parse(SafeEmail().fake()).unwrap();
        let auth_token = Secret::new(Faker.fake());
        // Retrieve mock server url with `mock_server.uri()`
        let email_client = EmailClient::new(mock_server.uri(), sender_email, auth_token);

        // Set up expected request requirements for the mock server inside `Mock::given`
        Mock::given(header_exists("X-Postmark-Server-Token"))
            .and(header("Content-Type", "application/json"))
            .and(path("/email"))
            .and(method("POST"))
            .and(SendEmailRequestBodyMatcher)
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        let subscriber_email = SubscriberEmail::parse(SafeEmail().fake()).unwrap();
        let subject: String = Sentence(1..2).fake();
        let content: String = Paragraph(1..10).fake();

        // Act
        let _ = email_client
            .send_email(&subscriber_email, &subject, &content, &content)
            .await;
    }
}

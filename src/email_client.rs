use crate::routes::SubscriberEmail;
use anyhow::Context;
use lettre::transport::smtp;
use lettre::{message, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use secrecy::{ExposeSecret, Secret};
use std::time::Duration;

// This api app use Email service provider to send email
// So this app is a client of Email service
pub struct EmailClient {
    smtp_transport: AsyncSmtpTransport<Tokio1Executor>,
    sender_email: SubscriberEmail,
}

impl EmailClient {
    pub fn new(
        host: String,
        sender_email: SubscriberEmail,
        username: Option<Secret<String>>,
        password: Option<Secret<String>>,
        port: Option<u16>,
        require_tls: bool,
        request_timeout_millis: u64,
    ) -> Result<Self, anyhow::Error> {
        let mut smtp_transport = match require_tls {
            true => AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
                .context("Failed to create smtp transport")?,
            false => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host),
        };

        if let (Some(username), Some(password)) = (username, password) {
            let credentials = smtp::authentication::Credentials::new(
                username.expose_secret().to_string(),
                password.expose_secret().to_string(),
            );
            smtp_transport = smtp_transport.credentials(credentials);
        }

        if let Some(port) = port {
            smtp_transport = smtp_transport.port(port);
        }

        let smtp_transport = smtp_transport
            .timeout(Some(Duration::from_millis(request_timeout_millis)))
            .build();

        Ok(Self {
            smtp_transport,
            sender_email,
        })
    }

    pub fn sender_email(&self) -> &str {
        self.sender_email.as_ref()
    }
    
    pub async fn send_multipart_email(
        &self,
        recipient_email: &SubscriberEmail,
        subject: impl Into<String>,
        text_content: impl Into<String>,
        html_content: impl Into<String>,
    ) -> Result<smtp::response::Response, anyhow::Error> {
        let message = Message::builder()
            .from(
                format!("{} <{}>", "Zero2Prod", self.sender_email.as_ref())
                    .parse()
                    .unwrap(),
            )
            .to(format!("<{}>", recipient_email.as_ref()).parse().unwrap())
            .subject(subject)
            .multipart(
                message::MultiPart::alternative()
                    .singlepart(
                        message::SinglePart::builder()
                            .header(message::header::ContentType::TEXT_PLAIN)
                            .body(text_content.into()),
                    )
                    .singlepart(
                        message::SinglePart::builder()
                            .header(message::header::ContentType::TEXT_HTML)
                            .body(html_content.into()),
                    ),
            )
            .context("Failed to create email message")?;

        self.smtp_transport
            .send(message)
            .await
            .context("Failed to send message to email service")
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
    use fake::Fake;

    fn subject() -> String {
        Sentence(1..2).fake()
    }

    fn plain_text() -> String {
        Paragraph(1..10).fake()
    }

    fn html_text() -> String {
        format!("<p>{}</p>", Paragraph(1..10).fake::<String>())
    }

    fn subscriber_email() -> SubscriberEmail {
        SubscriberEmail::parse(SafeEmail().fake()).unwrap()
    }

    fn sender_email() -> SubscriberEmail {
        SubscriberEmail::parse(SafeEmail().fake()).unwrap()
    }

    fn timeout_millis() -> u64 {
        100
    }

    // NOTE: these tests depending on mailcrab to host mock smtp server
    // make sure to launch mailcrab on local machine or docker before running the tests
    // REF: https://github.com/tweedegolf/mailcrab
    #[tokio::test]
    async fn send_email() {
        let email_client = EmailClient::new(
            "localhost".to_string(),
            sender_email(),
            None,
            None,
            Some(1025),
            false,
            timeout_millis(),
        )
        .expect("Failed to create email client");

        let subject = subject();
        let plain_text = plain_text();
        let html_text = html_text();
        let recipient_email = subscriber_email();

        let response = email_client
            .send_multipart_email(&recipient_email, &subject, &plain_text, &html_text)
            .await
            .expect(
                "Failed to send email to smtp server \
            This test depending on mailcrab as local smtp server\
            Launch mailcrab before running this test again",
            );

        let messages: Vec<_> = response.message().collect();
        assert_eq!(messages.len(), 1);
        let message = messages.first().unwrap();
        assert!(message.contains("2.0.0 Ok: queued as "));
        let message_id = message.strip_prefix("2.0.0 Ok: queued as ").unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://localhost:1080/api/message/{}", message_id))
            .send()
            .await
            .expect("Failed to get messages from mailcrab");
        assert_eq!(response.status().as_u16(), 200);

        let body: serde_json::Value = response
            .json()
            .await
            .expect("Failed to get messages from mailcrab");

        assert_eq!(body["subject"], subject);
        assert_eq!(body["to"][0]["email"], recipient_email.as_ref());
    }
}

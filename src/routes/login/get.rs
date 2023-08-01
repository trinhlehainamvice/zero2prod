use crate::authentication::HmacSecret;
use actix_web::http::header::ContentType;
use actix_web::{web, HttpResponse};
use hmac::{Hmac, Mac};
use secrecy::ExposeSecret;
use sha2::Sha256;

#[derive(serde::Deserialize)]
pub struct ErrorParam {
    // Make a param optional, mean query still passed even if this param is missing
    error: Option<String>,
    // hmac to qualify the valid error param
    tag: Option<String>,
}

impl ErrorParam {
    fn verify(self, hmac_secret: &HmacSecret) -> Result<String, anyhow::Error> {
        let tag = self
            .tag
            .ok_or_else(|| anyhow::anyhow!("Missing tag query param"))?;
        // Decode hex string to array of bytes
        // Example: abcd1e2f -> [10, 11, 12, 13, 1, 14, 15]
        let tag = hex::decode(tag)?;

        let error = self
            .error
            .ok_or_else(|| anyhow::anyhow!("Missing error query param"))?;

        let query_string = format!("error={}", error);
        // Create mac hash algorithm with hmac_secret seed
        let mut mac =
            Hmac::<Sha256>::new_from_slice(hmac_secret.0.expose_secret().as_bytes()).unwrap();
        // Hash query_string
        mac.update(query_string.as_bytes());
        // Compare hashed_query_string with tag
        mac.verify_slice(&tag)?;

        // Remove untrusted characters that can be used in HTML content and executed in browser
        // Known as XSS (Cross Site Scripting)
        // For example: https://www.example.com/search?q=<script>alert('XSS Attack!')</script>
        // Just like SQL injection, attacker injects untrusted characters into the query that lead to unexpected query execution
        Ok(htmlescape::encode_minimal(&error))
    }
}

pub async fn login_form(
    web::Query(params): web::Query<ErrorParam>,
    hmac_secret: web::Data<HmacSecret>,
) -> HttpResponse {
    let error_html = match params.verify(&hmac_secret) {
        Ok(error_html) => error_html,
        Err(error) => {
            tracing::warn!(
                error.message = %error,
                error.cause_chain = ?error,
                "Failed to verify query parameters"
            );
            "".to_string()
        }
    };

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(
            r#"
               <!DOCTYPE html>
<html lang="en">
<head>
    <meta http-equiv="content-type" content="text/html; charset=utf-8">
    <title>Login</title>
</head>
<body>
<form action="/login" method="POST">
    {error_html}
    <label>Username
        <input
                type="text"
                placeholder="Enter username"
                name="username"
        >
    </label>
    <label>Password
        <input
                type="password"
                placeholder="Enter password"
                name="password"
        >
    </label>
    <button type="submit">Login</button>
</form>
</body>
</html>"#
        ))
}

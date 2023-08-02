use crate::authentication::HmacSecret;
use actix_web::cookie::Cookie;
use actix_web::http::header::ContentType;
use actix_web::{web, HttpRequest, HttpResponse};
use hmac::{Hmac, Mac};
use secrecy::ExposeSecret;
use sha2::Sha256;

#[derive(serde::Deserialize)]
pub struct ErrorParams {
    // Make a param optional, mean query still passed even if this param is missing
    error: String,
    // hmac to qualify the valid error param
    tag: String,
}

impl ErrorParams {
    fn verify(self, hmac_secret: &HmacSecret) -> Result<String, anyhow::Error> {
        tracing::info!(
            error = %self.error.clone(),
            tag = %self.tag.clone(),
        );

        // Decode hex string to array of bytes
        // Example: abcd1e2f -> [10, 11, 12, 13, 1, 14, 15]
        let tag = hex::decode(self.tag)?;

        let query_string = format!("error={}", self.error);
        // Create mac hash algorithm with hmac_secret seed (aka private key)
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
        Ok(htmlescape::encode_minimal(&self.error))
    }
}

pub async fn login_form(hmac_secret: web::Data<HmacSecret>, req: HttpRequest) -> HttpResponse {
    let params: Option<ErrorParams> = match req.cookie("_flash") {
        None => None,
        Some(cookie) => match serde_json::from_str(cookie.value()) {
            Err(_) => None,
            Ok(params) => Some(params),
        },
    };

    let error_html = match params {
        None => "".to_string(),
        Some(params) => match params.verify(&hmac_secret) {
            Ok(error) => format!("<p><i>{}</i></p>", error),
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    error.cause_chain = ?error,
                    "Failed to verify error query parameters"
                );
                "".to_string()
            }
        },
    };

    let mut response = HttpResponse::Ok()
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
        ));

    response
        .add_removal_cookie(&Cookie::new("_flash", ""))
        .unwrap();

    response
}

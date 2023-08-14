use secrecy::{ExposeSecret, Secret};
use serde_aux::prelude::{deserialize_number_from_string, deserialize_option_number_from_string};
use sqlx::postgres::{PgConnectOptions, PgSslMode};

const APP_ENV_STATE: &str = "APP_ENV_STATE";
const LOCAL: &str = "local";
const PRODUCTION: &str = "production";

#[derive(serde::Deserialize, Clone)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub database: DatabaseSettings,
    pub email_client: EmailClientSettings,
}

impl Settings {
    pub fn get_configuration() -> Result<Settings, config::ConfigError> {
        let base_path = std::env::current_dir().expect("Failed to determine the current directory");
        let config_dir = base_path.join("configuration");

        let app_env_state: Environment = std::env::var(APP_ENV_STATE)
            .unwrap_or_else(|_| LOCAL.to_string())
            .try_into()
            // .expect(&format!("Failed to parse {}", APP_ENV_STATE));
            // `clippy` suggest to use `unwrap_or_else` instead of `expect` when use a function call
            // function in `expect` is always called even `expect` itself is not called
            .unwrap_or_else(|_| panic!("Failed to parse {}", APP_ENV_STATE));

        // TEMPLATE: APP_<Settings.data>__<data.var>
        // e.g. APP_DATABASE__DATABASE_NAME
        let config_env = config::Environment::default()
            .prefix("app")
            .prefix_separator("_")
            .separator("__");

        // Read the configuration from the file
        // supported file extensions: json, toml, yaml, etc
        config::Config::builder()
            .add_source(config::File::from(config_dir.clone().join("share")))
            // ConfigBuilder will merge multiple sources to one when build
            .add_source(config::File::from(config_dir.join(app_env_state.as_str())))
            .add_source(config_env)
            .build()?
            // Deserialize the configuration into a Settings struct
            .try_deserialize()
    }
}

#[derive(serde::Deserialize, Clone)]
pub struct ApplicationSettings {
    pub name: String,
    pub rust_log: String,
    pub host: String,
    pub base_url: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    pub flash_msg_key: Secret<String>,
    pub redis_url: Secret<String>,
    pub redis_session_key: Secret<String>,
    pub idempotency_expiration_millis: u64,
}

impl ApplicationSettings {
    pub fn get_url(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(serde::Deserialize, Clone)]
pub struct EmailClientSettings {
    pub username: Option<Secret<String>>,
    pub password: Option<Secret<String>>,
    pub host: String,
    #[serde(default, deserialize_with = "deserialize_option_number_from_string")]
    pub port: Option<u16>,
    pub sender_email: String,
    pub require_tls: bool,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub request_timeout_millis: u64,
}

#[derive(serde::Deserialize, Clone)]
pub struct DatabaseSettings {
    pub engine: String,
    pub username: String,
    pub password: Secret<String>,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    pub host: String,
    pub database_name: String,
    pub require_ssl: bool,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub query_timeout_secs: u64,
}

impl DatabaseSettings {
    pub fn get_pg_database_options(&self) -> PgConnectOptions {
        PgConnectOptions::new()
            .host(&self.host)
            .username(&self.username)
            .password(self.password.expose_secret())
            .database(&self.database_name)
            .port(self.port)
            .ssl_mode(self.get_ssl_mode())
    }

    /// Without specify database
    pub fn get_pg_options(&self) -> PgConnectOptions {
        PgConnectOptions::new()
            .host(&self.host)
            .username(&self.username)
            .password(self.password.expose_secret())
            .port(self.port)
            .ssl_mode(self.get_ssl_mode())
    }

    pub fn get_ssl_mode(&self) -> PgSslMode {
        match self.require_ssl {
            true => PgSslMode::Require,
            _ => PgSslMode::Prefer,
        }
    }
}

enum Environment {
    Local,
    Production,
}

impl Environment {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Environment::Local => LOCAL,
            Environment::Production => PRODUCTION,
        }
    }
}

impl TryFrom<String> for Environment {
    type Error = String;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.as_str() {
            LOCAL => Ok(Self::Local),
            PRODUCTION => Ok(Self::Production),
            other => Err(format!("Invalid {}: {}", APP_ENV_STATE, other)),
        }
    }
}

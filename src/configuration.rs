use secrecy::{ExposeSecret, Secret};
use serde_aux::prelude::deserialize_number_from_string;
use sqlx::postgres::{PgConnectOptions, PgSslMode};

#[derive(serde::Deserialize)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub database: DatabaseSettings,
}

impl Settings {
    pub fn get_configuration() -> Result<Settings, config::ConfigError> {
        let base_path = std::env::current_dir().expect("Failed to determine the current directory");
        let config_dir = base_path.join("configuration");

        let app_env_state: Environment = std::env::var("APP_ENV_STATE")
            .unwrap_or(Environment::Local.as_str().into())
            .try_into()
            .expect("Failed to parse APP_ENV_STATE");

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

#[derive(serde::Deserialize)]
pub struct ApplicationSettings {
    pub name: String,
    pub default_log_level: String,
    pub host: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
}

impl ApplicationSettings {
    pub fn get_url(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(serde::Deserialize)]
pub struct DatabaseSettings {
    pub engine: String,
    pub username: String,
    pub password: Secret<String>,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    pub host: String,
    pub database_name: String,
    pub require_ssl: bool,
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
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Production => "production",
        }
    }
}

impl TryFrom<String> for Environment {
    type Error = String;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.as_str() {
            "local" => Ok(Self::Local),
            "production" => Ok(Self::Production),
            other => Err(format!("Invalid APP_ENVIRONMENT: {}", other)),
        }
    }
}

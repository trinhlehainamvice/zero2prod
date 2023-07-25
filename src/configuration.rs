use secrecy::{ExposeSecret, Secret};

#[derive(serde::Deserialize)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub database: DatabaseSettings,
}

impl Settings {
    pub fn get_configuration() -> Result<Settings, config::ConfigError> {
        let base_path = std::env::current_dir().expect("Failed to determine the current directory");
        let config_dir = base_path.join("configuration");

        let env: Environment = std::env::var("APP_ENVIRONMENT")
            .unwrap_or(Environment::Local.as_str().into())
            .try_into()
            .expect("Failed to parse APP_ENVIRONMENT");

        // Read the configuration from the file
        // supported file extensions: json, toml, yaml, etc
        config::Config::builder()
            .add_source(config::File::from(config_dir.clone().join("share")))
            // ConfigBuilder will merge multiple sources to one when build
            .add_source(config::File::from(config_dir.join(env.as_str())))
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
    pub port: u16,
}

impl ApplicationSettings {
    pub fn get_url(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(serde::Deserialize)]
pub struct DatabaseSettings {
    pub protocol: String,
    pub username: String,
    pub password: Secret<String>,
    pub port: u16,
    pub host: String,
    pub database_name: String,
}

impl DatabaseSettings {
    pub fn get_database_url(&self) -> String {
        format!(
            "{}://{}:{}@{}:{}/{}",
            self.protocol,
            self.username,
            self.password.expose_secret(),
            self.host,
            self.port,
            self.database_name
        )
    }

    pub fn get_base_url(&self) -> String {
        format!(
            "{}://{}:{}@{}:{}",
            self.protocol,
            self.username,
            self.password.expose_secret(),
            self.host,
            self.port
        )
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

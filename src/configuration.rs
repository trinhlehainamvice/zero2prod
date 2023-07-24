#[derive(serde::Deserialize)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub database: DatabaseSettings,
}

impl Settings {
    pub fn get_configuration() -> Result<Settings, config::ConfigError> {
        // Read the configuration from the file
        // supported file extensions: json, toml, yaml, etc
        config::Config::builder()
            .add_source(config::File::with_name("configuration"))
            .build()?
            // Deserialize the configuration into a Settings struct
            .try_deserialize()
    }
}

#[derive(serde::Deserialize)]
pub struct ApplicationSettings {
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
    pub password: String,
    pub port: u16,
    pub host: String,
    pub database_name: String,
}

impl DatabaseSettings {
    pub fn get_database_url(&self) -> String {
        format!(
            "{}://{}:{}@{}:{}/{}",
            self.protocol, self.username, self.password, self.host, self.port, self.database_name
        )
    }

    pub fn get_base_url(&self) -> String {
        format!(
            "{}://{}:{}@{}:{}",
            self.protocol, self.username, self.password, self.host, self.port
        )
    }
}

#[derive(Debug)]
pub struct IdempotencyKey(String);

impl TryFrom<String> for IdempotencyKey {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            anyhow::bail!("idempotency key cannot be empty")
        }
        const MAX_LENGTH: usize = 64;
        if value.len() >= MAX_LENGTH {
            anyhow::bail!(
                "idempotency key cannot be longer than {} characters",
                MAX_LENGTH
            )
        }
        Ok(Self(value))
    }
}

impl From<IdempotencyKey> for String {
    fn from(key: IdempotencyKey) -> Self {
        key.0
    }
}

impl AsRef<str> for IdempotencyKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

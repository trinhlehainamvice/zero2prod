use validator::validate_email;

pub struct SubscriberEmail(String);

impl SubscriberEmail {
    pub fn parse(email: String) -> Result<Self, String> {
        match validate_email(&email) {
            true => Ok(Self(email)),
            false => Err("Invalid email address".into()),
        }
    }
}

impl AsRef<str> for SubscriberEmail {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsMut<str> for SubscriberEmail {
    fn as_mut(&mut self) -> &mut str {
        &mut self.0
    }
}

impl TryInto<String> for SubscriberEmail {
    type Error = String;
    fn try_into(self) -> Result<String, Self::Error> {
        Ok(self.0)
    }
}

#[cfg(test)]
mod tests {
    use crate::routes::SubscriberEmail;
    use fake::faker::internet::en::SafeEmail;
    use fake::Fake;
    use quickcheck::Gen;

    #[derive(Debug, Clone)]
    struct ValidEmailFixture(pub String);

    impl quickcheck::Arbitrary for ValidEmailFixture {
        fn arbitrary<G: Gen>(g: &mut G) -> Self {
            let email = SafeEmail().fake_with_rng(g);
            Self(email)
        }
    }

    #[quickcheck_macros::quickcheck]
    fn valid_email_are_accepted(email: ValidEmailFixture) -> bool {
        SubscriberEmail::parse(email.0).is_ok()
    }
}

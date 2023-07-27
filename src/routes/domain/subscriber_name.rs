use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug)]
pub struct SubscriberName(String);

impl SubscriberName {
    pub fn parse(name: String) -> Result<Self, String> {
        if name.trim().is_empty() {
            return Err("SubscriberName cannot be empty".into());
        }

        if !(3..=30).contains(&name.graphemes(true).count()) {
            return Err("SubscriberName must be between 3 and 30 characters".into());
        }

        const FORBIDDEN_CHARACTERS: [char; 9] = ['/', '(', ')', '"', '<', '>', '\\', '{', '}'];
        if name.chars().any(|c| FORBIDDEN_CHARACTERS.contains(&c)) {
            return Err("SubscriberName contain forbidden characters".into());
        }

        Ok(Self(name))
    }
}

impl AsRef<str> for SubscriberName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsMut<str> for SubscriberName {
    fn as_mut(&mut self) -> &mut str {
        &mut self.0
    }
}

impl TryInto<String> for SubscriberName {
    type Error = String;
    fn try_into(self) -> Result<String, Self::Error> {
        Ok(self.0)
    }
}

#[cfg(test)]
mod tests {
    use crate::routes::SubscriberName;
    use claims::{assert_err, assert_ok};

    #[test]
    fn name_in_range_of_3_to_30_grapheme() {
        let name = "a".repeat(20);
        assert_ok!(SubscriberName::parse(name));
    }

    #[test]
    fn name_is_not_in_range_of_3_to_30_grapheme() {
        let name = "a".repeat(31);
        assert_err!(SubscriberName::parse(name));
        let name = "a".repeat(2);
        assert_err!(SubscriberName::parse(name));
    }

    #[test]
    fn empty_name_is_rejected() {
        let empty_name = "".to_string();
        assert_err!(SubscriberName::parse(empty_name));
        let only_whitespace = " ".to_string();
        assert_err!(SubscriberName::parse(only_whitespace));
    }

    #[test]
    fn names_containing_an_invalid_character_are_rejected() {
        let name = "a".repeat(3);
        for invalid_char in &['/', '(', ')', '"', '<', '>', '\\', '{', '}'] {
            let mut name = name.clone();
            name.push_str(&invalid_char.to_string());
            assert_err!(SubscriberName::parse(name));
        }
    }

    #[test]
    fn a_valid_name_is_parsed_successfully() {
        let name = "Ursula Le Guin".to_string();
        assert_ok!(SubscriberName::parse(name));
    }
}

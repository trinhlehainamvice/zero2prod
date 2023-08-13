use crate::routes::{SubscriberEmail, SubscriberName};

pub struct NewSubscriber {
    pub name: SubscriberName,
    pub email: SubscriberEmail,
}

#[derive(strum::AsRefStr)]
pub enum SubscriptionStatus {
    #[strum(serialize = "pending")]
    Pending,
    #[strum(serialize = "confirmed")]
    Confirmed,
}

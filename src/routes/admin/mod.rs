mod dashboard;
mod change_password;
mod logout;

pub use dashboard::*;
pub use change_password::*;
pub use logout::*;

// TODO: add seed user (first user when deploy app)
// TODO: add button go to change_password in dashboard html
// TODO: add get change_password (get html form to change password that has back button to dashboard)
// TODO: add post change_password (handle received change password form and perform redirect)

use crate::helpers::{assert_redirects_to, spawn_app};

#[tokio::test]
async fn wrong_current_password() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let login_form = serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    });

    // Act 1 login
    let response = app.post_login(login_form).await;
    assert_redirects_to(&response, "/admin/dashboard");

    // Act 2 apply wrong current password to change password form
    let change_pwd_form = serde_json::json!({
        "current_password": "wrong_password",
        "new_password": &app.test_user.password,
        "confirm_password": &app.test_user.password
    });
    // Receive a flash message cookie about error message
    let response = app.post_form("/admin/password", change_pwd_form).await;
    assert_redirects_to(&response, "/admin/password");

    // Server use that flash message to render the page
    // And then command client to remove that flash message cookie
    let html = app.get_html("/admin/password").await;
    assert!(html.contains(r#"<p><i>Wrong current password</i></p>"#));
}

#[tokio::test]
async fn password_mismatch() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let login_form = serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    });

    // Act 1 login
    let response = app.post_login(login_form).await;
    assert_redirects_to(&response, "/admin/dashboard");

    let change_pwd_forms = vec![
        serde_json::json!({
        "current_password": &app.test_user.password,
        "new_password": &app.test_user.password,
        "confirm_password": "mismatch"
        }),
        serde_json::json!({
        "current_password": &app.test_user.password,
        "new_password": "mismatch",
        "confirm_password": &app.test_user.password
        }),
        serde_json::json!({
        "current_password": "mismatch",
        "new_password": "mismatch",
        "confirm_password": &app.test_user.password
        }),
    ];

    for change_pwd_form in change_pwd_forms {
        // Act 2 apply mismatched new passwords to change password form
        let response = app.post_form("/admin/password", change_pwd_form).await;
        assert_redirects_to(&response, "/admin/password");

        let html = app.get_html("/admin/password").await;
        assert!(html.contains(r#"<p><i>New passwords don't match</i></p>"#));
    }
}

#[tokio::test]
async fn new_password_same_as_current() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let login_form = serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    });

    // Act 1 login
    let response = app.post_login(login_form).await;
    assert_redirects_to(&response, "/admin/dashboard");

    // Act 2 apply mismatched new passwords to change password form
    let change_pwd_form = serde_json::json!({
        "current_password": &app.test_user.password,
        "new_password": &app.test_user.password,
        "confirm_password": &app.test_user.password
    });
    let response = app.post_form("/admin/password", change_pwd_form).await;
    assert_redirects_to(&response, "/admin/password");

    let html = app.get_html("/admin/password").await;
    assert!(html.contains(r#"<p><i>New password must be different with current password</i></p>"#));
}

#[tokio::test]
async fn change_password_succeed() {
    // Arrange
    let app = spawn_app().await.unwrap();
    let login_form = serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    });

    // Act 1 login
    let response = app.post_login(login_form).await;
    assert_redirects_to(&response, "/admin/dashboard");

    // Act 2 apply mismatched new passwords to change password form
    let change_pwd_form = serde_json::json!({
        "current_password": &app.test_user.password,
        "new_password": "very_weak_password",
        "confirm_password": "very_weak_password"
    });
    let response = app.post_form("/admin/password", change_pwd_form).await;
    assert_redirects_to(&response, "/admin/password");

    let html = app.get_html("/admin/password").await;
    assert!(html.contains(r#"<p><i>Password changed</i></p>"#));
}

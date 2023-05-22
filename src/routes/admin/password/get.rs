use actix_web::http::header::ContentType;
use actix_web::HttpResponse;
use actix_web_flash_messages::IncomingFlashMessages;
use std::fmt::Write;

pub async fn change_password_form(
    flash_messages: IncomingFlashMessages,
) -> Result<HttpResponse, actix_web::Error> {
    let mut msg_html = String::new();
    for m in flash_messages.iter() {
        writeln!(msg_html, "<p><i>{}</i></p>", m.content()).unwrap();
    }

    Ok(HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(
            r#"<!DOCTYPE html>
        <html lang="en">
          <head>
            <meta http-equiv="content-type" content="text/html; charset=utf-8" />
            <title>Change Password</title>
          </head>
          <body>
            {msg_html}
            <form method="post" action="/admin/password">
              <label
                >Current Password
                <input
                  type="password"
                  placeholder="Enter current password"
                  name="current_password"
                />
              </label>
              <label
                >New Password
                <input
                  type="password"
                  placeholder="Enter new password"
                  name="new_password"
                />
              </label>
              <label
                >Confirm New Password
                <input
                  type="password"
                  placeholder="Type the new password again"
                  name="new_password_check"
                />
              </label>
              <br />
              <button type="submit">Change Password</button>
            </form>
            <p><a href="/admin/dashboard">&lt;- Back</a></p>
          </body>
        </html>        
        "#,
        )))
}

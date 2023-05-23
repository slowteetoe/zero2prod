use actix_web::{http::header::ContentType, HttpResponse};
use actix_web_flash_messages::IncomingFlashMessages;
use std::fmt::Write;

pub async fn publish_newsletter_form(flash_messages: IncomingFlashMessages) -> HttpResponse {
    let mut msg_html = String::new();
    for m in flash_messages.iter() {
        writeln!(msg_html, "<p><i>{}</i></p>", m.content()).unwrap();
    }
    HttpResponse::Ok()
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
            <form method="post" action="/admin/newsletters">
              <label
                >Title
                <input
                  type="text"
                  placeholder="Choose a title for the newsletter"
                  name="title"
                />
              </label>
              <label for="content.html"
                >HTML content
                <textarea rows="4" cols="60" required="true" name="content.html"
                  placeholder="Enter newsletter HTML content"
                />
              </label>
              <label for="content.text
                >Plaintext content
                <textarea rows="4" cols="60" required="true" name="content.text"
                  placeholder="Enter newsletter plain text content"
                />
              </label>
              <br />
              <button type="submit">Send Newsletter</button>
            </form>
            <p><a href="/admin/dashboard">&lt;- Back</a></p>
          </body>
        </html>        
        "#,
        ))
}

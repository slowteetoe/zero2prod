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
                <style>
                  *, *:before, *:after {{
                    box-sizing: border-box;
                  }}
                  label {{
                    padding: 10px;
                    display: block;
                  }}
                  input[type=text], textarea {{
                    padding: 10px;
                    width: 60%;
                    margin: 10px 0;
                    border: 0;
                    box-shadow:0 0 15px 4px rgba(0,0,0,0.26);
                    border-radius:10px;
                  }}
                  button {{
                      /* remove default behavior */
                      appearance:none;
                      -webkit-appearance:none;
    
                      /* usual styles */
                      padding:10px;
                      border:none;
                      background-color:#3F51B5;
                      color:#fff;
                      font-weight:600;
                      border-radius:5px;
                      width:60%;
                  }}
                </style>
              </head>
              <body>
                {msg_html}
                <form method="post" action="/admin/newsletters">
                  <div>
                    <label for="title">Title</label>
                      <input
                        type="text"
                        placeholder="Choose a title for the newsletter"
                        name="title" required="true"/>
                  </div>
                  <br>
                  <div>
                    <label for="html_content">HTML content</label>
                    <textarea rows="4" cols="60" required="true" name="html_content"
                      placeholder="Enter newsletter HTML content"></textarea>
                  </div>
                  <br>
                  <label for="text_content">Plaintext content</label>
                  <textarea rows="4" cols="60" required="true" name="text_content"
                    placeholder="Enter newsletter plain text content"></textarea>
                  <br>
                  <button type="submit">Send Newsletter</button>
                </form>
                <p><a href="/admin/dashboard">&lt;- Back</a></p>
              </body>
            </html>
        "#,
        ))
}

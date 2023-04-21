use std::net::TcpListener;

use sqlx::{Connection, PgConnection, PgPool};
use zero2prod::configuration::get_configuration;
use zero2prod::startup::run;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let configuration = get_configuration().expect("Failed to read configuration");
    let connection_pool = PgPool::connect(&configuration.database.connection_string())
        .await
        .expect("failed to connect to postgres");
    let server_address = format!("127.0.0.1:{}", configuration.application_port);
    let listener = TcpListener::bind(server_address)?;
    run(listener, connection_pool)?.await
}

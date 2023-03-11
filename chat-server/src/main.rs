mod server;
mod client;

use std::{env, error::Error};
use server::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    if env::var_os("RUST_LOG").is_none() {
        env::set_var("RUST_LOG", "chat_server=trace");
    }
    pretty_env_logger::init();
    
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <port>", args[0]);
        std::process::exit(1);
    }

    let port = args[1].parse()?;
    let mut server = Server::new(port).await?;

    server.serve().await;

    Ok(())
}

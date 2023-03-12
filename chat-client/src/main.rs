use tokio_util::codec::{FramedRead, LengthDelimitedCodec, LinesCodec};
use tokio::{net::TcpStream, signal, select};
use futures::{stream::StreamExt, SinkExt};
use log::{error, trace, info};
use chat_common::ChatProtocol;
use std::{env, error::Error, io::Write};

fn flush() {
    std::io::stdout().flush().unwrap();
}

fn clear_terminal() {
    print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
    flush();
}

fn prompt(p: &str) {
    print!("{p}");
    flush();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    if env::var_os("RUST_LOG").is_none() {
        env::set_var("RUST_LOG", "chat_server=trace");
    }
    pretty_env_logger::init();
    
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <hostname> <port>", args[0]);
        std::process::exit(1);
    }

    let mut stdin = FramedRead::new(tokio::io::stdin(), LinesCodec::new());
    prompt("Please enter your nickname: ");
    let client_name = if let Some(Ok(name)) = stdin.next().await {
        if name.len() == 0 {
            eprintln!("Please enter your name.");
            std::process::exit(1);
        }

        name
    } else {
        eprintln!("Please enter your name.");
        std::process::exit(1);
    };

    clear_terminal();

    let host = args[1].clone();
    let port: u16 = args[2].parse()?;
    let server_addr = format!("{host}:{port}");
    info!("Connecting to server {}...", server_addr);

    let stream = match TcpStream::connect(&server_addr).await {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("Failed to connect to {}: {:?}... Stopping.", server_addr, e);
            std::process::exit(1);
        }
    };

    info!("Connected to server. Use CTRL+C to quit.");
    let mut framed = LengthDelimitedCodec::builder()
        .length_field_type::<u32>()
        .new_framed(stream);

    let name_msg = ChatProtocol::ClientJoined(client_name.clone());
    let name_msg = bincode::serialize(&name_msg).unwrap();
    if let Err(e) = framed.send(name_msg.into()).await {
        error!("TCP send error: {:?}", e);
        std::process::exit(1);
    }

    prompt("       You");

    loop {
        select! {
            quit = signal::ctrl_c() => match quit {
                Ok(_) => break,
                Err(_) => {
                    error!("failed to listen for CTRL+C event");
                    break;
                }
            },
            Some(Ok(line)) = stdin.next() => {
                let msg = ChatProtocol::Message(client_name.clone(), line.trim_end().into());
                
                trace!("Sending message to server: {:?}", msg);
                let bytes = bincode::serialize(&msg).unwrap();
                if let Err(e) = framed.send(bytes.into()).await {
                    error!("TCP send error: {:?}", e);
                    std::process::exit(1);
                }
                prompt("       You : ");
            },
            Some(Ok(bytes)) = framed.next() => {
                let msg: ChatProtocol = match bincode::deserialize(&bytes) {
                    Ok(m) => m,
                    Err(e) => {
                        error!("Deserialize error: {:?}", e);
                        std::process::exit(1);
                    }
                };

                match msg {
                    ChatProtocol::Message(c, msg)       => println!("{c: >10} : {msg}"),
                    ChatProtocol::ClientJoined(c)       => println!("\"{c}\" joined!"),
                    ChatProtocol::ClientDisconnected(c) => println!("\"{c}\" disconnected.")
                }
            }
        }
    }

    Ok(())
}

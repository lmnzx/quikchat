use std::net::SocketAddr;

use rand::Rng;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::{tcp::WriteHalf, TcpListener, TcpStream},
    sync::broadcast::{self, error::RecvError, Sender},
};

type IOResult<T> = std::io::Result<T>;

#[derive(Debug, Clone)]
struct MsgType {
    socket_addr: SocketAddr,
    payload: String,
    from_id: String,
}

fn generate_id(length: u32) -> String {
    let characters: Vec<char> = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
        .chars()
        .collect();
    let mut rng = rand::thread_rng();

    let x = (0..length)
        .map(|_| characters[rng.gen_range(0..characters.len())])
        .collect::<String>();

    return x;
}

async fn read_from_broadcast_channel(
    result: Result<MsgType, RecvError>,
    socket_addr: SocketAddr,
    writer: &mut BufWriter<WriteHalf<'_>>,
    id: &str,
) -> IOResult<()> {
    match result {
        Ok(it) => {
            let msg: MsgType = it;
            println!("[{}]: channel: {:?}", id, msg);
            if msg.socket_addr != socket_addr {
                writer.write(msg.payload.as_bytes()).await?;
                writer.flush().await?;
            }
        }
        Err(error) => {
            eprintln!("{:?}", error);
        }
    }

    Ok(())
}

async fn handle_socket_read(
    num_bytes_read: usize,
    id: &str,
    incoming: &str,
    writer: &mut BufWriter<WriteHalf<'_>>,
    tx: Sender<MsgType>,
    socket_addr: SocketAddr,
) -> IOResult<()> {
    println!(
        "[{}]: incoming: {}, size: {}",
        id,
        incoming.trim(),
        num_bytes_read
    );

    writer
        .write(format!("{}\n", incoming.trim()).as_bytes())
        .await?;
    writer.flush().await?;

    let _ = tx.send(MsgType {
        socket_addr,
        payload: incoming.to_string(),
        from_id: id.to_string(),
    });

    println!(
        "[{}]: outgoing: {}, size: {}",
        id,
        incoming.trim(),
        num_bytes_read
    );

    Ok(())
}

async fn handle_client_task(
    mut stream: TcpStream,
    tx: Sender<MsgType>,
    socket_addr: SocketAddr,
) -> IOResult<()> {
    println!("handle socket connections from client");

    let id = generate_id(10);
    let mut rx = tx.subscribe();

    let (reader, writer) = stream.split();
    let mut reader = BufReader::new(reader);
    let mut writer = BufWriter::new(writer);

    let welcome_msg = format!("addr: {}, id: {}\n", socket_addr, id);
    writer.write(welcome_msg.as_bytes()).await?;
    writer.flush().await?;

    let mut incoming = String::new();

    loop {
        let tx = tx.clone();
        tokio::select! {
            // read from the broadcast channel
            result = rx.recv() => {
                read_from_broadcast_channel(result, socket_addr, &mut writer, &id).await?;
            }

            // read from socket
            network_read_result = reader.read_line(&mut incoming) => {
                let num_bytes_read: usize = network_read_result?;
                // EOF check
                if num_bytes_read == 0 {
                    break;
                }
                handle_socket_read(num_bytes_read, &id, &incoming, &mut writer, tx, socket_addr).await?;
                incoming.clear();
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> IOResult<()> {
    let addr = "0.0.0.0:3000";

    let listener = TcpListener::bind(addr).await?;
    println!("server is accepting connections on {}", addr);

    let (tx, _) = broadcast::channel::<MsgType>(10);

    loop {
        let (stream, socket_addr) = listener.accept().await?;

        let tx = tx.clone();

        tokio::spawn(async move {
            let result = handle_client_task(stream, tx, socket_addr).await;
            match result {
                Ok(_) => {
                    println!("handle_client_task() terminated gracefully")
                }
                Err(error) => {
                    println!("handle_client_task() encounted error: {}", error)
                }
            }
        });
    }
}

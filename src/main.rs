// Jackson Coxson

use std::sync::Arc;

use chat::ChatMessage;
use log::{debug, error, info, warn};
use thirtyfour::{error::WebDriverResult, By};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Mutex,
};

use crate::cache::Cache;

mod browser;
mod cache;
mod chat;
mod config;

async fn entry() -> WebDriverResult<()> {
    let config = config::Config::load();
    let client = browser::Browser::new(&config).await.unwrap();

    if !client.is_logged_in().await
        && client
            .login()
            .await
            .is_err()
    {
        warn!("Cookies are invalid, logging in again");
        client
            .login()
            .await
            .unwrap();
    }

    client.wrap_up().await?;

    let listener =
        tokio::net::TcpListener::bind(format!("{}:{}", config.tcp.host, config.tcp.port))
            .await
            .unwrap();

    let senders = Arc::new(Mutex::new(Vec::new()));
    let tcp_senders = senders.clone();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<ChatMessage>(100);

    tokio::spawn(async move {
        loop {
            if let Ok((mut stream, addr)) = listener.accept().await {
                info!("Accepted connection from {:?}", addr);

                let (local_tx, mut local_rx) = tokio::sync::mpsc::channel::<ChatMessage>(100);
                let tx = tx.clone();
                tcp_senders.lock().await.push(local_tx);

                tokio::spawn(async move {
                    loop {
                        let mut buf = [0; 4096];
                        tokio::select! {
                            msg = local_rx.recv() => {
                                let msg = serde_json::to_string(&msg).unwrap();
                                if stream.write(msg.as_bytes()).await.is_err() {
                                    break;
                                }
                                if stream.flush().await.is_err() {
                                    warn!("Unable to flush message to client");
                                    break;
                                }
                            }
                            x = stream.read(&mut buf) => {
                                if let Ok(x) = x {
                                    if let Ok(buf) = String::from_utf8(buf[0..x].to_vec()) {
                                        if x == 0 {
                                            break;
                                        }
                                        // Split the buf into JSON packets
                                        // As we've learned, sometimes nagle's algo will squish them
                                        // together into one packet, so we need to split them up
                                        let packets = buf.split("}{")
                                            .map(|s| {
                                                let s = if s.ends_with('}') {
                                                    s.to_string()
                                                } else {
                                                    format!("{s}}}")
                                                };
                                                if s.starts_with('{') {
                                                    s.to_string()
                                                } else {
                                                    format!("{{{s}")
                                                }
                                            })
                                            .collect::<Vec<_>>();

                                        for packet in packets {
                                            if let Ok(mut msg) = serde_json::from_str::<ChatMessage>(&packet) {
                                                msg.clean();
                                                tx.send(msg).await.unwrap();
                                            } else {
                                                warn!("Failed to parse msg: {:?}", buf);
                                            }
                                        }
                                    }
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                });
            }
        }
    });

    let mut cache = Cache::new();
    // let current_chat = client.get_current_chat().await.unwrap();
    // cache
    //     .check(&current_chat, &client.get_messages(false).await.unwrap())
    //     .await;

    let mut error_count: u8 = 0;

    info!("Startup complete");
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(
            config.refresh_rate as u64 / 2,
            ))
            .await;
        loop {
        // Check for unread messages
        // We can't get messages without selecting a chat first!
        let mut chats = match client.get_chats().await {
            Ok(chats) => chats,
            Err(e) => {
                error!("Unable to get chats: {:?}", e);
                error_count += 1;
                if error_count > 10 {
                    return Err(e);
                }
                break;
            }
        };
        debug!("Unread chats: {chats:?}");
        chats.retain(|chat| chat.unread || (!cache.check_key(&chat.id) && cache.size() < 20));
        if !chats.is_empty() {
            if chats[0].click(config.latency).await.is_err() {
                if let Err(e) = client.refresh().await {
                    error!("Unable to refresh, aborting Holly!");
                    return Err(e);
                }
            }
        } else {
            break
        }

        if let Ok(b) = chats[0].element.find(By::XPath(".//button[@aria-label=\"Mark as read\"]")).await {
            b.click().await?;
        }

        // See if the current chat has different messages than before
        let current_message = match client.get_messages(false, chats[0].id.clone()).await {
            Ok(c) => c,
            Err(e) => {
                error!("Unable to get messages: {:?}", e);
                error_count += 1;
                if error_count > 10 {
                    return Err(e);
                }
                break;
            }
        };

        // tokio::time::sleep(std::time::Duration::from_millis(
        //     config.refresh_rate as u64 / 2,
        // ))
        // .await;

        // let second_sample = match client.get_messages(false, chats[0].id.clone()).await {
        //     Ok(c) => c,
        //     Err(e) => {
        //         error!("Unable to get messages: {:?}", e);
        //         error_count += 1;
        //         if error_count > 10 {
        //             return Err(e);
        //         }
        //         break;
        //     }
        // };

        // if current_message != second_sample {
        //     warn!("Message samples don't match!");
        //     break;
        // }

        if let Some(unread_messages) = cache.check(&chats[0].id, &current_message).await {
            for message in unread_messages {
                info!(
                    "in {}: {}",
                    chats[0].id, message.content
                );
                let blocking_senders = senders.clone();
                tokio::task::spawn_blocking(move || {
                    blocking_senders
                        .blocking_lock()
                        .retain(|sender| sender.blocking_send(message.clone()).is_ok());
                });
            }
        }
        break;
        }

        // Possibly send a message
        if let Ok(msg) = rx.try_recv() {
            match msg.sender.as_str() {
                "<screenshot>" => {
                    if let Err(e) = client.screenshot_log().await {
                        error!("Unable to take screenshot!");
                        error_count += 1;
                        if error_count > 10 {
                            return Err(e);
                        }
                    }
                    continue;
                }
                "<html>" => {
                    if let Err(e) = client.html_log().await {
                        error!("Unable to take html log!");
                        error_count += 1;
                        if error_count > 10 {
                            return Err(e);
                        }
                    }
                    continue;
                }
                "<restart>" => return Ok(()),
                "<refresh>" => {
                    client.refresh().await?;
                    continue;
                }
                "<file>" => {
                    error!("File sending is not yet implemented");
                    continue;
                    info!("Sending file!");
                    if let Err(e) = client.go_to_chat(&msg.chat_id).await {
                        error!("Unable to go to chat for file send: {:?}", e);
                        error_count += 1;
                        if error_count > 10 {
                            return Err(e);
                        }
                        continue;
                    }
                    if let Err(e) = client.send_file(&msg.content).await {
                        error!("Unable to send file: {:?}", e);
                        error_count += 1;
                        if error_count > 10 {
                            return Err(e);
                        }
                        continue;
                    }
                    continue;
                }
                _ => {
                    info!("Sending message: {:?}", msg);
                    if let Err(e) = client.go_to_chat(&msg.chat_id).await {
                        error!("Unable to go to chat for send: {:?}", e);
                        error_count += 1;
                        if error_count > 10 {
                            return Err(e);
                        }
                        continue;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(config.latency as u64))
                        .await;
                    if let Err(e) = client.send_message(&msg.content).await {
                        error!("Unable to send message: {:?}", e);
                        error_count += 1;
                        if error_count > 10 {
                            return Err(e);
                        }
                        continue;
                    }
                    continue;
                }
            }
        }

        // Until next time *rides motorcycle away*
        
    }
}

#[tokio::main]
async fn main() {
    println!("Starting Holly core...");

    if std::env::var("RUST_LOG").is_err() {
        println!("Don't forget to initialize the logger with the RUST_LOG env var!!");
    }

    env_logger::init();
    info!("Logger initialized");

    let mut last_error = std::time::Instant::now();
    let mut errored = false;

    loop {
        if let Err(e) = entry().await {
            error!("Holly crashed with {:?}", e);
            if last_error.elapsed().as_secs() > 60 {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                info!("Restarting Holly...");
                last_error = std::time::Instant::now();
                errored = false;
            } else if errored {
                panic!("Holly has run into an unrecoverable state!")
            } else {
                errored = true;
            }
        } else {
            errored = false;
            info!("Holly is restarting...");
        }
    }
}

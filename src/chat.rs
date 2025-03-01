// Jackson Coxson

use std::{
    fmt::{Debug, Formatter},
    time::Duration,
};

use log::{debug, warn, trace};
use serde::{Deserialize, Serialize};
use thirtyfour::prelude::*;

/// A chat found on the sidebar.
/// Includes whether or not the chat is unread.
pub struct ChatOption {
    pub id: String,
    pub element: WebElement,
    pub unread: bool,
}

/// A message found in a chat.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub sender: String,
    pub content: String,
    pub chat_id: String,
}

impl ChatOption {
    /// Gets all the chats in the sidebar
    pub async fn get_all(driver: &WebDriver) -> WebDriverResult<Vec<ChatOption>> {
        trace!("ChatOption::get_all");
        // Get the chats object
        let chats_object = driver
            .query(By::Css("div[role=\"list\"]"))
            .wait(Duration::from_secs(15), Duration::from_millis(100))
            .first()
            .await?;

        // Get all the chat options
        let chat_options = chats_object
            .find_all(By::XPath("./span"))
            .await?;

        // Create a vector to store the chat options
        let mut chat_options_vec: Vec<ChatOption> = Vec::new();

        for chat in chat_options {
            // Get chat ID
            let id = chat.attr("data-group-id").await?.unwrap();

            // Determine if the unread marker is there
            let unread = chat.find(By::XPath(".//div[@class=\"FbT2ze\"]")).await.is_ok();

            // Add the chat option to the vector
            chat_options_vec.push(ChatOption {
                id,
                element: chat,
                unread,
            });
        }
        Ok(chat_options_vec)
    }

    /// Clicks on the sidebar, thereby navigating to the chat
    pub async fn click(&self, latency: usize) -> WebDriverResult<()> {
        trace!("ChatOption::click");
        self.element.scroll_into_view().await?;
        self.element.click().await?;
        tokio::time::sleep(std::time::Duration::from_millis(latency as u64)).await;
        Ok(())
    }
}

impl ChatMessage {
    /// Gets all the chat messages in the current chat
    pub async fn get(
        driver: &WebDriver,
        chat_id: String,
        last: bool,
    ) -> WebDriverResult<Vec<Self>> {
        trace!("ChatMessage::get");
        // Get the chat container
        let chat_container = driver
            .query(By::XPath(
                "//div[@class=\"SvOPqd\"]",
            ))
            .wait(Duration::from_secs(2), Duration::from_millis(100))
            .first()
            .await?;

        // Get all the messages in the chat container
        let mut tries = 0;
        let messages = loop {
            debug!("Getting chat messages from container");
            let messages = chat_container
                .find_all(By::XPath("./c-wiz"))
                .await?;
            if messages.len() > 13 || tries > 1 {
                if last && !messages.is_empty() {
                    break vec![messages.last().unwrap().to_owned()];
                }
                break messages;
            }
            debug!("Failed to get at least 13 chat messages, trying again...");
            tries += 1;
            tokio::time::sleep(std::time::Duration::from_secs(1)).await
        };
        if messages.is_empty() {
            warn!("Collected no messages!");
        }

        let mut res = Vec::new();
        let mut sender = String::default(); // Not implemented yet...
        for message in messages {
            let cont = message.text().await?; // I'm not even going to bother doing this better
            //debug!("Message: {}",cont);
            let lines: Vec<&str> = cont.split(',').collect();
            let content = if lines.len() == 4 {
                sender = lines[0].trim().to_owned();
                lines[2].trim().to_owned()
            } else if lines.len() == 2 {
                lines[0].trim().to_owned()
            } else {
                continue
            };

            if &sender == "" || &sender == "You" { continue }

            res.push(Self {
                content,
                sender: sender.to_owned(),
                chat_id: chat_id.clone(),
            });
        }

        Ok(res)
    }

    /// Removes special characters that can't be sent into Messenger
    pub fn clean(&mut self) {
        self.content = unidecode::unidecode(&self.content);
    }
}

impl Debug for ChatOption {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chat")
            .field("id", &self.id)
            .field("unread", &self.unread)
            .finish()
    }
}

impl Debug for ChatMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let msg_chars = &self.content.chars().collect::<Vec<char>>();
        let msg = if msg_chars.len() > 50 {
            format!("{}...", &msg_chars[..50].iter().collect::<String>())
        } else {
            self.content.to_string()
        };
        f.debug_struct("Msg")
            .field("msg", &msg)
            .field("chat_id", &self.chat_id)
            .finish()
    }
}

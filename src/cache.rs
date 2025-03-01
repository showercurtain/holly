// Jackson Coxson
// Cache for messages fetched in chats
// The idea is to make a more solid way to determine if a message is new or not.
// As of writing, the way to compare messages was to compare the content and sender.
// This meant that if a person sent the same message twice, it was ignored.
// Facebook ships roughly 13 messages on load, which means we can compare a tree.

use std::collections::HashMap;

use log::{debug, info, warn};

use crate::chat::ChatMessage;

pub struct Cache {
    inner: HashMap<String, Vec<ChatMessage>>,
}

impl Cache {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    pub async fn check(
        &mut self,
        chat_id: &str,
        new_messages: &Vec<ChatMessage>,
    ) -> Option<Vec<ChatMessage>> {
        let old_messages = match self.inner.get(chat_id) {
            Some(o) => o,
            None => {
                info!("Inserting new chat into cache: {:?}", chat_id);
                self.inner.insert(chat_id.to_owned(), new_messages.clone());
                return None;
            }
        };

        if old_messages.is_empty() {
            warn!("Cache for {chat_id} was empty");
            self.inner.insert(chat_id.to_owned(), new_messages.clone());
            return None;
        }
        if new_messages.is_empty() {
            warn!("Comparing against empty new messages");
            return None;
        }

        if new_messages == old_messages {
            return None;
        }

        let debug_lines = new_messages.iter().zip(old_messages.iter());
        debug!("New Messages | Old Messages");
        for (n, o) in debug_lines {
            debug!("{:?} | {:?}", n.content, o.content);
        }

        let mut new_count = 0;
        let mut old_count = 0;
        let last_old_message = old_messages.last().cloned();
        let last_new_message = new_messages.last().cloned();
        loop {
            if old_messages[old_count] == new_messages[new_count] {
                new_count += 1;
            }
            old_count += 1;

            if old_count == old_messages.len() {
                self.inner.insert(chat_id.to_owned(), new_messages.clone());
                if new_count > 3 {
                    return Some(new_messages[new_count..].to_vec());
                } else {
                    warn!("New messages had no match on old messages");
                    // We'll just return a single latest message since the caches don't match up
                    if last_old_message != last_new_message {
                        return Some(vec![last_new_message?]);
                    }
                    return None;
                }
            }
            if new_count == new_messages.len() {
                debug!("The new message length was somehow shorter?");
                return None;
            }
        }
    }

    pub fn check_key(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    pub fn size(&self) -> usize {
        self.inner.len()
    }
}

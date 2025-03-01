// Jackson Coxson
// But also Sean Cowley

use std::{io::Write, process::Stdio, time::Duration};

use log::{error, info, trace, warn};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};
use thirtyfour::prelude::*;
use tokio::process::{Child, Command};

use crate::config::Config;

pub struct Browser {
    driver: WebDriver,
    latency: usize,
    _gecko: Child,
}

#[derive(Serialize, Deserialize)]
struct JsonCookie {
    name: String,
    value: String,
    domain: String,
    path: String,
}

impl Browser {
    pub async fn new(config: &Config) -> Result<Self, WebDriverResult<()>> {
        trace!("Browser::new");
        let _gecko = launch_driver(&config.gecko.path, config.gecko.port);
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let mut caps = DesiredCapabilities::chrome();
        caps.add_chrome_arg("start-maximized").unwrap();
        caps.add_chrome_arg("disable-infobars").unwrap();
        //caps.add_chrome_arg("--disable-dev-shm-usage").unwrap();
        caps.add_chrome_arg("--disable-extensions").unwrap();
        //caps.add_chrome_arg("--no-sandbox").unwrap();
        caps.add_chrome_arg("--remote-debugging-pipe").unwrap();
        
        caps.add_chrome_arg("--user-data-dir=./data").unwrap();

        if !std::fs::exists("./data").unwrap_or(true) && config.gecko.headless {
            info!("No chrome data found, running not headless for sign-in")
        } else if config.gecko.headless {
            caps.add_chrome_arg("--headless").unwrap();
            caps.add_chrome_arg("--disable-gpu").unwrap();
        }

        let driver = WebDriver::new("http://localhost:4444", caps).await.unwrap();

        driver.goto("https://mail.google.com/chat").await.unwrap();

        Ok(Self {
            driver,
            _gecko,
            latency: config.latency,
        })
    }

    // Log into Google Chat manually
    // Only works if we're not on headless mode
    pub async fn login(&self) -> WebDriverResult<()> {
        trace!("Browser::login");
        self.driver.goto("https://mail.google.com/chat").await?;

        print!("Waiting for login... Press enter when finished");
        let _ = std::io::stdout().flush();
        let _ = std::io::stdin().read_line(&mut String::new());

        Ok(())
    }

    /// If we're not logged in, mail.google.com will always redirect somewhere
    pub async fn is_logged_in(&self) -> bool {
        trace!("is_logged_in");
        if let Some(domain) = self.driver.current_url().await.unwrap().domain() {
            domain.contains("mail.google.com")
        } else {
            false
        }
    }

    pub async fn wrap_up(&self) -> WebDriverResult<()> {
        trace!("Browser::wrap_up");
        tokio::time::sleep(Duration::from_secs(2)).await;
        self.driver.switch_to().frame_element(&self.driver.find(By::XPath("//iframe[@class=\"bGz\"]")).await?).await
        // If anyone knows the not-deprecated way to do this, I'm all ears
    }

    /// Gets all the chats on the side bar. Includes whether or not they are unread.
    pub async fn get_chats(&self) -> WebDriverResult<Vec<crate::chat::ChatOption>> {
        trace!("Browser::get_chats");
        crate::chat::ChatOption::get_all(&self.driver).await
    }

    /// Navigates the browser to the chat with the given id.
    /// Attempts to find it on the side bar to click that object.
    /// If it's not found, it will just navigate via URL.
    pub async fn go_to_chat(&self, id: &str) -> WebDriverResult<()> {
        trace!("Browser::go_to_chat");
        //self.decline_call().await.unwrap();
        let chats = self.get_chats().await?;
        match chats.iter().find(|c| c.id == id) {
            Some(chat) => {
                chat.click(self.latency).await?;
            }
            None => {
                // Manually go
                self.driver
                    .goto(format!("https://mail.google.com/chat/u/0/#chat/{}", id))
                    .await?;
            }
        }
        Ok(())
    }

    /// Refreshes the tab
    pub async fn refresh(&self) -> WebDriverResult<()> {
        trace!("Browser::refresh");
        self.driver.refresh().await?;
        self.wrap_up().await
    }

    /// Takes a screenshot and saves it to logs/timestamp.png
    pub async fn screenshot_log(&self) -> WebDriverResult<()> {
        let b = self.driver.screenshot_as_png().await?;
        let timestamp = chrono::offset::Local::now().to_string();

        // Create the log folder if not created
        if let Err(e) = tokio::fs::create_dir_all("logs").await {
            error!("Could not create logs folder: {:?}", e);
            return Err(WebDriverError::CustomError(
                "Could not create logs folder".to_string(),
            ));
        }

        match tokio::fs::File::create(format!("logs/{timestamp}-log.png")).await {
            Ok(mut file) => {
                if tokio::io::AsyncWriteExt::write_all(&mut file, &b)
                    .await
                    .is_err()
                {
                    error!("Could not write screenshot data to file");
                    return Err(WebDriverError::CustomError(
                        "Could not write screenshot data to file".to_string(),
                    ));
                }
                Ok(())
            }
            Err(e) => {
                error!("Could not create file to save screenshot: {:?}", e);
                Err(WebDriverError::CustomError(
                    "Could not create file to save screenshot".to_string(),
                ))
            }
        }
    }

    /// Takes a snapshot of the page HTML and saves to to logs/timestamp.html
    pub async fn html_log(&self) -> WebDriverResult<()> {
        let html = self.driver.source().await?;
        let timestamp = chrono::offset::Local::now().to_string();

        // Create the log folder if not created
        if let Err(e) = tokio::fs::create_dir_all("logs").await {
            error!("Could not create logs folder: {:?}", e);
            return Err(WebDriverError::CustomError(
                "Could not create logs folder".to_string(),
            ));
        }

        if let Ok(mut file) = tokio::fs::File::create(format!("logs/{timestamp}-log.html")).await {
            if tokio::io::AsyncWriteExt::write_all(&mut file, html.as_bytes())
                .await
                .is_err()
            {
                error!("Could not write html data to file");
                return Err(WebDriverError::CustomError(
                    "Could not write html data to file".to_string(),
                ));
            }
            Ok(())
        } else {
            error!("Could not create file to save html");
            Err(WebDriverError::CustomError(
                "Could not create file to save html".to_string(),
            ))
        }
    }

    /// Gets the list of all the messages in the current chat
    pub async fn get_messages(&self, last: bool, chat_id: String) -> WebDriverResult<Vec<crate::chat::ChatMessage>> {
        trace!("Browser::get_messages");
        crate::chat::ChatMessage::get(&self.driver, chat_id, last).await
    }

    /// Sends a message to the current chat
    pub async fn send_message(&self, message: &str) -> WebDriverResult<()> {
        trace!("Browser::send_message");
        //self.decline_call().await.unwrap();

        let chat_bar = match self
            .driver
            .query(By::XPath("//div[@role='textbox']"))
            .wait(
                std::time::Duration::from_secs(5),
                std::time::Duration::from_millis(100),
            )
            .all()
            .await
        {
            Ok(mut c) => c.pop().unwrap_or(
                self.driver
                    .find(By::XPath("//div[contains(@aria-label,'Message ')]"))
                    .await?
            ),
            Err(_) => {
                warn!("Unable to get sender box by textbox role");
                self.driver
                    .find(By::XPath("//div[contains(@aria-label,'Message ')]"))
                    .await?
            }
        };
        chat_bar.click().await?;

        let mut rand_gen = rand::thread_rng();
        for c in message.chars() {
            //self.decline_call().await.unwrap();
            let x = rand_gen.gen_range(1..=30);
            if x == 7 {
                for asdf in "asdf".chars() {
                    chat_bar.send_keys(String::from(asdf)).await?;
                    tokio::time::sleep(std::time::Duration::from_millis(
                        rand_gen.gen_range(10..=20),
                    ))
                    .await;
                }
                for _ in 0..4 {
                    chat_bar.send_keys(Key::Backspace + "").await?;
                    tokio::time::sleep(std::time::Duration::from_millis(
                        rand_gen.gen_range(10..=20),
                    ))
                    .await;
                }
            }
            if c == '\n' {
                chat_bar.send_keys(Key::Shift+Key::Enter).await?;
            } else {
                chat_bar.send_keys(String::from(c)).await?;
            }
            tokio::time::sleep(std::time::Duration::from_millis(
                rand_gen.gen_range(10..=20),
            ))
            .await;
        }
        chat_bar.send_keys(Key::Enter + "").await?;

        if let Ok(send_button) = self
            .driver
            .find(By::XPath("//div[@aria-label='Send message']"))
            .await
        {
            let _ = send_button.click().await;
        }

        Ok(())
    }

    pub async fn send_file(&self, path: &str) -> WebDriverResult<()> {
        todo!("Correct for Google Chats");
        let chat_bar = match self
            .driver
            .query(By::XPath("//div[@role='textbox']"))
            .wait(
                std::time::Duration::from_secs(5),
                std::time::Duration::from_millis(100),
            )
            .first()
            .await
        {
            Ok(c) => c,
            Err(_) => {
                warn!("Unable to get sender box by textbox role");
                self.driver
                    .find(By::XPath("//div[@aria-label='Message']"))
                    .await?
            }
        };
        chat_bar.click().await?;

        let ret = self
            .driver
            .execute(
                include_str!("drop.js"),
                vec![
                    chat_bar.to_json()?,
                    Value::Number(Number::from(0)),
                    Value::Number(Number::from(0)),
                ],
            )
            .await?
            .element()?;

        ret.send_keys(path).await?;

        // Detect an invalid file format
        if let Ok(dialogue) = self
            .driver
            .find(By::XPath("//div[@aria-label='Invalid file format']"))
            .await
        {
            warn!("File upload failed: invalid file format!");
            // Close the box
            dialogue
                .find(By::XPath("//div[@aria-label='Close']"))
                .await?
                .click()
                .await?;
        }

        // Detect a file upload
        if let Ok(dialogue) = self
            .driver
            .find(By::XPath("//div[@aria-label='Failed to upload files']"))
            .await
        {
            warn!("File upload failed! (Is the file below 25 MB?)");
            // Close the box
            dialogue
                .find(By::XPath("//div[@aria-label='Close']"))
                .await?
                .click()
                .await?;
        }

        chat_bar.click().await?;
        chat_bar.send_keys(Key::Enter + "").await?;

        if let Ok(send_button) = self
            .driver
            .find(By::XPath("//div[@aria-label='Press enter to send']"))
            .await
        {
            let _ = send_button.click().await;
        }

        Ok(())
    }
}

fn launch_driver(path: &str, port: u16) -> Child {
    trace!("launch_driver");
    Command::new(path)
        .arg(format!("--port={port}"))
        .kill_on_drop(true)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .expect("Unable to spawn chromedriver! Check that the path is correct!")
}

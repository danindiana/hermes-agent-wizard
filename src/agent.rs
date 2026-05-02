use serde::{Deserialize, Serialize};
use reqwest::Client;
use color_eyre::eyre::{eyre, Result};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Choice {
    message: Message,
}

pub struct Agent {
    pub client: Client,
    pub base_url: String,
    pub model: String,
    pub history: Vec<Message>,
}

impl Agent {
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            model: model.to_string(),
            history: Vec::new(),
        }
    }

    pub async fn chat(&mut self, user_input: &str) -> Result<String> {
        self.history.push(Message {
            role: "user".to_string(),
            content: user_input.to_string(),
        });

        let url = format!("{}/chat/completions", self.base_url);
        let request = ChatRequest {
            model: self.model.clone(),
            messages: self.history.clone(),
        };

        let response = self.client.post(&url)
            .json(&request)
            .send()
            .await?
            .json::<ChatResponse>()
            .await?;

        if let Some(choice) = response.choices.get(0) {
            let bot_msg = choice.message.clone();
            self.history.push(bot_msg.clone());
            Ok(bot_msg.content)
        } else {
            Err(eyre!("No response from model"))
        }
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }
}

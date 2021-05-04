use reqwest::blocking::Client;

const API_URL: &'static str =
    "https://api.telegram.org/bot1758156340:AAHaSu4qhKwNN9ynRq90bMu7Q4oxemS5rZE";

pub struct Bot {
    chat_id: i32,
    client: Client,
}

impl Bot {
    pub fn new() -> Self {
        Self {
            chat_id: -513690128,
            client: Client::builder().https_only(true).build().unwrap(),
        }
    }

    pub fn send_message(&self, message: &str) {
        let url = format!(
            "{}/sendMessage?chat_id={}&text={}",
            API_URL, self.chat_id, message
        );
        self.client.post(url).send().unwrap();
    }
}

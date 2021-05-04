use std::fmt;
use std::io::Read;

use super::Interval;
use crate::finder::Haystack;

use chrono::Utc;
use hmac::{Hmac, Mac, NewMac};
use reqwest::blocking::{Client, Response};
use sha2::Sha256;

const API_URL: &'static str = "https://api.binance.com";
const API_KEY: &'static str = "X-MBX-APIKEY";

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Error {
    code: i32,
    message: String,
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl Error {
    fn from_json(slice: &[u8]) -> Self {
        let mut code = 0;
        let mut message = String::new();

        let mut i: Option<usize> = None;
        let mut key: Option<&[u8]> = None;

        for (j, &element) in slice.iter().enumerate() {
            match key {
                None => match i {
                    None => {
                        if element == b'"' {
                            i = Some(j + 1);
                        }
                    }
                    Some(index) => {
                        if element == b'"' {
                            key = Some(&slice[index..j]);
                            i = None;
                        }
                    }
                },
                Some(bytes) => match i {
                    None => {
                        if element == b':' {
                            i = Some(j + 1);
                        }
                    }
                    Some(index) => {
                        if element == b',' || element == b'}' {
                            let value = &slice[index..j];

                            if bytes == b"code" {
                                code = std::str::from_utf8(value).unwrap().parse().unwrap();
                            } else if bytes == b"msg" {
                                let len = value.len();
                                message = String::from_utf8(value[1..len - 1].to_vec()).unwrap();
                            } else {
                                panic!("This should not happen");
                            }

                            key = None;
                            i = None;
                        }
                    }
                },
            }
        }

        Self { code, message }
    }
}

#[derive(Debug)]
pub struct Asset {
    pub name: String,
    pub balance: f64,
}

impl Asset {
    pub fn new(name: &str, balance: f64) -> Self {
        Self {
            name: name.to_string(),
            balance,
        }
    }
}

#[derive(Debug)]
pub struct SymbolString {
    inner: String,
    mid: usize,
}

impl SymbolString {
    pub fn new(base: &str, quote: &str) -> Self {
        let mut inner = String::with_capacity(base.len() + quote.len());
        inner.push_str(base);
        inner.push_str(quote);

        Self {
            inner,
            mid: base.len(),
        }
    }

    pub fn from_raw_parts(inner: String, mid: usize) -> Self {
        Self { inner, mid }
    }

    pub fn base(&self) -> &str {
        &self.inner[..self.mid]
    }

    pub fn quote(&self) -> &str {
        &self.inner[self.mid..]
    }

    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

impl fmt::Display for SymbolString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.inner)
    }
}

impl PartialEq for SymbolString {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for SymbolString {}

pub struct Account {
    _api_key: String,
    secret_key: String,
    client: Client,
}

impl Account {
    pub fn new() -> Self {
        let content = std::fs::read_to_string("config.txt").unwrap();
        let pos = content.find('\n').unwrap();
        let (s1, s2) = content.split_at(pos + 1);
        let api_key = s1.strip_prefix("api_key:").unwrap().trim();
        let secret_key = s2.strip_prefix("secret_key:").unwrap().trim();

        let mut map = reqwest::header::HeaderMap::new();
        map.insert(
            API_KEY,
            reqwest::header::HeaderValue::from_str(&api_key).unwrap(),
        );
        let client = Client::builder()
            .https_only(true)
            .default_headers(map)
            .build()
            .unwrap();

        Self {
            _api_key: String::from(api_key),
            secret_key: String::from(secret_key),
            client,
        }
    }

    pub fn get_kline_data(
        &self,
        symbol: &str,
        interval: Interval,
        start_time: Option<i64>,
        end_time: Option<i64>,
        limit: Option<u32>,
    ) -> Result<Response> {
        let mut url = format!(
            "{}/api/v3/klines?symbol={}&interval={}",
            API_URL, symbol, interval,
        );

        if let Some(time) = start_time {
            let parameter = format!("&startTime={}", time);
            url.push_str(&parameter);
        }

        if let Some(time) = end_time {
            let parameter = format!("&endTime={}", time);
            url.push_str(&parameter);
        }

        if let Some(limit) = limit {
            let parameter = format!("&limit={}", limit);
            url.push_str(&parameter);
        }

        let response = self.client.get(&url).send().unwrap();
        let status = response.status();

        if status.is_success() {
            Ok(response)
        } else {
            Err(Error::from_json(&response.bytes().unwrap()))
        }
    }

    pub fn market_buy(&self, symbol: &str, quote_order_quantity: f64) -> Result<Response> {
        let parameters = format!(
            "symbol={}&side=BUY&type=MARKET&quoteOrderQty={}&timestamp={}",
            symbol,
            quote_order_quantity,
            Utc::now().timestamp_millis(),
        );

        self.new_order(parameters)
    }

    pub fn market_sell(&self, symbol: &str, quantity: f64) -> Result<Response> {
        let parameters = format!(
            "symbol={}&side=SELL&type=MARKET&quantity={}&timestamp={}",
            symbol,
            quantity,
            Utc::now().timestamp_millis(),
        );

        self.new_order(parameters)
    }

    pub fn get_balance(&self, asset: &str) -> Result<f64> {
        let response = self.account_information()?;
        let bytes = response.bytes().unwrap();

        let mut haystack = Haystack::new(&bytes);
        let mut balance = String::new();

        haystack.find_key("balances");

        haystack.find_pair("asset", asset);
        haystack.find_key("free");

        while let Some(b) = haystack.peek() {
            if **b == b'"' {
                haystack.next().unwrap();
                break;
            }

            haystack.next().unwrap();
        }

        while let Some(b) = haystack.peek() {
            if b.is_ascii_digit() || **b == b'.' {
                balance.push(char::from(**b));
            }

            if **b == b'"' {
                break;
            }

            haystack.next().unwrap();
        }

        Ok(balance.parse().unwrap())
    }

    pub fn get_precision(&self, symbol: &str) -> usize {
        let mut f =
            std::fs::File::open("exchangeInfo.json").expect("Could not open exchangeInfo.json");
        let mut buffer = Vec::new();
        f.read_to_end(&mut buffer).unwrap();

        let mut haystack = Haystack::new(&buffer);
        let mut precision: Option<usize> = None;

        haystack.find_key("symbols");

        haystack.find_pair("symbol", symbol);
        haystack.find_key("filters");

        haystack.find_pair("filterType", "LOT_SIZE");
        haystack.find_key("stepSize");

        while let Some(b) = haystack.peek() {
            if let Some(ref mut p) = precision {
                *p += 1;
                if **b == b'1' {
                    break;
                }
            } else {
                if **b == b'1' {
                    precision = Some(0);
                    break;
                }

                if **b == b'.' {
                    precision = Some(0);
                }

                if **b == b',' {
                    break;
                }
            }

            haystack.next().unwrap();
        }

        precision.expect("Could not find precision of given symbol")
    }

    fn new_order(&self, parameters: String) -> Result<Response> {
        let signature = self.signed_endpoint(&parameters);
        let url = format!(
            "{}/api/v3/order?{}&signature={}",
            API_URL, parameters, signature,
        );

        let response = self.client.post(&url).send().unwrap();
        let status = response.status();

        if status.is_success() {
            Ok(response)
        } else {
            Err(Error::from_json(&response.bytes().unwrap()))
        }
    }

    pub fn test_order(&self, parameters: String) -> Result<Response> {
        let signature = self.signed_endpoint(&parameters);
        let url = format!(
            "{}/api/v3/order/test?{}&signature={}",
            API_URL, parameters, signature,
        );
        println!("{}", &url);
        let response = self.client.post(&url).send().unwrap();
        let status = response.status();

        if status.is_success() {
            Ok(response)
        } else {
            Err(Error::from_json(&response.bytes().unwrap()))
        }
    }

    fn signed_endpoint(&self, parameters: &str) -> String {
        let mut mac: Hmac<Sha256> = Hmac::new_varkey(self.secret_key.as_bytes()).unwrap();
        mac.update(parameters.as_bytes());
        format!("{:x}", mac.finalize().into_bytes())
    }

    pub fn account_information(&self) -> Result<Response> {
        let parameters = format!("timestamp={}", Utc::now().timestamp_millis());
        let signature = self.signed_endpoint(&parameters);
        let url = format!(
            "{}/api/v3/account?{}&signature={}",
            API_URL, parameters, signature
        );

        let response = self.client.get(&url).send().unwrap();
        let status = response.status();

        if status.is_success() {
            Ok(response)
        } else {
            Err(Error::from_json(&response.bytes().unwrap()))
        }
    }

    pub fn exchange_information(&self) -> Result<Response> {
        let url = format!("{}/api/v3/exchangeInfo", API_URL);
        let response = self.client.get(&url).send().unwrap();
        let status = response.status();

        if status.is_success() {
            Ok(response)
        } else {
            Err(Error::from_json(&response.bytes().unwrap()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_from_json() {
        let response1 = r#"{"code":-1120,"msg":"Invalid interval."}"#;
        let error1 = Error::from_json(response1.as_bytes());

        let response2 = r#"{"msg":"Invalid symbol.","code":-1121}"#;
        let error2 = Error::from_json(response2.as_bytes());

        assert_eq!(
            error1,
            Error {
                code: -1120,
                message: String::from("Invalid interval."),
            },
        );

        assert_ne!(error1, error2);

        assert_eq!(
            error2,
            Error {
                code: -1121,
                message: String::from("Invalid symbol."),
            },
        );
    }

    #[test]
    fn new_order() {
        let binance = Account::new();

        let parameters = format!(
            "symbol=ETHBTC&side=BUY&type=MARKET&quoteOrderQty=23.12345678&timestamp={}",
            Utc::now().timestamp_millis(),
        );

        let response = binance.test_order(parameters);

        assert!(response.is_ok());
        assert_eq!("{}".to_string(), response.unwrap().text().unwrap());
    }

    #[test]
    fn invalid_order() {
        let binance = Account::new();

        let parameters = format!(
            "symbol=ETHBTC&side=BUY&type=MARKET&quantity=20.12345678&timestamp={}",
            Utc::now().timestamp_millis(),
        );

        let response = binance.test_order(parameters);

        assert!(response.is_err());
    }
}

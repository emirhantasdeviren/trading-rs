use std::fmt;

use chrono::{DateTime, TimeZone, Utc};
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
    pub fn from_json(slice: &[u8]) -> Self {
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
pub struct Symbol {
    inner: String,
    mid: usize,
}

impl Symbol {
    pub fn new(base: &str, quote: &str) -> Self {
        let mut inner = String::with_capacity(base.len() + quote.len());
        inner.push_str(base);
        inner.push_str(quote);

        Self {
            inner,
            mid: base.len(),
        }
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

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.inner)
    }
}

impl PartialEq for Symbol {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for Symbol {}

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
        symbol: &Symbol,
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

    fn signed_endpoint(&self, parameters: &str) -> String {
        let mut mac: Hmac<Sha256> = Hmac::new_varkey(self.secret_key.as_bytes()).unwrap();
        mac.update(parameters.as_bytes());
        format!("{:x}", mac.finalize().into_bytes())
    }

    pub fn new_order(&self, parameters: String) -> Result<Response> {
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

#[derive(Clone, Copy)]
pub enum Interval {
    Minute(i32),
    Hour(i32),
    Day(i32),
    Week,
    Month,
}

impl fmt::Display for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Interval::Minute(t) => write!(f, "{}m", t),
            Interval::Hour(t) => write!(f, "{}h", t),
            Interval::Day(t) => write!(f, "{}d", t),
            Interval::Week => write!(f, "1w"),
            Interval::Month => write!(f, "1M"),
        }
    }
}

impl Interval {
    pub fn to_millis(&self) -> i64 {
        match self {
            Interval::Minute(m) => i64::from(*m) * 60 * 1000,
            Interval::Hour(h) => i64::from(*h) * 60 * 60 * 1000,
            Interval::Day(d) => i64::from(*d) * 24 * 60 * 60 * 1000,
            Interval::Week => 7 * 24 * 60 * 60 * 1000,
            Interval::Month => 30 * 60 * 60 * 1000,
        }
    }
}

pub struct Klines {
    pub open_time: Vec<DateTime<Utc>>,
    pub open: Vec<f64>,
    pub high: Vec<f64>,
    pub low: Vec<f64>,
    pub close: Vec<f64>,
    _volume: Vec<f64>,
    _close_time: Vec<DateTime<Utc>>,
    _quote_asset_volume: Vec<f64>,
    _number_of_trades: Vec<u32>,
    _tbbav: Vec<f64>,
    _tbqav: Vec<f64>,
}

impl Klines {
    pub fn last(&self) -> Kline {
        Kline {
            open: self.open.last().unwrap().clone(),
            high: self.high.last().unwrap().clone(),
            low: self.low.last().unwrap().clone(),
            close: self.close.last().unwrap().clone(),
            open_time: self.open_time.last().unwrap().clone(),
        }
    }
}

pub fn parse_kline_body(bytes: &[u8], data_count: usize) -> Klines {
    let mut found_open_bracket = false;
    let mut found_body = false;
    let mut index = 0;

    let mut tmp_bytes: Vec<u8> = Vec::new();

    let mut open_time: Vec<DateTime<Utc>> = Vec::with_capacity(data_count);
    let mut open: Vec<f64> = Vec::with_capacity(data_count);
    let mut high: Vec<f64> = Vec::with_capacity(data_count);
    let mut low: Vec<f64> = Vec::with_capacity(data_count);
    let mut close: Vec<f64> = Vec::with_capacity(data_count);
    let mut volume: Vec<f64> = Vec::with_capacity(data_count);
    let mut close_time: Vec<DateTime<Utc>> = Vec::with_capacity(data_count);
    let mut quote_asset_volume: Vec<f64> = Vec::with_capacity(data_count);
    let mut number_of_trades: Vec<u32> = Vec::with_capacity(data_count);
    let mut tbbav: Vec<f64> = Vec::with_capacity(data_count);
    let mut tbqav: Vec<f64> = Vec::with_capacity(data_count);

    for &byte in bytes {
        if byte == b'[' {
            found_open_bracket = true;
        } else if byte == b']' && found_body {
            found_open_bracket = false;
            found_body = false;
            index = 0;
            // tmp_bytes.clear();
        }

        if !found_body && found_open_bracket && byte.is_ascii_digit() {
            found_body = true;
        }

        if found_body {
            if (byte.is_ascii_digit() || byte == b'.') && index <= 10 {
                tmp_bytes.push(byte);
            } else if byte == b',' {
                let tmp_str = std::str::from_utf8(&tmp_bytes).unwrap();
                match index {
                    0 => {
                        let val: i64 = tmp_str.parse().unwrap();
                        open_time.push(Utc.timestamp_millis(val));
                    }
                    1 => {
                        let val: f64 = tmp_str.parse().unwrap();
                        open.push(val);
                    }
                    2 => {
                        let val: f64 = tmp_str.parse().unwrap();
                        high.push(val);
                    }
                    3 => {
                        let val: f64 = tmp_str.parse().unwrap();
                        low.push(val);
                    }
                    4 => {
                        let val: f64 = tmp_str.parse().unwrap();
                        close.push(val);
                    }
                    5 => {
                        let val: f64 = tmp_str.parse().unwrap();
                        volume.push(val);
                    }
                    6 => {
                        let val: i64 = tmp_str.parse().unwrap();
                        close_time.push(Utc.timestamp_millis(val));
                    }
                    7 => {
                        let val: f64 = tmp_str.parse().unwrap();
                        quote_asset_volume.push(val);
                    }
                    8 => {
                        let val: u32 = tmp_str.parse().unwrap();
                        number_of_trades.push(val);
                    }
                    9 => {
                        let val: f64 = tmp_str.parse().unwrap();
                        tbbav.push(val);
                    }
                    10 => {
                        let val: f64 = tmp_str.parse().unwrap();
                        tbqav.push(val);
                    }
                    _ => (),
                }
                tmp_bytes.clear();
                index += 1;
            }
        }
    }

    Klines {
        open_time,
        open,
        high,
        low,
        close,
        _volume: volume,
        _close_time: close_time,
        _quote_asset_volume: quote_asset_volume,
        _number_of_trades: number_of_trades,
        _tbbav: tbbav,
        _tbqav: tbqav,
    }
}

pub struct Kline {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    open_time: DateTime<Utc>,
}

impl Kline {
    pub fn from_bytes(slice: &[u8]) -> Self {
        let mut i: usize = 0;
        let mut index: usize = 0;

        let mut open: f64 = 0f64;
        let mut high: f64 = 0f64;
        let mut low: f64 = 0f64;
        let mut close: f64 = 0f64;
        let mut open_time: DateTime<Utc> = Utc.timestamp_millis(1004508300000);

        let mut found_i = false;

        for (count, &element) in slice.iter().enumerate() {
            if element.is_ascii_digit() && !found_i {
                found_i = true;
                i = count;
            }

            if found_i && (element == b'"' || element == b',') {
                let s = std::str::from_utf8(&slice[i..count]).unwrap();

                match index {
                    0 => open_time = Utc.timestamp_millis(s.parse().unwrap()),
                    1 => open = s.parse().unwrap(),
                    2 => high = s.parse().unwrap(),
                    3 => low = s.parse().unwrap(),
                    4 => {
                        close = s.parse().unwrap();
                        break;
                    }
                    _ => (),
                }

                index += 1;
                found_i = false;
            }
        }

        Self {
            open,
            high,
            low,
            close,
            open_time,
        }
    }

    pub fn open(&self) -> f64 {
        self.open
    }

    pub fn high(&self) -> f64 {
        self.high
    }

    pub fn low(&self) -> f64 {
        self.low
    }

    pub fn close(&self) -> f64 {
        self.close
    }

    pub fn time(&self) -> DateTime<Utc> {
        self.open_time
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
    fn test_order() {
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
    fn invalid_test_order() {
        let binance = Account::new();

        let parameters = format!(
            "symbol=ETHBTC&side=BUY&type=MARKET&quantity=20.12345678&timestamp={}",
            Utc::now().timestamp_millis(),
        );

        let response = binance.test_order(parameters);

        assert!(response.is_err());
    }

    #[test]
    fn symbol() {
        let a = Symbol::new("CAKE", "USDT");
        let b = Symbol::new("CAKE", "USDT");
        let c = Symbol::new("BTC", "USDT");

        assert_eq!(a.as_str(), "CAKEUSDT");
        assert_eq!(a.base(), "CAKE");
        assert_eq!(a.quote(), "USDT");

        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}

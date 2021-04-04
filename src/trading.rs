use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;

use chrono::{DateTime, TimeZone, Utc};

use crate::exchange::binance::{self, Account, Interval, Kline, Klines, Symbol};
use crate::finder::Haystack;
use crate::indicators::{Adx, BollingerBand};

#[derive(Clone, Copy)]
pub enum Mode {
    Backtest,
    Live,
}

pub struct Trader {
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    symbol: Symbol,
    interval: Interval,
    mode: Mode,
    quote: f64,
    base: f64,
    precision: usize,
}

impl Trader {
    fn get_balance(binance: &Account, asset: &str) -> f64 {
        let response = binance
            .account_information()
            .expect("Could not get account information");
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

        balance.parse().unwrap()
    }

    fn get_precision(symbol: &str) -> usize {
        let mut f = fs::File::open("exchangeInfo.json").expect("Could not open exchangeInfo.json");
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

    pub fn new(
        binance: &Account,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        symbol: Symbol,
        interval: Interval,
        mode: Mode,
        start_amount: f64,
    ) -> Self {
        let start_time = match mode {
            Mode::Live => {
                let now = Utc::now().timestamp_millis();
                let interval = interval.to_millis();
                Utc.timestamp_millis(now + interval - now % interval)
            }
            Mode::Backtest => start_time,
        };

        let start_amount = match mode {
            Mode::Live => Self::get_balance(binance, symbol.quote()),
            Mode::Backtest => start_amount,
        };

        let precision = Self::get_precision(symbol.as_str());

        let base = match mode {
            Mode::Live => {
                let mut base = Self::get_balance(binance, symbol.base());
                base -= base % 10f64.powi(-(precision as i32));
                base
            }
            Mode::Backtest => 0f64,
        };

        println!(
            "[INFO] Start Time: {}, Symbol: {}, Interval: {}\n[INFO] {} Balance: {}\n[INFO] {} Balance: {}\n",
            start_time, symbol, interval, symbol.base(), base, symbol.quote(), start_amount,
        );

        Self {
            start_time,
            end_time,
            symbol,
            interval,
            mode,
            quote: start_amount,
            base,
            precision,
        }
    }

    fn get_required_data(&self, binance: &Account) -> Klines {
        let interval: i64 = self.interval.to_millis();
        let prev_time = self.start_time.timestamp_millis() - interval;

        match self.mode {
            Mode::Backtest => {
                let data_count = (self.end_time.timestamp_millis()
                    - self.start_time.timestamp_millis())
                    / interval;

                let iteration = if data_count % 1000 == 0 {
                    data_count / 1000
                } else {
                    data_count / 1000 + 1
                };

                let path = self.file_name();

                match std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create_new(true)
                    .open(&path)
                {
                    Ok(mut f) => {
                        let response = binance
                            .get_kline_data(
                                &self.symbol,
                                self.interval,
                                None,
                                Some(prev_time),
                                Some(1000),
                            )
                            .expect("Could not get kline data");
                        f.write_all(&response.bytes().unwrap()).unwrap();

                        for i in 0..iteration {
                            let start_time =
                                self.start_time.timestamp_millis() + (i * interval * 1000);
                            let response = binance
                                .get_kline_data(
                                    &self.symbol,
                                    self.interval,
                                    Some(start_time),
                                    Some(self.end_time.timestamp_millis()),
                                    Some(1000),
                                )
                                .expect("Could not get kline data");

                            f.write_all(&response.bytes().unwrap()).unwrap();
                        }

                        f.sync_all().unwrap();
                        f.seek(SeekFrom::Start(0)).unwrap();
                        let mut bytes = Vec::new();
                        f.read_to_end(&mut bytes).unwrap();

                        binance::parse_kline_body(&bytes, (data_count + 1000) as usize)
                    }
                    Err(e) => match e.kind() {
                        std::io::ErrorKind::AlreadyExists => {
                            let mut f = std::fs::File::open(&path).unwrap();
                            let mut bytes = Vec::new();
                            f.read_to_end(&mut bytes).unwrap();

                            binance::parse_kline_body(&bytes, (data_count + 1000) as usize)
                        }
                        kind => panic!("Could not create file: {:?}", kind),
                    },
                }
            }
            Mode::Live => {
                let prev_time = prev_time - interval;
                let response = binance
                    .get_kline_data(
                        &self.symbol,
                        self.interval,
                        None,
                        Some(prev_time),
                        Some(1000),
                    )
                    .expect("Could not get kline data");
                binance::parse_kline_body(&response.bytes().unwrap(), 1000)
            }
        }
    }

    fn market_buy(&self, binance: &Account) {
        let quanity = format!("quoteOrderQty={}", self.quote);

        let parameters = format!(
            "symbol={}&side=BUY&type=MARKET&{}&timestamp={}",
            &self.symbol,
            quanity,
            Utc::now().timestamp_millis(),
        );

        binance
            .new_order(parameters)
            .expect("Could not send new order");
    }

    fn market_sell(&self, binance: &Account) {
        let quanity = format!("quantity={}", self.base);

        let parameters = format!(
            "symbol={}&side=SELL&type=MARKET&{}&timestamp={}",
            &self.symbol,
            quanity,
            Utc::now().timestamp_millis(),
        );

        binance
            .new_order(parameters)
            .expect("Could not send new order");
    }

    fn file_name(&self) -> String {
        format!(
            "./data/{}_{}_{}.txt",
            &self.symbol,
            self.start_time.naive_utc().date(),
            self.interval,
        )
    }

    fn backtest(&mut self, data: Klines) {
        let mut bb = BollingerBand::new(20, 2f64);
        let mut adx = Adx::new(14);

        let mut net: f64 = 0f64;
        let mut p_trade = 0;
        let mut n_trade = 0;

        let mut uptrend_buy_case = false;

        for index in 0..data.close.len() {
            let &high = data.high.get(index).unwrap();
            let &low = data.low.get(index).unwrap();
            let &close = data.close.get(index).unwrap();
            bb.next(close);

            if let Some(prev_index) = index.checked_sub(1) {
                let &high_prev = data.high.get(prev_index).unwrap();
                let &low_prev = data.low.get(prev_index).unwrap();
                let &close_prev = data.close.get(prev_index).unwrap();
                adx.next(high, high_prev, low, low_prev, close_prev);
            }

            if self.start_time.timestamp_millis()
                <= data.open_time.get(index).unwrap().timestamp_millis()
            {
                if let (
                    Some((mean, bb_upper, bb_lower)),
                    (Some(adx_val), Some(pdi_val), Some(mdi_val)),
                ) = (bb.get(), adx.get())
                {
                    if self.base.abs() < f64::EPSILON {
                        /*
                        let bound = if adx_val < 15f64 {
                            bb_lower
                        } else {
                            bb_lower - bb.dev().unwrap() / 2f64
                        };
                        */
                        let ult_lower = bb_lower - bb.dev().unwrap() / 2f64;
                        let bb_lower_condition = (adx_val < 15f64 && close < bb_lower)
                            || (adx_val < 25f64 && low < ult_lower);

                        let moving_average_condition = pdi_val > mdi_val
                            && adx_val > 25f64
                            && low > mean
                            && low < mean + bb.dev().unwrap() / 2f64;
                        uptrend_buy_case = moving_average_condition;

                        if bb_lower_condition || moving_average_condition {
                            self.base = self.quote / close;
                            net = self.quote;
                            self.quote = 0f64;
                            if moving_average_condition {
                                println!("TO THE MOON!");
                            }
                            println!(
                                "{}: Bought {:.4} amount coin    BB: {:<16.4}{:<16.4}{:<16.4}ADX: {:<16.4}+DI: {:<16.4}-DI: {:<16.4}PRICE: {:<16.4}",
                                data.open_time.get(index).unwrap(),
                                self.base,
                                mean,
                                bb_upper,
                                bb_lower,
                                adx_val,
                                pdi_val,
                                mdi_val,
                                close,
                            );
                        }
                    } else {
                        let conditional_net = ((self.base * close) / net) - 1f64;
                        let moving_average_condition =
                            close > mean && conditional_net.is_sign_positive() && !uptrend_buy_case;
                        let bb_upper_condition = close > bb_upper;

                        if moving_average_condition || bb_upper_condition {
                            self.quote = self.base * close;
                            net = ((self.quote / net) - 1f64) * 100f64;
                            println!(
                                "{}: Sold   {:.4} amount coin    BB: {:<16.4}{:<16.4}{:<16.4}ADX:{:<16.4}+DI: {:<16.4}-DI: {:<16.4}PRICE: {:<16.4}NET: {:<+16.4}\n",
                                data.open_time.get(index).unwrap(),
                                self.base,
                                mean,
                                bb_upper,
                                bb_lower,
                                adx_val,
                                pdi_val,
                                mdi_val,
                                close,
                                net
                            );
                            self.base = 0f64;

                            if net.is_sign_positive() {
                                p_trade += 1;
                            } else {
                                n_trade += 1;
                            }
                        }
                    }
                }
            }
        }
        println!("Positive trade: {}", p_trade);
        println!("Negative trade: {}", n_trade);
        println!("Result: {}", self.base * data.close.last().unwrap());
        println!("Result: {}", self.quote);
    }

    fn live(&mut self, binance: Account) {
        let data = self.get_required_data(&binance);
        let mut bb = BollingerBand::new(20, 2f64);
        let mut adx = Adx::new(14);

        let mut net = 0f64;
        let _uptrend_buy_case = false;

        for index in 0..data.close.len() {
            let &high = data.high.get(index).unwrap();
            let &low = data.low.get(index).unwrap();
            let &close = data.close.get(index).unwrap();
            bb.next(close);

            if let Some(prev_index) = index.checked_sub(1) {
                let &high_prev = data.high.get(prev_index).unwrap();
                let &low_prev = data.low.get(prev_index).unwrap();
                let &close_prev = data.close.get(prev_index).unwrap();
                adx.next(high, high_prev, low, low_prev, close_prev);
            }
        }

        let (tx, rx) = mpsc::channel::<()>();
        let mut close_time = self.start_time.timestamp_millis() - 1;
        let interval = self.interval.to_millis();

        let stdin = io::stdin();
        let handle = thread::spawn(move || {
            for b in stdin.bytes() {
                if b.unwrap() == b'q' {
                    tx.send(()).unwrap();
                    break;
                }
            }
        });

        let mut prev_kline = data.last();

        loop {
            let timeout = std::time::Duration::from_millis(
                (close_time - Utc::now().timestamp_millis()) as u64,
            );
            match rx.recv_timeout(timeout) {
                Ok(_) => {
                    println!("Exiting");
                    handle.join().unwrap();
                    break;
                }
                Err(RecvTimeoutError::Timeout) => {
                    let response = binance
                        .get_kline_data(
                            &self.symbol,
                            self.interval,
                            None,
                            Some(close_time),
                            Some(1),
                        )
                        .expect("Could not get kline data");

                    let kline = Kline::from_bytes(&response.bytes().unwrap());

                    let open_time = kline.time();
                    let high = kline.high();
                    let prev_high = prev_kline.high();
                    let low = kline.low();
                    let prev_low = prev_kline.low();
                    let close = kline.close();
                    let prev_close = prev_kline.close();

                    bb.next(close);
                    adx.next(high, prev_high, low, prev_low, prev_close);

                    if let (
                        Some((mean, bb_upper, bb_lower)),
                        (Some(adx_val), Some(pdi_val), Some(mdi_val)),
                    ) = (bb.get(), adx.get())
                    {
                        println!(
                            "[TRACE {}] Checking conditions. BB: {:<16.4}{:<16.4}{:<16.4}ADX: {:<16.4}+DI: {:<16.4}-DI: {:<16.4}CLOSE: {:<16.4}",
                            open_time,
                            mean,
                            bb_upper,
                            bb_lower,
                            adx_val,
                            pdi_val,
                            mdi_val,
                            close,
                        );
                        if self.base.abs() < f64::EPSILON {
                            let bound = if adx_val < 15f64 {
                                bb_lower
                            } else {
                                bb_lower - bb.dev().unwrap() / 2f64
                            };
                            let bb_lower_condition = bound > close;
                            let moving_average_condition = pdi_val > mdi_val
                                && adx_val > 25f64
                                && low > mean
                                && low < mean + bb.dev().unwrap() / 2f64;
                            // uptrend_buy_case = moving_average_condition;

                            if bb_lower_condition || moving_average_condition {
                                self.market_buy(&binance);
                                self.base = Self::get_balance(&binance, self.symbol.base());
                                self.base -= self.base % 10f64.powi(-(self.precision as i32));
                                net = self.quote;
                                self.quote = 0f64;

                                println!(
                                    "\n[BUY  {}] BB: {:<16.4}{:<16.4}{:<16.4}ADX: {:<16.4} +DI: {:<16.4}-DI: {:<16.4}PRICE: {:<16.4}",
                                    open_time,
                                    mean,
                                    bb_upper,
                                    bb_lower,
                                    adx_val,
                                    pdi_val,
                                    mdi_val,
                                    close,
                                );
                            }
                        } else {
                            let _conditional_net = ((self.base * close) / net) - 1f64;
                            let moving_average_condition = close > mean;
                            let bb_upper_condition = close > bb_upper;

                            if bb_upper_condition {
                                self.market_sell(&binance);
                                self.quote = Self::get_balance(&binance, self.symbol.quote());
                                net = ((self.quote / net) - 1f64) * 100f64;
                                self.base = 0f64;

                                println!(
                                    "[SELL {}] BB: {:<16.4}{:<16.4}{:<16.4}ADX: {:<16.4} +DI: {:<16.4}-DI: {:<16.4}PRICE: {:<16.4}NET: {:<16.4}\n",
                                    open_time,
                                    mean,
                                    bb_upper,
                                    bb_lower,
                                    adx_val,
                                    pdi_val,
                                    mdi_val,
                                    close,
                                    net,
                                );
                            }
                        }
                    }
                    prev_kline = kline;
                    close_time += interval;
                }
                Err(e) => panic!("{}", e),
            }
        }
    }

    pub fn run(&mut self, binance: Account) {
        match self.mode {
            Mode::Backtest => {
                let data = self.get_required_data(&binance);
                self.backtest(data);
            }
            Mode::Live => self.live(binance),
        }
    }
}

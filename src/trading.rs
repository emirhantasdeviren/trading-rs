use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, FixedOffset, TimeZone, Utc};

use crate::exchange::binance::{Account, Asset, SymbolString};
use crate::exchange::{Interval, Kline};
use crate::indicators::{BollingerBand, Dema, Dmi, TdSeq};
use crate::parser::TomlParser;
use crate::telegram;

pub struct Trader {
    binance: Account,
    telegram: telegram::Bot,
    start_time: DateTime<Utc>,
    symbols: Vec<Symbol>,
    assets: Vec<Asset>,
    interval: Interval,
}

impl Trader {
    pub fn new(binance: Account, interval: Interval) -> Self {
        let start_time = {
            let now = Utc::now().timestamp_millis();
            let interval = interval.to_millis();
            Utc.timestamp_millis(now + interval - now % interval)
        };

        let mut f = File::open("cache.toml").unwrap();
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).unwrap();
        let mut parser = TomlParser::new(&buf);
        let (assets, symbols) = parser.load_cache();

        println!("[INFO] Start Time: {}", start_time);
        print!("[INFO] Symbols: ");

        for (i, symbol) in symbols.iter().enumerate() {
            if i < symbols.len() - 1 {
                print!("{}, ", symbol.as_str());
            } else {
                println!("{}\n", symbol.as_str());
            }
        }

        println!("[INFO] Assets:");
        for asset in assets.iter() {
            println!("    {}: {:.8}", &asset.name, asset.balance);
        }

        println!("\n[INFO] Interval: {}", interval);

        Self {
            binance,
            telegram: telegram::Bot::new(),
            start_time,
            symbols,
            assets,
            interval,
        }
    }

    pub fn run(&mut self) {
        let data = self.get_required_data();

        for (i, klines) in data.into_iter().enumerate() {
            let mut prev_kline = klines.first().unwrap();

            for kline in klines.iter().skip(1) {
                self.symbols[i].indicators.update(kline, prev_kline);
                self.symbols[i].kline.update(kline);

                prev_kline = kline;
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

        loop {
            let timeout = std::time::Duration::from_millis(
                (close_time
                    - SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .expect("system time before Unix epoch")
                        .as_millis() as i64) as u64,
            );
            match rx.recv_timeout(timeout) {
                Ok(_) => {
                    println!("Exiting");
                    handle.join().unwrap();
                    break;
                }
                Err(RecvTimeoutError::Timeout) => {
                    for symbol in self.symbols.iter_mut() {
                        let response = self
                            .binance
                            .get_kline_data(
                                symbol.as_str(),
                                self.interval,
                                None,
                                Some(close_time),
                                Some(1),
                            )
                            .expect("Could not get kline data");

                        let kline = Kline::parse_array(&response.bytes().unwrap());

                        symbol.indicators.update(&kline, &symbol.kline);
                        symbol.kline.update(&kline);
                    }

                    let mut signals: Vec<Option<Signal>> = Vec::with_capacity(self.symbols.len());
                    for symbol in self.symbols.iter() {
                        let signal = symbol.check_conditions();
                        if let Some(s) = signal {
                            match s {
                                Signal::Buy(_) => {
                                    let msg = format!(
                                        "[{}] {} Buy Signal",
                                        FixedOffset::east(3)
                                            .timestamp_millis(symbol.kline.open_time),
                                        symbol.as_str()
                                    );
                                    self.telegram.send_message(&msg);
                                }
                                Signal::Sell => {
                                    let msg = format!(
                                        "[{}] {} Sell Signal",
                                        FixedOffset::east(3)
                                            .timestamp_millis(symbol.kline.open_time),
                                        symbol.as_str()
                                    );
                                    self.telegram.send_message(&msg);
                                }
                            }
                        }
                        signals.push(signal);
                    }

                    for (i, signal) in signals.into_iter().enumerate() {
                        if let Some(Signal::Buy(pos)) = signal {
                            self.buy(i, pos);
                        } else if let Some(Signal::Sell) = signal {
                            self.sell(i);
                        }
                    }

                    close_time += interval;
                }
                Err(e) => panic!("{}", e),
            }
        }
    }

    fn get_required_data(&self) -> Vec<Vec<Kline>> {
        let interval: i64 = self.interval.to_millis();
        let prev_time = self.start_time.timestamp_millis() - 2 * interval;
        let mut klines_for_each_symbol = Vec::with_capacity(self.symbols.len());

        for symbol in self.symbols.iter() {
            let response = self
                .binance
                .get_kline_data(
                    symbol.as_str(),
                    self.interval,
                    None,
                    Some(prev_time),
                    Some(1000),
                )
                .expect("Could not get kline data");

            klines_for_each_symbol.push(Kline::parse_2d_array(&response.bytes().unwrap(), 1000));
        }

        klines_for_each_symbol
    }

    fn buy(&mut self, symbol_index: usize, pos: Position) {
        let close_position_count = self
            .symbols
            .iter()
            .filter(|symbol| symbol.position.is_none())
            .count();
        let symbol = self.symbols.get_mut(symbol_index).unwrap();
        let mut quote = None;
        let mut base = None;

        for asset in self.assets.iter_mut() {
            if symbol.quote() == &asset.name {
                quote = Some(asset);
            } else if symbol.base() == &asset.name {
                base = Some(asset);
            }
        }

        let quote = quote.unwrap();
        let base = base.unwrap();
        let quote_order_quantity = quote.balance / (close_position_count as f64);

        if quote_order_quantity > 10f64 {
            self.binance
                .market_buy(symbol.as_str(), quote_order_quantity)
                .expect("Could not buy the coin");

            println!(
                "[{}] Bought {} with {} {}",
                Utc.timestamp_millis(symbol.kline.open_time),
                symbol.base(),
                quote_order_quantity,
                symbol.quote(),
            );

            symbol.net = symbol.kline.close;
            symbol.position = Some(pos);

            base.balance = self
                .binance
                .get_balance(symbol.base())
                .expect("Could not get balance");
            quote.balance = self
                .binance
                .get_balance(symbol.quote())
                .expect("Could not get balance");
        } else {
            println!(
                "[{}] {} MIN_NOTIONAL Filter: {} < 10",
                Utc.timestamp_millis(symbol.kline.open_time),
                symbol.as_str(),
                quote_order_quantity,
            );
        }
    }

    fn sell(&mut self, symbol_index: usize) {
        let symbol = self.symbols.get_mut(symbol_index).unwrap();
        let mut quote = None;
        let mut base = None;

        for asset in self.assets.iter_mut() {
            if symbol.quote() == &asset.name {
                quote = Some(asset);
            } else if symbol.base() == &asset.name {
                base = Some(asset);
            }
        }

        let quote = quote.unwrap();
        let base = base.unwrap();

        base.balance -= base.balance % 10f64.powi(-symbol.step_size);

        self.binance
            .market_sell(symbol.as_str(), base.balance)
            .expect("Could not sell the coin");

        println!(
            "[{}] Sold {} {} NET: {:.1}%",
            Utc.timestamp_millis(symbol.kline.open_time),
            base.balance,
            symbol.base(),
            (symbol.kline.close / symbol.net - 1f64) * 100f64,
        );

        symbol.net = 0f64;
        symbol.position = None;

        quote.balance = self
            .binance
            .get_balance(symbol.quote())
            .expect("Could not get balance");
        base.balance = 0f64;
    }
}

pub struct Backtester {
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    symbol: Symbol,
    base: Asset,
    quote: Asset,
    interval: Interval,
    net: f64,
}

impl Backtester {
    pub fn new(
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        symbol: Symbol,
        interval: Interval,
    ) -> Self {
        let base = Asset::new(symbol.base(), 0f64);
        let quote = Asset::new(symbol.quote(), 100f64);

        println!("[INFO] Symbol: {}\n", symbol.as_str());
        println!("[INFO] Start Time: {}", start_time);
        println!("[INFO] End Time: {}\n", end_time);
        println!(
            "[INFO] {} Balance: {}\n       {} Balance: {}",
            &base.name, base.balance, &quote.name, quote.balance
        );
        println!("[INFO] Interval: {}\n", interval);

        Self {
            start_time,
            end_time,
            symbol,
            base,
            quote,
            interval,
            net: 0f64,
        }
    }

    pub fn run(&mut self, binance: Account) {
        let klines = self.get_required_data(&binance);
        let mut prev_kline = klines.first().unwrap();

        for kline in klines.iter().skip(1) {
            self.symbol.kline.update(kline);
            self.symbol.indicators.update(kline, prev_kline);

            if self.start_time.timestamp_millis() <= kline.open_time {
                match self.symbol.check_conditions() {
                    Some(Signal::Buy(pos)) => {
                        self.buy();
                        self.symbol.net = self.symbol.kline.close;
                        self.symbol.position = Some(pos);
                    }
                    Some(Signal::Sell) => {
                        self.sell();
                        self.symbol.position = None;
                    }
                    None => (),
                }
            }

            prev_kline = kline;
        }

        if self.symbol.position.is_none() {
            println!("ROI: {:.1}%", (self.quote.balance / 100f64 - 1f64) * 100f64);
        } else {
            println!(
                "ROI: {:.1}%",
                (self.base.balance * klines.last().unwrap().close / 100f64 - 1f64) * 100f64
            );
        }
    }

    fn get_required_data(&self, binance: &Account) -> Vec<Kline> {
        let interval: i64 = self.interval.to_millis();
        let prev_time = self.start_time.timestamp_millis() - interval;

        let data_count =
            (self.end_time.timestamp_millis() - self.start_time.timestamp_millis()) / interval;

        let iteration = if data_count % 1000 == 0 {
            data_count / 1000
        } else {
            data_count / 1000 + 1
        };

        let path = format!(
            "./data/{}_{}_{}.txt",
            self.symbol.as_str(),
            self.start_time.naive_utc().date(),
            self.interval
        );

        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                let response = binance
                    .get_kline_data(
                        self.symbol.as_str(),
                        self.interval,
                        None,
                        Some(prev_time),
                        Some(1000),
                    )
                    .expect("Could not get kline data");
                f.write_all(&response.bytes().unwrap()).unwrap();

                for i in 0..iteration {
                    let start_time = self.start_time.timestamp_millis() + (i * interval * 1000);
                    let response = binance
                        .get_kline_data(
                            self.symbol.as_str(),
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

                Kline::parse_2d_array(&bytes, (data_count + 1000) as usize)
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::AlreadyExists => {
                    let mut f = std::fs::File::open(&path).unwrap();
                    let mut bytes = Vec::new();
                    f.read_to_end(&mut bytes).unwrap();

                    Kline::parse_2d_array(&bytes, (data_count + 1000) as usize)
                }
                _ => panic!("{:?}", e),
            },
        }
    }

    fn buy(&mut self) {
        self.net = self.symbol.kline.close;
        self.base.balance = self.quote.balance / self.symbol.kline.close;
        self.quote.balance = 0f64;
        println!(
            "[INFO] BUY  {}: PRICE: {:.4}",
            Utc.timestamp_millis(self.symbol.kline.open_time),
            self.symbol.kline.close
        );
    }

    fn sell(&mut self) {
        self.net = (self.symbol.kline.close / self.net - 1f64) * 100f64;
        self.quote.balance = self.base.balance * self.symbol.kline.close;
        self.base.balance = 0f64;
        println!(
            "[INFO] SELL {}: PRICE: {:.4}    NET: {:.4}\n",
            Utc.timestamp_millis(self.symbol.kline.open_time),
            self.symbol.kline.close,
            self.net,
        );
    }
}

pub struct Symbol {
    name: SymbolString,
    indicators: Indicators,
    kline: Kline,
    step_size: i32,
    position: Option<Position>,
    net: f64,
}

impl Symbol {
    pub fn new(base: &str, quote: &str) -> Self {
        Self {
            name: SymbolString::new(base, quote),
            indicators: Indicators::default(),
            kline: Kline::default(),
            step_size: 8,
            position: Option::default(),
            net: 0f64,
        }
    }

    pub fn from_string(inner: String, mid: usize, step_size: i32) -> Self {
        Self {
            name: SymbolString::from_raw_parts(inner, mid),
            indicators: Indicators::default(),
            kline: Kline::default(),
            step_size,
            position: Option::default(),
            net: 0f64,
        }
    }

    fn check_conditions(&self) -> Option<Signal> {
        if let (Some((basis, upper, lower)), (Some(adx), Some(pdi), Some(mdi)), Some(dema)) = (
            self.indicators.bb.get(),
            self.indicators.dmi.get(),
            self.indicators.dema.get(),
        ) {
            match self.position {
                None => {
                    let bound = if adx > 15f64 {
                        lower - self.indicators.bb.dev().unwrap() / 2f64
                    } else {
                        lower
                    };

                    let buy_the_dip = self.kline.close < bound;

                    let to_the_moon = pdi > mdi
                        && adx > 25f64
                        && adx < 40f64
                        && adx > dema
                        && self.kline.low > basis
                        && self.kline.low < basis + self.indicators.bb.dev().unwrap() / 2f64;

                    if buy_the_dip {
                        Some(Signal::Buy(Position::Dip))
                    } else if to_the_moon {
                        Some(Signal::Buy(Position::Mean))
                    } else {
                        None
                    }
                }
                Some(Position::Dip) => {
                    let conditional_net = self.kline.close / self.net - 1f64;

                    if (conditional_net.is_sign_positive() && self.kline.close > basis)
                        || self.kline.close > upper
                    {
                        Some(Signal::Sell)
                    } else {
                        None
                    }
                }
                Some(Position::Mean) => {
                    let at_the_moon =
                        pdi > mdi && adx > 25f64 && dema > adx && self.indicators.was_perfect;

                    if at_the_moon {
                        Some(Signal::Sell)
                    } else {
                        None
                    }
                }
            }
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &str {
        self.name.as_str()
    }

    pub fn base(&self) -> &str {
        self.name.base()
    }

    pub fn quote(&self) -> &str {
        self.name.quote()
    }
}

struct Indicators {
    dmi: Dmi,
    bb: BollingerBand<20>,
    dema: Dema,
    td_seq: TdSeq,
    was_perfect: bool,
}

impl Default for Indicators {
    fn default() -> Self {
        Self {
            dmi: Dmi::new(14),
            bb: BollingerBand::new(2f64),
            dema: Dema::new(9),
            td_seq: TdSeq::new(),
            was_perfect: false,
        }
    }
}

impl Indicators {
    pub fn update(&mut self, kline: &Kline, prev_kline: &Kline) {
        self.dmi.next(
            kline.high,
            prev_kline.high,
            kline.low,
            prev_kline.low,
            prev_kline.close,
        );
        self.bb.next(kline.close);

        if let Some(adx) = self.dmi.get().0 {
            self.dema.next(adx);
        }

        self.td_seq.next(kline.high, kline.low, kline.close);

        if let ((Some(adx), Some(pdi), Some(mdi)), Some(dema)) = (self.dmi.get(), self.dema.get()) {
            if self.td_seq.sell_perfect() && adx > dema && pdi > mdi && adx > 25f64 {
                self.was_perfect = true;
            }

            if self.was_perfect && adx < 25f64 {
                self.was_perfect = false;
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Signal {
    Buy(Position),
    Sell,
}

#[derive(Clone, Copy)]
enum Position {
    Dip,
    Mean,
}

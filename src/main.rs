use chrono::{TimeZone, Utc};
use trading_rs::exchange::binance::{Account, Interval, Symbol};
use trading_rs::trading::{Mode, Trader};

fn main() {
    // let start_time = Utc.ymd(2018, 1, 1).and_hms(0, 0, 0);
    // let end_time = Utc.ymd(2021, 3, 24).and_hms(0, 0, 0);
    let start_time = Utc.ymd(2021, 1, 15).and_hms(21, 0, 0);
    let end_time = Utc.ymd(2021, 3, 30).and_hms(0, 0, 0);

    let binance = Account::new();
    let mut trader = Trader::new(
        &binance,
        start_time,
        end_time,
        Symbol::new("AVAX", "USDT"),
        Interval::Hour(1),
        Mode::Backtest,
        100f64,
    );

    trader.run(binance);
}

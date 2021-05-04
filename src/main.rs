use chrono::{TimeZone, Utc};
use trading_rs::exchange::binance::Account;
use trading_rs::exchange::Interval;
use trading_rs::trading::Trader;

fn main() {
    let _start_time = Utc.ymd(2021, 4, 29).and_hms(9, 0, 0);
    let _end_time = Utc.ymd(2021, 5, 4).and_hms(9, 0, 0);

    let binance = Account::new();

    let mut trader = Trader::new(binance, Interval::Hour(1));
    trader.run();

    /*
    let mut backtester = Backtester::new(
        start_time,
        end_time,
        Symbol::new("DOGE", "USDT"),
        Interval::Hour(1),
    );
    backtester.run(binance);
    */
}

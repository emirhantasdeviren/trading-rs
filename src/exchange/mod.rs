pub mod binance;

use std::fmt;

#[derive(Clone, Copy)]
pub enum Interval {
    Minute(i64),
    Hour(i64),
    Day(i64),
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
            Interval::Minute(m) => *m * 60 * 1000,
            Interval::Hour(h) => *h * 60 * 60 * 1000,
            Interval::Day(d) => *d * 24 * 60 * 60 * 1000,
            Interval::Week => 7 * 24 * 60 * 60 * 1000,
            Interval::Month => 30 * 60 * 60 * 1000,
        }
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Kline {
    pub open_time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

impl Kline {
    pub fn update(&mut self, kline: &Self) {
        self.open_time = kline.open_time;
        self.open = kline.open;
        self.high = kline.high;
        self.low = kline.low;
        self.close = kline.close;
    }

    pub fn parse_2d_array(slice: &[u8], capacity: usize) -> Vec<Self> {
        let mut i: Option<usize> = None;
        let mut klines = Vec::with_capacity(capacity);

        for (j, &item) in slice.iter().enumerate().skip(1) {
            if i.is_none() && item == b'[' {
                i = Some(j);
            }

            if i.is_some() && item == b']' {
                klines.push(Self::parse_array(&slice[i.unwrap()..j]));
                i = None;
            }
        }

        klines
    }

    pub fn parse_array(slice: &[u8]) -> Self {
        let mut i: Option<usize> = None;
        let mut index: usize = 0;

        let mut kline = Self::default();

        for (count, &element) in slice.iter().enumerate() {
            if element.is_ascii_digit() && i.is_none() {
                i = Some(count);
            }

            if i.is_some() && (element == b'"' || element == b',') {
                let s = std::str::from_utf8(&slice[i.unwrap()..count]).unwrap();

                match index {
                    0 => kline.open_time = s.parse().unwrap(),
                    1 => kline.open = s.parse().unwrap(),
                    2 => kline.high = s.parse().unwrap(),
                    3 => kline.low = s.parse().unwrap(),
                    4 => {
                        kline.close = s.parse().unwrap();
                        break;
                    }
                    _ => (),
                }

                index += 1;
                i = None;
            }
        }

        kline
    }
}

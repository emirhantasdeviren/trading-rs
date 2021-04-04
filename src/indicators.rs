pub struct Sma {
    period: usize,
    data: Box<[f64]>,
    index: usize,
    value: Option<f64>,
}

impl Sma {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            data: vec![f64::NAN; period].into_boxed_slice(),
            index: 0,
            value: None,
        }
    }

    pub fn next(&mut self, value: f64) {
        if self.data[self.index].is_nan() {
            self.data[self.index] = value;

            if self.index < self.period - 1 {
                self.index += 1;
            } else {
                self.value = Some(self.data.iter().sum::<f64>() / self.period as f64);
                self.index = 0;
            }
        } else {
            self.data[self.index] = value;
            self.value = Some(self.data.iter().sum::<f64>() / self.period as f64);

            if self.index < self.period - 1 {
                self.index += 1;
            } else {
                self.index = 0;
            }
        }
    }

    pub fn get(&self) -> Option<f64> {
        self.value
    }
}

pub struct StandardDeviation {
    period: usize,
    index: usize,
    data: Box<[f64]>,
    value: Option<f64>,
}

impl StandardDeviation {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            index: 0,
            data: vec![f64::NAN; period].into_boxed_slice(),
            value: None,
        }
    }

    pub fn next(&mut self, value: f64) {
        if self.data[self.index].is_nan() {
            self.data[self.index] = value;

            if self.index < self.period - 1 {
                self.index += 1;
            } else {
                self.index = 0;

                let mean = self.data.iter().sum::<f64>() / self.period as f64;
                self.value = Some(
                    (self.data.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
                        / self.period as f64)
                        .sqrt(),
                );
            }
        } else {
            self.data[self.index] = value;

            if self.index < self.period - 1 {
                self.index += 1;
            } else if self.index == self.period - 1 {
                self.index = 0;
            }

            let mean = self.data.iter().sum::<f64>() / self.period as f64;
            self.value = Some(
                (self.data.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / self.period as f64)
                    .sqrt(),
            );
        }
    }

    pub fn get(&self) -> Option<f64> {
        self.value
    }
}

pub struct Ema {
    period: usize,
    index: usize,
    value: Option<f64>,
    current: f64,
    alpha: f64,
}

impl Ema {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            index: 0,
            value: None,
            current: 0f64,
            alpha: 2f64 / (period + 1) as f64,
        }
    }

    pub fn new_with_constant(period: usize, alpha: f64) -> Self {
        Self {
            period,
            index: 0,
            value: None,
            current: 0f64,
            alpha,
        }
    }

    pub fn next(&mut self, price: f64) {
        if self.index < self.period - 1 {
            self.current += price;
            self.index += 1;
        } else if self.index == self.period - 1 {
            self.current += price;
            self.current /= self.period as f64;
            self.value = Some(self.current);
            self.index += 1;
        } else {
            self.current += self.alpha * (price - self.current);
            self.value = Some(self.current);
        }
    }

    pub fn get(&self) -> Option<f64> {
        self.value
    }
}

pub struct Macd {
    fast: Ema,
    slow: Ema,
    signal: Ema,
}

impl Macd {
    pub fn new(fast: usize, slow: usize, signal: usize) -> Self {
        Self {
            fast: Ema::new(fast),
            slow: Ema::new(slow),
            signal: Ema::new(signal),
        }
    }

    pub fn next(&mut self, price: f64) {
        self.fast.next(price);
        self.slow.next(price);

        match (self.fast.get(), self.slow.get()) {
            (Some(fast), Some(slow)) => self.signal.next(fast - slow),
            _ => (),
        }
    }

    pub fn get(&self) -> (Option<f64>, Option<f64>, Option<f64>) {
        match (self.fast.get(), self.slow.get(), self.signal.get()) {
            (Some(fast), Some(slow), Some(signal)) => (
                Some(fast - slow),
                Some(signal),
                Some((fast - slow) - signal),
            ),
            (Some(fast), Some(slow), None) => (Some(fast - slow), None, None),
            _ => (None, None, None),
        }
    }

    pub fn is_positive(&self) -> Option<bool> {
        match (self.fast.get(), self.slow.get(), self.signal.get()) {
            (Some(fast), Some(slow), Some(signal)) => {
                Some(((fast - slow) - signal).is_sign_positive())
            }
            _ => None,
        }
    }
}

pub struct Atr {
    period: usize,
    value: Option<f64>,
    current: f64,
    index: usize,
}

impl Atr {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            value: None,
            current: 0f64,
            index: 0,
        }
    }

    pub fn next(&mut self, high: f64, close_prev: f64, low: f64) {
        let true_range = high.max(close_prev) - low.min(close_prev);

        if self.index < self.period - 1 {
            self.current += true_range;
            self.index += 1;
        } else if self.index == self.period - 1 {
            self.current += true_range;
            self.current /= self.period as f64;
            self.value = Some(self.current);
            self.index += 1;
        } else {
            self.current =
                (self.current * (self.period - 1) as f64 + true_range) / self.period as f64;
            self.value = Some(self.current);
        }
    }

    pub fn get(&self) -> Option<f64> {
        self.value
    }
}

pub struct Adx {
    spdm: Ema,
    smdm: Ema,
    dx: Ema,
    atr: Atr,
    value: Option<f64>,
}

impl Adx {
    pub fn new(period: usize) -> Self {
        Self {
            spdm: Ema::new_with_constant(period, 1f64 / period as f64),
            smdm: Ema::new_with_constant(period, 1f64 / period as f64),
            dx: Ema::new_with_constant(period, 1f64 / period as f64),
            atr: Atr::new(period),
            value: None,
        }
    }

    pub fn next(&mut self, high: f64, high_prev: f64, low: f64, low_prev: f64, close_prev: f64) {
        let up_move = high - high_prev;
        let down_move = low_prev - low;

        let pdm = if up_move > down_move && up_move.is_sign_positive() {
            up_move
        } else {
            0f64
        };

        let mdm = if up_move < down_move && down_move.is_sign_positive() {
            down_move
        } else {
            0f64
        };

        self.spdm.next(pdm);
        self.smdm.next(mdm);
        self.atr.next(high, close_prev, low);

        if let (Some(pdm), Some(mdm), Some(atr)) =
            (self.spdm.get(), self.smdm.get(), self.atr.get())
        {
            let pdi = pdm / atr * 100f64;
            let mdi = mdm / atr * 100f64;
            self.dx.next(((pdi - mdi) / (pdi + mdi)).abs() * 100f64);
        }

        if let Some(dx) = self.dx.get() {
            self.value = Some(dx);
        }
    }

    pub fn get(&self) -> (Option<f64>, Option<f64>, Option<f64>) {
        let (pdi, mdi) = if let (Some(val), Some(val2), Some(atr)) =
            (self.spdm.get(), self.smdm.get(), self.atr.get())
        {
            (Some(val / atr * 100f64), Some(val2 / atr * 100f64))
        } else {
            (None, None)
        };
        (self.value, pdi, mdi)
    }
}

pub struct Rsi {
    smoothed_upward_change: Ema,
    smoothed_downward_change: Ema,
    value: Option<f64>,
}

impl Rsi {
    pub fn new(period: usize) -> Self {
        Self {
            smoothed_upward_change: Ema::new_with_constant(period, 1f64 / period as f64),
            smoothed_downward_change: Ema::new_with_constant(period, 1f64 / period as f64),
            value: None,
        }
    }

    pub fn next(&mut self, close_now: f64, close_prev: f64) {
        let (upward_change, downward_change) =
            if close_now - close_prev < f64::EPSILON && close_now - close_prev > -f64::EPSILON {
                (0f64, 0f64)
            } else if close_now > close_prev {
                (close_now - close_prev, 0f64)
            } else {
                (0f64, close_prev - close_now)
            };

        self.smoothed_upward_change.next(upward_change);
        self.smoothed_downward_change.next(downward_change);

        if let (Some(smmau), Some(smmad)) = (
            self.smoothed_upward_change.get(),
            self.smoothed_downward_change.get(),
        ) {
            let rs = smmau / smmad;
            self.value = Some(100f64 - 100f64 / (1f64 + rs));
        }
    }

    pub fn get(&self) -> Option<f64> {
        self.value
    }
}

pub struct StochRsi {
    rsi: Rsi,
    maximum: Maximum,
    minimum: Minimum,
    value: Sma,
}

impl StochRsi {
    pub fn new(period: usize) -> Self {
        Self {
            rsi: Rsi::new(period),
            maximum: Maximum::new(period),
            minimum: Minimum::new(period),
            value: Sma::new(3),
        }
    }

    pub fn next(&mut self, close: f64, close_prev: f64) {
        self.rsi.next(close, close_prev);
        if let Some(rsi) = self.rsi.get() {
            self.maximum.next(rsi);
            self.minimum.next(rsi);

            if let (Some(max), Some(min)) = (self.maximum.get(), self.minimum.get()) {
                self.value.next(100f64 * (rsi - min) / (max - min));
            }
        }
    }

    pub fn get(&self) -> Option<f64> {
        self.value.get()
    }
}

pub struct Maximum {
    period: usize,
    max_index: usize,
    cur_index: usize,
    values: Box<[f64]>,
}

impl Maximum {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            max_index: 0,
            cur_index: 0,
            values: vec![f64::MIN; period].into_boxed_slice(),
        }
    }

    fn find_max_index(&self) -> usize {
        let mut max = f64::MIN;
        let mut index: usize = 0;

        for (i, &val) in self.values.iter().enumerate() {
            if max < val {
                max = val;
                index = i;
            }
        }

        index
    }

    pub fn next(&mut self, price: f64) {
        self.values[self.cur_index] = price;

        if price > self.values[self.max_index] {
            self.max_index = self.cur_index;
        } else if self.max_index == self.cur_index {
            self.max_index = self.find_max_index();
        }

        self.cur_index = if self.cur_index + 1 < self.period {
            self.cur_index + 1
        } else {
            0
        };
    }

    pub fn get(&self) -> Option<f64> {
        self.values.get(self.max_index).copied()
    }
}

pub struct Minimum {
    period: usize,
    min_index: usize,
    cur_index: usize,
    values: Box<[f64]>,
}

impl Minimum {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            min_index: 0,
            cur_index: 0,
            values: vec![f64::MAX; period].into_boxed_slice(),
        }
    }

    fn find_max_index(&self) -> usize {
        let mut min = f64::MAX;
        let mut index: usize = 0;

        for (i, &val) in self.values.iter().enumerate() {
            if min > val {
                min = val;
                index = i;
            }
        }

        index
    }

    pub fn next(&mut self, price: f64) {
        self.values[self.cur_index] = price;

        if price < self.values[self.min_index] {
            self.min_index = self.cur_index;
        } else if self.min_index == self.cur_index {
            self.min_index = self.find_max_index();
        }

        self.cur_index = if self.cur_index + 1 < self.period {
            self.cur_index + 1
        } else {
            0
        };
    }

    pub fn get(&self) -> Option<f64> {
        self.values.get(self.min_index).copied()
    }
}

pub struct BollingerBand {
    typical_price: Sma,
    dev: StandardDeviation,
    m: f64,
    value: Option<(f64, f64, f64)>,
}

impl BollingerBand {
    pub fn new(period: usize, m: f64) -> Self {
        Self {
            typical_price: Sma::new(period),
            dev: StandardDeviation::new(period),
            m,
            value: None,
        }
    }

    // pub fn next(&mut self, high: f64, low: f64, close: f64) {
    pub fn next(&mut self, close: f64) {
        // let value: f64 = (high + low + close) / 3f64;
        self.typical_price.next(close);
        self.dev.next(close);

        if let (Some(mean), Some(deviation)) = (self.typical_price.get(), self.dev.get()) {
            let upper_band = mean + self.m * deviation;
            let lower_band = mean - self.m * deviation;
            self.value = Some((mean, upper_band, lower_band));
        }
    }

    pub fn dev(&self) -> Option<f64> {
        self.dev.get().map(|dev| dev * self.m)
    }

    pub fn get(&self) -> Option<(f64, f64, f64)> {
        self.value
    }
}

pub struct TdSeq {
    highs: [f64; 5],
    lows: [f64; 5],
    closes: [f64; 5],
    index: usize,
    buy_setup_count: u32,
    sell_setup_count: u32,
    perfect: bool,
    pub support: f64,
    pub resistance: f64,
    _buy_stop: f64,
    _sell_stop: f64,
}

impl TdSeq {
    pub fn new() -> Self {
        Self {
            highs: [f64::NAN; 5],
            lows: [f64::NAN; 5],
            closes: [f64::NAN; 5],
            index: 0,
            buy_setup_count: 0,
            sell_setup_count: 0,
            perfect: false,
            support: 0f64,
            resistance: 0f64,
            _buy_stop: 0f64,
            _sell_stop: 0f64,
        }
    }

    pub fn next(&mut self, high: f64, low: f64, close: f64) {
        if self.closes[self.index].is_nan() && self.index != 4 {
            self.closes[self.index] = close;
            self.highs[self.index] = high;
            self.lows[self.index] = low;
            self.index += 1;
        } else {
            self.closes[self.index] = close;
            self.highs[self.index] = high;
            self.lows[self.index] = low;

            let prev_index = if let Some(prev_index) = self.index.checked_sub(4) {
                prev_index
            } else {
                self.index + 1
            };

            if self.buy_setup_count == 9 {
                self.buy_setup_count = 0;
            }

            if self.sell_setup_count == 9 {
                self.sell_setup_count = 0;
            }

            if self.closes[prev_index] > self.closes[self.index] {
                if self.sell_setup_count != 0 {
                    self.sell_setup_count = 0;
                }
                if self.buy_setup_count == 0 {
                    self.resistance = self.highs[self.index];
                } else {
                    self.resistance.max(self.highs[self.index]);
                }
                self.buy_setup_count += 1;
            } else {
                if self.buy_setup_count != 0 {
                    self.buy_setup_count = 0;
                }
                if self.sell_setup_count == 0 {
                    self.support = self.lows[self.index];
                } else {
                    self.support.min(self.lows[self.index]);
                }
                self.sell_setup_count += 1;
            }

            if self.buy_setup_count == 9 {
                let sixth = self.lows[(prev_index + 1) % 5];
                let seventh = self.lows[(prev_index + 2) % 5];
                let eighth = self.lows[(prev_index + 3) % 5];
                let ninth = self.lows[self.index];

                self.perfect =
                    (eighth < sixth && eighth < seventh) || (ninth < sixth && ninth < seventh);
            }

            if self.sell_setup_count == 9 {
                let sixth = self.highs[(prev_index + 1) % 5];
                let seventh = self.highs[(prev_index + 2) % 5];
                let eighth = self.highs[(prev_index + 3) % 5];
                let ninth = self.highs[self.index];

                self.perfect =
                    (eighth > sixth && eighth > seventh) || (ninth > sixth && ninth > seventh);
            }

            if self.index == 4 {
                self.index = 0;
            } else {
                self.index += 1;
            }
        }
    }

    pub fn buy_perfect(&self) -> bool {
        self.perfect && self.buy_setup_count == 9
    }

    pub fn sell_perfect(&self) -> bool {
        self.perfect && self.sell_setup_count == 9
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sma_test() {
        let mut sma = Sma::new(3);

        assert_eq!(None, sma.get());
        sma.next(1f64);
        assert_eq!(None, sma.get());
        sma.next(2f64);
        assert_eq!(None, sma.get());
        sma.next(12f64);
        assert!(
            sma.get().unwrap() - 5f64 < f64::EPSILON && sma.get().unwrap() - 5f64 > -f64::EPSILON
        );
    }

    #[test]
    fn std_dev_test() {
        let mut std = StandardDeviation::new(8);

        assert_eq!(None, std.get());
        std.next(2f64);
        assert_eq!(None, std.get());
        std.next(4f64);
        assert_eq!(None, std.get());
        std.next(4f64);
        assert_eq!(None, std.get());
        std.next(4f64);
        assert_eq!(None, std.get());
        std.next(5f64);
        assert_eq!(None, std.get());
        std.next(5f64);
        assert_eq!(None, std.get());
        std.next(7f64);
        assert_eq!(None, std.get());
        std.next(9f64);
        assert!(
            std.get().unwrap() - 2f64 < f64::EPSILON && std.get().unwrap() - 2f64 > -f64::EPSILON
        );
    }
}

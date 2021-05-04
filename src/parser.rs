use std::iter::{Enumerate, Peekable};
use std::slice::Iter;

use crate::exchange::binance::Asset;
use crate::trading::Symbol;

pub struct TomlParser<'a> {
    iter: Peekable<Enumerate<Iter<'a, u8>>>,
    slice: &'a [u8],
}

impl<'a> TomlParser<'a> {
    pub fn new(slice: &'a [u8]) -> Self {
        Self {
            iter: slice.iter().enumerate().peekable(),
            slice,
        }
    }

    pub fn load_cache(&mut self) -> (Vec<Asset>, Vec<Symbol>) {
        let mut assets = Vec::new();
        let mut symbols = Vec::new();

        while let Some(table) = self.next_table() {
            if table == b"assets" {
                assets.push(self.parse_asset());
            } else if table == b"symbols" {
                symbols.push(self.parse_symbol());
            } else {
                panic!("Expected `assets` or `symbols` found something else");
            }
        }

        (assets, symbols)
    }

    fn skip_whitespaces(&mut self) {
        while let Some(_) = self.iter.next_if(|(_, b)| b.is_ascii_whitespace()) {}
    }

    fn skip_alphanumerics(&mut self) {
        while let Some(_) = self
            .iter
            .next_if(|&(_, &b)| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {}
    }

    fn skip_until(&mut self, delim: u8) -> Option<usize> {
        while let Some(_) = self.iter.next_if(|&(_, &b)| b != delim) {}
        self.iter.peek().map(|&(i, _)| i)
    }

    fn next_table(&mut self) -> Option<&[u8]> {
        self.skip_whitespaces();
        self.iter.next_if(|&(_, &b)| b == b'[')?;
        self.iter.next_if(|&(_, &b)| b == b'[');

        let start = self
            .iter
            .next_if(|(_, b)| b.is_ascii_alphanumeric())
            .map(|(i, _)| i)
            .unwrap();

        self.skip_alphanumerics();

        let end = self.iter.peek().map(|&(i, _)| i).unwrap();

        self.slice.get(start..end)
    }

    fn next_key(&mut self) -> Option<&[u8]> {
        self.skip_until(b'\n');
        self.skip_whitespaces();

        let start = self
            .iter
            .next_if(|(_, b)| b.is_ascii_alphanumeric())
            .map(|(i, _)| i)
            .unwrap();

        self.skip_alphanumerics();

        let end = self.iter.peek().map(|&(i, _)| i).unwrap();

        self.slice.get(start..end)
    }

    fn next_value(&mut self) -> Option<&[u8]> {
        self.skip_whitespaces();

        if let Some((i, &b)) = self.iter.next() {
            if b != b'=' {
                panic!("Unexpected char at {}, found {}", i, char::from(b));
            }
        } else {
            panic!("Unexpected EOF");
        }

        self.skip_whitespaces();

        if let Some(&(i, &b)) = self.iter.peek() {
            if b == b'"' {
                self.iter.next().unwrap();
                let start = self.iter.next().map(|(i, _)| i).unwrap();
                let end = self.skip_until(b'"').unwrap();

                self.slice.get(start..end)
            } else {
                let start = i;
                let end = self.skip_until(b'\n').unwrap();

                self.slice.get(start..end)
            }
        } else {
            panic!("Unexpected EOF");
        }
    }

    fn parse_asset(&mut self) -> Asset {
        let mut name = None;
        let mut balance = None;

        if let Some(key) = self.next_key() {
            if key != b"name" {
                panic!(
                    "Expected `name`, found {}",
                    std::str::from_utf8(key).unwrap()
                );
            }
            name = self
                .next_value()
                .map(|s| String::from_utf8(s.to_vec()).unwrap());
        }

        if let Some(key) = self.next_key() {
            if key != b"balance" {
                panic!(
                    "Expected `balance`, found {}",
                    std::str::from_utf8(key).unwrap()
                );
            }
            balance = self.next_value().map(|s| {
                let s = std::str::from_utf8(s).unwrap();
                s.parse::<f64>().unwrap()
            });
        }

        Asset::new(&name.unwrap(), balance.unwrap())
    }

    fn parse_symbol(&mut self) -> Symbol {
        let mut base = None;
        let mut quote = None;
        let mut mid = None;
        let mut step_size = None;

        if let Some(key) = self.next_key() {
            if key != b"base" {
                panic!(
                    "Expected `base`, found {}",
                    std::str::from_utf8(key).unwrap()
                );
            }
            base = self
                .next_value()
                .map(|s| String::from_utf8(s.to_vec()).unwrap());
            mid = base.as_ref().map(|s| s.len());
        }

        if let Some(key) = self.next_key() {
            if key != b"quote" {
                panic!(
                    "Expected `quote`, found {}",
                    std::str::from_utf8(key).unwrap()
                );
            }
            quote = self
                .next_value()
                .map(|s| String::from_utf8(s.to_vec()).unwrap());
        }

        if let Some(key) = self.next_key() {
            if key != b"step-size" {
                panic!(
                    "Expected `step-size`, found {}",
                    std::str::from_utf8(key).unwrap()
                );
            }
            step_size = self.next_value().map(|s| {
                let s = std::str::from_utf8(s).unwrap();
                s.parse::<i32>().unwrap()
            });
        }

        let name = base.zip(quote).map(|(b, q)| {
            let mut s = String::with_capacity(b.len() + q.len());
            s.push_str(&b);
            s.push_str(&q);
            s
        });

        Symbol::from_string(name.unwrap(), mid.unwrap(), step_size.unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn it_works() {
        let mut f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("cache.toml")
            .unwrap();
        let mut buf: Vec<u8> = Vec::new();
        f.read_to_end(&mut buf).unwrap();

        let mut parser = TomlParser::new(&buf);
        let (assets, symbols) = parser.load_cache();

        assert_eq!(assets.len(), 2);
        assert_eq!(symbols.len(), 2);
    }
}

use std::iter::Peekable;
use std::slice::Iter;

pub enum Value {
    Object,
    Array,
    Other,
}

pub struct Haystack<'a> {
    iter: Peekable<Iter<'a, u8>>,
    pos: usize,
    slice: &'a [u8],
}

impl<'a> Haystack<'a> {
    pub fn new(slice: &'a [u8]) -> Self {
        Self {
            iter: slice.iter().peekable(),
            pos: 0,
            slice,
        }
    }

    pub fn next(&mut self) -> Option<&u8> {
        let a = self.iter.next()?;
        self.pos += 1;
        Some(a)
    }

    pub fn peek(&mut self) -> Option<&&u8> {
        self.iter.peek()
    }

    pub fn skip_whitespace(&mut self) {
        while let Some(b) = self.peek() {
            if !b.is_ascii_whitespace() {
                break;
            }
            self.next().unwrap();
        }
    }

    pub fn skip_value(&mut self, value: Value) {
        match value {
            Value::Object => self.skip_object(),
            Value::Array => self.skip_array(),
            Value::Other => self.skip_other(),
        }
    }

    fn skip_object(&mut self) {
        let mut curly: usize = 1;

        while let Some(b) = self.peek() {
            if **b == b'{' {
                curly += 1;
            } else if **b == b'}' {
                curly -= 1;
                if curly == 0 {
                    self.next().unwrap();
                    break;
                }
            }
            self.next().unwrap();
        }
    }

    fn skip_array(&mut self) {
        let mut square: usize = 1;

        while square > 0 {
            let b = self.next().unwrap();
            if *b == b'[' {
                square += 1;
            } else if *b == b']' {
                square -= 1;
            } else {
                continue;
            }
        }
    }

    fn skip_other(&mut self) {
        while let Some(b) = self.next() {
            if *b == b',' {
                break;
            }
        }
    }

    pub fn find_pair(&mut self, key: &str, value: &str) {
        let mut k: Option<&[u8]> = None;
        let mut v: Option<&[u8]> = None;

        let mut index: Option<usize> = None;

        loop {
            match (k, v) {
                (None, None) => match index {
                    None => {
                        while let Some(b) = self.peek() {
                            if **b == b'"' {
                                break;
                            }

                            self.next().unwrap();
                        }

                        if let Some(_) = self.next() {
                            if let Some(b) = self.peek() {
                                if b.is_ascii_alphanumeric() {
                                    index = Some(self.pos);
                                } else {
                                    panic!("Unexpected char: {:?}", char::from(**b));
                                }
                            } else {
                                panic!("Unexpected EOF");
                            }
                        } else {
                            panic!("Unexpected EOF");
                        }
                    }
                    Some(i) => {
                        while let Some(b) = self.peek() {
                            if **b == b'"' {
                                k = Some(&self.slice[i..self.pos]);
                                self.next().unwrap();
                                break;
                            }

                            if !b.is_ascii_alphanumeric() {
                                panic!("Unexpected char in the key: {:?}", char::from(**b));
                            }

                            self.next().unwrap();
                        }

                        if k.is_none() {
                            panic!("Could not find end of the key. Unexpected EOF");
                        } else {
                            index = None;
                        }
                    }
                },
                (Some(key_bytes), None) => {
                    if key_bytes != key.as_bytes() {
                        /*
                        if let Some(b) = self.next() {
                            if *b == b':' {
                                if let Some(b) = self.next() {
                                    match *b {
                                        b'[' => self.skip_value(Value::Array),
                                        b'{' => self.skip_value(Value::Object),
                                        _ => self.skip_value(Value::Other),
                                    }
                                } else {
                                    panic!("Unexpected EOF");
                                }
                            } else {
                                panic!("Unexpected char: {:?}, expected ':'", char::from(*b));
                            }
                        } else {
                            panic!("Unexpected EOF");
                        }
                        */
                        self.skip_value(Value::Object);

                        k = None;
                    } else {
                        match index {
                            None => {
                                if let Some(b) = self.next() {
                                    if *b == b':' {
                                        if let Some(b) = self.peek() {
                                            if b.is_ascii_alphanumeric() {
                                                index = Some(self.pos);
                                            } else if **b == b'"' {
                                                index = Some(self.pos + 1);
                                                self.next().unwrap();
                                            } else {
                                                panic!(
                                                    "Unexpected char in the value: {:?}",
                                                    char::from(**b)
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            Some(i) => {
                                while let Some(b) = self.peek() {
                                    if **b == b'"' || **b == b',' {
                                        v = Some(&self.slice[i..self.pos]);
                                        self.next().unwrap();
                                        break;
                                    }

                                    if !b.is_ascii_alphanumeric() && **b != b'_' {
                                        panic!(
                                            "Unexpected char in the value: {:?}",
                                            char::from(**b)
                                        );
                                    }

                                    self.next().unwrap();
                                }

                                if v.is_none() {
                                    panic!("Could not find end of the value. Unexpected EOF");
                                }
                            }
                        }
                    }
                }
                (Some(_), Some(val_bytes)) => {
                    if val_bytes == value.as_bytes() {
                        break;
                    } else {
                        k = None;
                        v = None;
                        index = None;
                    }
                }
                _ => (),
            }
        }
    }

    pub fn find_key(&mut self, key: &str) {
        let mut k: Option<&[u8]> = None;
        let mut index: Option<usize> = None;

        loop {
            match k {
                None => match index {
                    None => {
                        while let Some(b) = self.peek() {
                            if **b == b'"' {
                                self.next().unwrap();
                                break;
                            }

                            self.next().unwrap();
                        }

                        if let Some(b) = self.peek() {
                            if b.is_ascii_alphanumeric() {
                                index = Some(self.pos);
                            } else {
                                panic!(
                                    "Unexpected char at the start of key: {:?}",
                                    char::from(**b)
                                );
                            }
                        } else {
                            panic!("Unexpected EOF");
                        }
                    }
                    Some(i) => {
                        while let Some(b) = self.peek() {
                            if **b == b'"' {
                                k = Some(&self.slice[i..self.pos]);
                                self.next().unwrap();
                                break;
                            }

                            if !b.is_ascii_alphanumeric() {
                                panic!("Unexpected char in the key: {:?}", char::from(**b));
                            }

                            self.next().unwrap();
                        }

                        if k.is_none() {
                            panic!("Could not find end of the key. Unexpected EOF");
                        }
                    }
                },
                Some(key_bytes) => {
                    if key_bytes != key.as_bytes() {
                        if let Some(b) = self.next() {
                            if *b == b':' {
                                if let Some(b) = self.next() {
                                    match *b {
                                        b'[' => self.skip_value(Value::Array),
                                        b'{' => self.skip_value(Value::Object),
                                        _ => self.skip_value(Value::Other),
                                    }
                                } else {
                                    panic!("Unexpected EOF after colon");
                                }
                            } else {
                                panic!("Expected ':' after key, found {:?}", char::from(*b));
                            }
                        } else {
                            panic!("Unexpected EOF after key");
                        }

                        k = None;
                        index = None;
                    } else {
                        break;
                    }
                }
            }
        }
    }
}

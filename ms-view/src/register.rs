//! Vim register storage.
//!
//! Registers map a character to a list of string values.
//! Special registers: `"` (unnamed default), `_` (blackhole).

use std::collections::HashMap;

/// Register storage: char → values.
#[derive(Debug)]
pub struct Registers {
    inner: HashMap<char, Vec<String>>,
}

impl Registers {
    /// Create empty register store.
    #[must_use]
    pub fn new() -> Self {
        Self { inner: HashMap::new() }
    }

    /// Read the latest value from a register.
    /// Returns `None` for blackhole (`_`) or empty.
    #[must_use]
    pub fn read(&self, reg: char) -> Option<&str> {
        if reg == '_' {
            return None;
        }
        let reg = Self::resolve(reg);
        self.inner.get(&reg).and_then(|v| v.last()).map(String::as_str)
    }

    /// Read all values from a register.
    #[must_use]
    pub fn read_all(&self, reg: char) -> Option<&[String]> {
        if reg == '_' {
            return None;
        }
        let reg = Self::resolve(reg);
        self.inner.get(&reg).map(Vec::as_slice)
    }

    /// Write a value to a register, replacing previous.
    pub fn write(&mut self, reg: char, value: String) {
        if reg == '_' {
            return;
        }
        let reg = Self::resolve(reg);
        self.inner.insert(reg, vec![value]);
    }

    /// Push a value to a register (for appending with
    /// uppercase registers A-Z).
    pub fn push(&mut self, reg: char, value: String) {
        if reg == '_' {
            return;
        }
        // Uppercase appends to lowercase register
        let target = if reg.is_ascii_uppercase() {
            reg.to_ascii_lowercase()
        } else {
            reg
        };
        self.inner.entry(target).or_default().push(value);
    }

    /// Resolve register name: `"` is unnamed (stored
    /// as `"`), uppercase maps to lowercase for reads.
    const fn resolve(reg: char) -> char {
        if reg.is_ascii_uppercase() {
            reg.to_ascii_lowercase()
        } else {
            reg
        }
    }
}

impl Default for Registers {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_read() {
        let mut r = Registers::new();
        r.write('"', "hello".to_owned());
        assert_eq!(r.read('"'), Some("hello"));
    }

    #[test]
    fn blackhole_discards() {
        let mut r = Registers::new();
        r.write('_', "gone".to_owned());
        assert_eq!(r.read('_'), None);
    }

    #[test]
    fn uppercase_appends() {
        let mut r = Registers::new();
        r.write('a', "first".to_owned());
        r.push('A', "second".to_owned());
        let vals = r.read_all('a');
        assert_eq!(
            vals,
            Some(vec!["first".to_owned(), "second".to_owned(),].as_slice()),
        );
    }

    #[test]
    fn uppercase_reads_lowercase() {
        let mut r = Registers::new();
        r.write('a', "hello".to_owned());
        assert_eq!(r.read('A'), Some("hello"));
    }

    #[test]
    fn empty_register_returns_none() {
        let r = Registers::new();
        assert_eq!(r.read('a'), None);
    }

    #[test]
    fn write_replaces() {
        let mut r = Registers::new();
        r.write('a', "old".to_owned());
        r.write('a', "new".to_owned());
        assert_eq!(r.read('a'), Some("new"));
    }
}

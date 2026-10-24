pub trait HeaderLookup {
    fn header_value(&self, name: &str) -> Option<&str>;
}

impl<K: AsRef<str>, V: AsRef<str>> HeaderLookup for [(K, V)] {
    fn header_value(&self, name: &str) -> Option<&str> {
        self.iter()
            .find(|(key, _)| key.as_ref().eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_ref())
    }
}

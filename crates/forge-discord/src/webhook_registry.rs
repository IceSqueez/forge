use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct WebhookRegistry {
    names: Vec<String>,
    last_ok: HashMap<String, bool>,
}

impl WebhookRegistry {
    pub(crate) fn names(&self) -> &[String] {
        &self.names
    }

    pub(crate) fn first(&self) -> Option<&String> {
        self.names.first()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.names.len()
    }

    pub(crate) fn last_ok(&self, name: &str) -> Option<bool> {
        self.last_ok.get(name).copied()
    }

    pub(crate) fn replace(&mut self, names: &[String]) {
        self.names = names.to_vec();
    }

    pub(crate) fn insert(&mut self, name: &str) {
        if !self.names.iter().any(|known| known == name) {
            self.names.push(name.to_owned());
            self.names.sort_unstable();
        }
    }

    pub(crate) fn remove(&mut self, name: &str) {
        self.names.retain(|known| known != name);
        self.last_ok.remove(name);
    }

    pub(crate) fn record_outcome(&mut self, name: &str, ok: bool) {
        self.last_ok.insert(name.to_owned(), ok);
    }
}

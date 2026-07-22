pub struct Confirm<T> {
    value: Option<T>,
}

impl<T> Default for Confirm<T> {
    fn default() -> Self {
        Self { value: None }
    }
}

impl<T> Confirm<T> {
    pub fn request(&mut self, value: T) {
        self.value = Some(value);
    }

    pub fn take(&mut self) -> Option<T> {
        self.value.take()
    }

    pub fn cancel(&mut self) {
        self.value = None;
    }

    pub fn is_pending(&self) -> bool {
        self.value.is_some()
    }

    pub fn get(&self) -> Option<&T> {
        self.value.as_ref()
    }
}

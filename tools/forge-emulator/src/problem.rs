use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub location: String,
    pub message: String,
}

impl Problem {
    pub fn new(location: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            location: location.into(),
            message: message.into(),
        }
    }

    pub fn under(self, prefix: &str) -> Self {
        Self::new(format!("{prefix}.{}", self.location), self.message)
    }

    pub fn outside(location: impl Into<String>, value: u64, min: u64, max: u64) -> Option<Self> {
        (!(min..=max).contains(&value)).then(|| {
            Self::new(
                location,
                format!("must be between {min} and {max}, got {value}"),
            )
        })
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.location, self.message)
    }
}

use serde_json::{Value, json};

const PARSE_USERS: &str = "users";
const PARSE_ROLES: &str = "roles";
const PARSE_EVERYONE: &str = "everyone";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MentionPolicy {
    pub allow_roles: bool,
    pub allow_everyone: bool,
}

impl Default for MentionPolicy {
    fn default() -> Self {
        Self {
            allow_roles: true,
            allow_everyone: false,
        }
    }
}

impl MentionPolicy {
    pub(crate) fn to_wire(self) -> Value {
        let mut parse = vec![PARSE_USERS];
        if self.allow_roles {
            parse.push(PARSE_ROLES);
        }
        if self.allow_everyone {
            parse.push(PARSE_EVERYONE);
        }
        json!({ "parse": parse })
    }
}

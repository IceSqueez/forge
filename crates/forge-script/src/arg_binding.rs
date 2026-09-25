use std::collections::HashMap;

use forge_types::{ArgStack, Variant, is_variable_reference};

use crate::convert::variant_to_dynamic;

const BINDING_PREFIX: &str = "forge_arg_";

/// An author-written expression whose `%name%` tokens are rewritten into rhai scope variables, so argument values never become rhai syntax.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundExpression {
    original: String,
    source: String,
    bindings: Vec<(String, Variant)>,
    unresolved: Vec<String>,
}

impl BoundExpression {
    /// Outside string literals a token binds its typed argument (numeric / boolean text reads as that literal); inside one it binds the argument text.
    pub fn bind(expr: &str, args: &ArgStack) -> Self {
        let mut prefix = BINDING_PREFIX.to_owned();
        while expr.contains(&prefix) {
            prefix.push('x');
        }
        let mut binder = Binder {
            args,
            prefix,
            bindings: Vec::new(),
            slots: HashMap::new(),
            unresolved: Vec::new(),
        };
        let chars: Vec<char> = expr.chars().collect();
        let mut pos = 0;
        let mut source = String::with_capacity(expr.len());
        binder.code(&chars, &mut pos, &mut source, false);
        Self {
            original: expr.to_owned(),
            source,
            bindings: binder.bindings,
            unresolved: binder.unresolved,
        }
    }

    pub fn original(&self) -> &str {
        &self.original
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn scope(&self) -> rhai::Scope<'static> {
        let mut scope = rhai::Scope::new();
        for (name, value) in &self.bindings {
            scope.push_constant_dynamic(name.clone(), variant_to_dynamic(value.clone()));
        }
        scope
    }

    /// Names of `%name%` tokens outside string literals that matched no argument; they stay verbatim in the source.
    pub fn unresolved(&self) -> &[String] {
        &self.unresolved
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Slot {
    Typed,
    Text,
}

#[derive(Clone, Copy)]
enum TokenScope {
    Code,
    Quoted,
    Verbatim,
}

enum Piece {
    Text(String),
    Bound(String),
}

struct Binder<'a> {
    args: &'a ArgStack,
    prefix: String,
    bindings: Vec<(String, Variant)>,
    slots: HashMap<(String, Slot), String>,
    unresolved: Vec<String>,
}

impl Binder<'_> {
    fn code(&mut self, chars: &[char], pos: &mut usize, out: &mut String, in_interpolation: bool) {
        let mut depth = 0usize;
        while let Some(&c) = chars.get(*pos) {
            match c {
                '"' => self.quoted(chars, pos, out),
                '`' => self.verbatim(chars, pos, out),
                '\'' => copy_char_literal(chars, pos, out),
                '/' if matches!(chars.get(*pos + 1), Some('/' | '*')) => {
                    copy_comment(chars, pos, out)
                }
                '%' => self.code_token(chars, pos, out),
                '{' => {
                    depth += 1;
                    out.push(c);
                    *pos += 1;
                }
                '}' if in_interpolation && depth == 0 => return,
                '}' => {
                    depth = depth.saturating_sub(1);
                    out.push(c);
                    *pos += 1;
                }
                _ => {
                    out.push(c);
                    *pos += 1;
                }
            }
        }
    }

    fn code_token(&mut self, chars: &[char], pos: &mut usize, out: &mut String) {
        let Some((key, end)) = token_at(chars, *pos, TokenScope::Code) else {
            out.push('%');
            *pos += 1;
            return;
        };
        let args = self.args;
        match args.resolve(&key) {
            Some(value) => {
                let name = self.slot(&key, Slot::Typed, value);
                push_identifier(out, &name, chars.get(end).copied());
            }
            None => {
                out.extend(&chars[*pos..end]);
                if !self.unresolved.contains(&key) {
                    self.unresolved.push(key);
                }
            }
        }
        *pos = end;
    }

    fn quoted(&mut self, chars: &[char], pos: &mut usize, out: &mut String) {
        let start = *pos;
        let args = self.args;
        let mut pieces = Vec::new();
        let mut text = String::new();
        let mut cursor = start + 1;
        loop {
            let Some(&c) = chars.get(cursor) else {
                out.extend(&chars[start..]);
                *pos = chars.len();
                return;
            };
            match c {
                '"' if chars.get(cursor + 1) == Some(&'"') => {
                    text.push_str("\"\"");
                    cursor += 2;
                }
                '"' => {
                    cursor += 1;
                    break;
                }
                '\\' => {
                    text.push(c);
                    if let Some(&escaped) = chars.get(cursor + 1) {
                        text.push(escaped);
                    }
                    cursor += 2;
                }
                '%' => match token_at(chars, cursor, TokenScope::Quoted) {
                    Some((key, end)) => {
                        match args.resolve(&key) {
                            Some(value) => {
                                if !text.is_empty() {
                                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                                }
                                pieces.push(Piece::Bound(self.slot(&key, Slot::Text, value)));
                            }
                            None => text.extend(&chars[cursor..end]),
                        }
                        cursor = end;
                    }
                    None => {
                        text.push('%');
                        cursor += 1;
                    }
                },
                _ => {
                    text.push(c);
                    cursor += 1;
                }
            }
        }
        *pos = cursor;
        if !pieces.iter().any(|p| matches!(p, Piece::Bound(_))) {
            out.extend(&chars[start..cursor]);
            return;
        }
        if !text.is_empty() {
            pieces.push(Piece::Text(text));
        }
        if let [Piece::Bound(name)] = pieces.as_slice() {
            out.push_str(name);
            return;
        }
        let parts: Vec<String> = pieces
            .into_iter()
            .map(|piece| match piece {
                Piece::Text(raw) => format!("\"{raw}\""),
                Piece::Bound(name) => name,
            })
            .collect();
        out.push('(');
        out.push_str(&parts.join(" + "));
        out.push(')');
    }

    fn verbatim(&mut self, chars: &[char], pos: &mut usize, out: &mut String) {
        let args = self.args;
        out.push('`');
        *pos += 1;
        while let Some(&c) = chars.get(*pos) {
            match c {
                '`' if chars.get(*pos + 1) == Some(&'`') => {
                    out.push_str("``");
                    *pos += 2;
                }
                '`' => {
                    out.push('`');
                    *pos += 1;
                    return;
                }
                '$' if chars.get(*pos + 1) == Some(&'{') => {
                    out.push_str("${");
                    *pos += 2;
                    self.code(chars, pos, out, true);
                    if chars.get(*pos) == Some(&'}') {
                        out.push('}');
                        *pos += 1;
                    }
                }
                '%' => match token_at(chars, *pos, TokenScope::Verbatim) {
                    Some((key, end)) => {
                        match args.resolve(&key) {
                            Some(value) => {
                                let name = self.slot(&key, Slot::Text, value);
                                out.push_str("${");
                                out.push_str(&name);
                                out.push('}');
                            }
                            None => out.extend(&chars[*pos..end]),
                        }
                        *pos = end;
                    }
                    None => {
                        out.push('%');
                        *pos += 1;
                    }
                },
                _ => {
                    out.push(c);
                    *pos += 1;
                }
            }
        }
    }

    fn slot(&mut self, key: &str, slot: Slot, value: &Variant) -> String {
        let slot_key = (key.to_owned(), slot);
        if let Some(name) = self.slots.get(&slot_key) {
            return name.clone();
        }
        let name = format!("{}{}", self.prefix, self.bindings.len());
        let bound = match slot {
            Slot::Typed => as_code_value(value),
            Slot::Text => Variant::String(value.to_string()),
        };
        self.bindings.push((name.clone(), bound));
        self.slots.insert(slot_key, name.clone());
        name
    }
}

fn token_at(chars: &[char], start: usize, scope: TokenScope) -> Option<(String, usize)> {
    let mut end = start + 1;
    while let Some(&c) = chars.get(end) {
        if c == '%' {
            let key: String = chars[start + 1..end].iter().collect();
            return is_variable_reference(&key).then(|| (key, end + 1));
        }
        let leaves_literal = match scope {
            TokenScope::Code => false,
            TokenScope::Quoted => matches!(c, '"' | '\\'),
            TokenScope::Verbatim => c == '`',
        };
        if leaves_literal {
            return None;
        }
        end += 1;
    }
    None
}

fn as_code_value(value: &Variant) -> Variant {
    let Variant::String(text) = value else {
        return value.clone();
    };
    let trimmed = text.trim();
    if let Ok(n) = trimmed.parse::<i64>() {
        return Variant::Int(n);
    }
    if let Ok(f) = trimmed.parse::<f64>()
        && f.is_finite()
    {
        return Variant::Float(f);
    }
    match trimmed {
        "true" => Variant::Bool(true),
        "false" => Variant::Bool(false),
        _ => value.clone(),
    }
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn push_identifier(out: &mut String, name: &str, next: Option<char>) {
    if out.chars().next_back().is_some_and(is_identifier_char) {
        out.push(' ');
    }
    out.push_str(name);
    if next.is_some_and(is_identifier_char) {
        out.push(' ');
    }
}

fn copy_char_literal(chars: &[char], pos: &mut usize, out: &mut String) {
    out.push('\'');
    *pos += 1;
    while let Some(&c) = chars.get(*pos) {
        out.push(c);
        *pos += 1;
        match c {
            '\\' => {
                if let Some(&escaped) = chars.get(*pos) {
                    out.push(escaped);
                    *pos += 1;
                }
            }
            '\'' => return,
            _ => {}
        }
    }
}

fn copy_comment(chars: &[char], pos: &mut usize, out: &mut String) {
    let block = chars.get(*pos + 1) == Some(&'*');
    out.push_str(if block { "/*" } else { "//" });
    *pos += 2;
    while let Some(&c) = chars.get(*pos) {
        if block && c == '*' && chars.get(*pos + 1) == Some(&'/') {
            out.push_str("*/");
            *pos += 2;
            return;
        }
        if !block && c == '\n' {
            return;
        }
        out.push(c);
        *pos += 1;
    }
}

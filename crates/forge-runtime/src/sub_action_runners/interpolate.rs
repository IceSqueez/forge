use forge_storage::GlobalsRepo;
use forge_types::ArgStack;

pub(super) async fn interpolate_with_globals(
    template: &str,
    arg_stack: &ArgStack,
    globals: &dyn GlobalsRepo,
) -> String {
    let after_args = arg_stack.interpolate(template);
    if !after_args.contains('%') {
        return after_args;
    }
    let mut result = String::with_capacity(after_args.len());
    let mut chars = after_args.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            result.push(ch);
            continue;
        }
        let token_start = result.len();
        result.push('%');
        let mut key = String::new();
        let mut closed = false;
        for inner in chars.by_ref() {
            if inner == '%' {
                closed = true;
                break;
            }
            key.push(inner);
        }
        if !closed {
            continue;
        }
        match globals.get(&key).await {
            Ok(Some(value)) => {
                result.truncate(token_start);
                result.push_str(&value.to_string());
            }
            _ => {
                result.push_str(&key);
                result.push('%');
            }
        }
    }
    result
}

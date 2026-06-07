#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserParam {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserFunctionSig {
    pub name: String,
    pub params: Vec<UserParam>,
    pub return_type: Option<String>,
    pub doc: Option<String>,
}

pub fn collect_user_functions(source: &str) -> Vec<UserFunctionSig> {
    let lines: Vec<&str> = source.lines().collect();
    let mut result = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let Some(after_fn) = trimmed.strip_prefix("fn ") else {
            continue;
        };
        let Some(paren_pos) = after_fn.find('(') else {
            continue;
        };
        let name = after_fn[..paren_pos].trim().to_string();
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }

        let mut params: Vec<UserParam> = Vec::new();
        let mut return_type: Option<String> = None;
        let mut doc_lines: Vec<String> = Vec::new();

        let mut scan_idx = line_idx;
        while scan_idx > 0 {
            scan_idx -= 1;
            let scan_trimmed = lines[scan_idx].trim();
            if !scan_trimmed.starts_with("//") {
                break;
            }
            let after_slashes = scan_trimmed.trim_start_matches("//").trim();
            if let Some(input_rest) = after_slashes.strip_prefix("@input ") {
                if let Some(colon) = input_rest.find(':') {
                    let param_name = input_rest[..colon].trim().to_string();
                    let ty = input_rest[colon + 1..].trim().to_string();
                    if !param_name.is_empty() && !ty.is_empty() {
                        params.push(UserParam {
                            name: param_name,
                            ty,
                        });
                    }
                }
            } else if let Some(ret_rest) = after_slashes.strip_prefix("@return ") {
                let ty = ret_rest.trim().to_string();
                if !ty.is_empty() && return_type.is_none() {
                    return_type = Some(ty);
                }
            } else if !after_slashes.is_empty() {
                doc_lines.push(after_slashes.to_string());
            }
        }

        params.reverse();
        doc_lines.reverse();

        let doc = if doc_lines.is_empty() {
            None
        } else {
            Some(doc_lines.join(" "))
        };

        result.push(UserFunctionSig {
            name,
            params,
            return_type,
            doc,
        });
    }

    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn collect_user_functions_extracts_simple_fn() {
        let src = "fn greet(name) { \"hello\" }";
        let fns = collect_user_functions(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "greet");
    }

    #[test]
    fn collect_user_functions_parses_at_input_and_at_return_annotations() {
        let src = "// @input name: string\n// @return string\nfn greet(name) { name }";
        let fns = collect_user_functions(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].params.len(), 1);
        assert_eq!(fns[0].params[0].name, "name");
        assert_eq!(fns[0].params[0].ty, "string");
        assert_eq!(fns[0].return_type, Some("string".to_owned()));
    }

    #[test]
    fn collect_user_functions_handles_fn_without_annotations() {
        let src = "fn helper(x) { x + 1 }";
        let fns = collect_user_functions(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "helper");
        assert!(fns[0].params.is_empty());
        assert!(fns[0].return_type.is_none());
    }

    #[test]
    fn collect_user_functions_stops_at_blank_line() {
        let src = "// @input x: int\n\n// @input y: int\nfn add(x, y) { x + y }";
        let fns = collect_user_functions(src);
        assert_eq!(fns[0].params.len(), 1);
        assert_eq!(fns[0].params[0].name, "y");
    }
}

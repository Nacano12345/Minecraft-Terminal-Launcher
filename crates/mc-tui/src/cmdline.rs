//! Minimal shell-like tokenizer for the editable launch command line.
//!
//! Supports single/double quotes and backslash escapes. No variables, pipes
//! or redirection: the command line is only split into argv.

pub fn tokenize(input: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    let mut chars = input.chars();

    while let Some(ch) = chars.next() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                } else if ch == '\\' && q == '"' {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    } else {
                        current.push('\\');
                    }
                } else {
                    current.push(ch);
                }
            }
            None => {
                if ch == '\'' || ch == '"' {
                    quote = Some(ch);
                    started = true;
                } else if ch == '\\' {
                    if let Some(next) = chars.next() {
                        current.push(next);
                        started = true;
                    }
                } else if ch.is_whitespace() {
                    if started {
                        tokens.push(std::mem::take(&mut current));
                        started = false;
                    }
                } else {
                    current.push(ch);
                    started = true;
                }
            }
        }
    }

    if let Some(q) = quote {
        return Err(format!("unterminated {q} quote"));
    }
    if started {
        tokens.push(current);
    }
    if tokens.is_empty() {
        return Err("empty command line".to_string());
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::tokenize;

    #[test]
    fn splits_plain_args() {
        assert_eq!(
            tokenize("java -Xmx2G -cp a:b Main").unwrap(),
            vec!["java", "-Xmx2G", "-cp", "a:b", "Main"]
        );
    }

    #[test]
    fn handles_quotes() {
        assert_eq!(
            tokenize(r#"java -Dfoo="a b" 'x y'"#).unwrap(),
            vec!["java", "-Dfoo=a b", "x y"]
        );
    }

    #[test]
    fn rejects_invalid() {
        assert!(tokenize("java \"oops").is_err());
        assert!(tokenize("   ").is_err());
    }
}

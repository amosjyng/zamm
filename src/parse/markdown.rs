use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};

/// Extracts Rust code blocks from the markdown.
pub fn extract_rust(markdown: &str) -> String {
    // note: go back to commit 158f648 to retrieve YAML-parsing code, including markdown quote
    // extraction
    let mut code = String::new();
    let mut in_rust_block = false;
    for event in Parser::new(markdown) {
        match event {
            Event::Start(tag) => {
                if let Tag::CodeBlock(kind) = tag {
                    if let CodeBlockKind::Fenced(cow) = kind {
                        if let "rust" = cow.to_string().as_str() {
                            in_rust_block = true
                        }
                    }
                }
            }
            Event::Text(content) => {
                if in_rust_block {
                    code += &content;
                }
            }
            Event::End(tag) => {
                if let Tag::CodeBlock(_) = tag {
                    in_rust_block = false
                }
            }
            _ => (),
        }
    }
    if code.ends_with("\n\n") {
        // happens if input code already contains trailing newline
        code.pop();
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_rust_extraction_nothing() {
        assert_eq!(
            extract_rust(indoc! {"
            # Some document

            No code in here.
        "}),
            "".to_owned()
        );
    }

    #[test]
    fn test_rust_extraction_one_block() {
        assert_eq!(
            extract_rust(indoc! {"
            # Some document

            ```rust
            let x = 5;
            ```

            Aha! We have some code.
        "}),
            indoc! {"
            let x = 5;
        "}
        );
    }

    #[test]
    fn test_rust_extraction_multiple_blocks() {
        assert_eq!(
            extract_rust(indoc! {r#"
            # Some document

            ```rust
            let x = 5;
            ```

            Aha! We have some code. More?

            ## Yes more

            ```json
            {"very": "devious"}
            ```

            Will it skip that?

            ```
            And this too?
            ```

            ```rust
            let y = x + 1;
            println!("One more than x is {}", y);
            ```
        "#}),
            indoc! {r#"
            let x = 5;
            let y = x + 1;
            println!("One more than x is {}", y);
        "#}
        );
    }
}

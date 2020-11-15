use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};

/// Extraction of different languages from the Markdown source.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct CodeExtraction {
    pub rust: String,
    pub toml: String,
}

impl CodeExtraction {
    fn trim_code(code: &mut String) {
        if code.ends_with("\n\n") {
            // happens if input code already contains trailing newline
            code.pop();
        }
    }

    fn trim(&mut self) {
        Self::trim_code(&mut self.rust);
        Self::trim_code(&mut self.toml);
    }
}

/// Extracts code blocks from the markdown.
pub fn extract_code(markdown: &str) -> CodeExtraction {
    // note: go back to commit 158f648 in Yang to retrieve YAML-parsing code, including markdown
    // quote extraction
    let mut code = CodeExtraction::default();
    let mut code_block: Option<String> = None;
    for event in Parser::new(markdown) {
        match event {
            Event::Start(tag) => {
                if let Tag::CodeBlock(kind) = tag {
                    if let CodeBlockKind::Fenced(cow) = kind {
                        code_block = Some(cow.to_string());
                    }
                }
            }
            Event::Text(content) => match &code_block {
                Some(lang) if lang == "rust" => code.rust += &content,
                Some(lang) if lang == "toml" => code.toml += &content,
                _ => (),
            },
            Event::End(tag) => {
                if let Tag::CodeBlock(_) = tag {
                    code_block = None;
                }
            }
            _ => (),
        }
    }

    code.trim();
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_rust_extraction_nothing() {
        assert_eq!(
            extract_code(indoc! {"
                # Some document

                No code in here.
            "}),
            CodeExtraction::default()
        );
    }

    #[test]
    fn test_rust_extraction_one_block() {
        assert_eq!(
            extract_code(indoc! {"
            # Some document

            ```rust
            let x = 5;
            ```

            Aha! We have some code.
        "}),
            CodeExtraction {
                rust: indoc! {"
                    let x = 5;
                "}
                .to_owned(),
                toml: "".to_owned()
            }
        );
    }

    #[test]
    fn test_rust_extraction_multiple_blocks() {
        assert_eq!(
            extract_code(indoc! {r#"
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
            CodeExtraction {
                rust: indoc! {r#"
                    let x = 5;
                    let y = x + 1;
                    println!("One more than x is {}", y);
                "#}
                .to_owned(),
                toml: "".to_owned()
            }
        );
    }

    #[test]
    fn test_rust_extraction_multiple_blocks_and_toml() {
        assert_eq!(
            extract_code(indoc! {r#"
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

            Add some dependencies.

            ```toml
            dep1 = "0.0.1"
            ```

            ```rust
            let y = x + 1;
            println!("One more than x is {}", y);
            ```

            So dependent on others, so very helpless:

            ```toml
            dep2 = {path = "C:/Users/Me/Documents/forbidden/fruit/"}
            ```
        "#}),
            CodeExtraction {
                rust: indoc! {r#"
                    let x = 5;
                    let y = x + 1;
                    println!("One more than x is {}", y);
                "#}
                .to_owned(),
                toml: indoc! {r#"
                    dep1 = "0.0.1"
                    dep2 = {path = "C:/Users/Me/Documents/forbidden/fruit/"}
                "#}
                .to_owned()
            }
        );
    }
}

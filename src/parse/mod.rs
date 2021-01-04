/// Grabs imported data.
mod handle_imports;
/// Literate programming support - extracts relevant code from Markdown file.
pub mod markdown;

use handle_imports::retrieve_imports;
pub use markdown::{extract_code, CodeExtraction};
use path_abs::{PathAbs, PathInfo};
use std::env;
use std::fs::read_to_string;
use std::io::{Error, ErrorKind};
use std::path::Path;

/// All supported input filename extensions.
pub const SUPPORTED_EXTENSIONS: &[&str] = &["md"];

/// Filename for ZAMM override file.
pub const ZAMM_OVERRIDE_NAME: &str = "zamm_override.md";

/// Parse output, including the original markdown text.
pub struct ParseOutput {
    /// The original filename.
    pub filename: String,
    /// The original markdown text.
    pub markdown: String,
    /// Code extractions from the original markdown.
    pub extractions: CodeExtraction,
}

/// Find the right input file.
pub fn find_file(specified_file: Option<&str>) -> Result<PathAbs, Error> {
    match specified_file {
        Some(filename) => {
            let path = PathAbs::new(Path::new(&filename))?;
            let path_str = path.as_path().to_str().unwrap();
            if path.exists() {
                println!("Using specified input file at {}", path_str);
                Ok(path)
            } else {
                Err(Error::new(
                    ErrorKind::NotFound,
                    format!("Specified input file was not found at {}", path_str),
                ))
            }
        }
        None => {
            for extension in SUPPORTED_EXTENSIONS {
                let path = PathAbs::new(Path::new(format!("yin.{}", extension).as_str()))?;
                if path.exists() {
                    println!(
                        "Using default input file at {}",
                        path.as_path().to_str().unwrap()
                    );
                    return Ok(path);
                }
            }
            let current_dir = env::current_dir()?;
            let current_dir_path = current_dir.to_str().unwrap();
            Err(Error::new(
                ErrorKind::NotFound,
                format!(
                    "No input file was specified, and no default inputs were found in the current \
                    directory of {}",
                    current_dir_path
                ),
            ))
        }
    }
}

fn retrieve_override() -> Result<Option<String>, Error> {
    let override_path = PathAbs::new(Path::new(ZAMM_OVERRIDE_NAME))?;
    if override_path.exists() {
        let override_content = read_to_string(&override_path)?;
        Ok(Some(override_content))
    } else {
        Ok(None)
    }
}

/// Parse the given input file.
pub fn parse_input(found_input: PathAbs) -> Result<ParseOutput, Error> {
    println!(
        "cargo:rerun-if-changed={}",
        found_input.as_os_str().to_str().unwrap()
    );
    let contents = read_to_string(&found_input)?;
    let extension = found_input
        .extension()
        .map(|e| e.to_str().unwrap())
        .unwrap_or("");
    match extension {
        "md" => {
            let mut initial_extraction = extract_code(&contents);
            let override_content: String = retrieve_override()?.unwrap_or_default();
            let override_extraction = extract_code(&override_content);

            initial_extraction.rust += &override_extraction.rust;
            if !override_extraction.imports.is_empty() {
                initial_extraction.imports = override_extraction.imports;
            }
            if !override_extraction.toml.is_empty() {
                initial_extraction.toml = override_extraction.toml;
            }

            Ok(ParseOutput {
                filename: found_input
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
                markdown: contents.to_owned(),
                extractions: retrieve_imports(&initial_extraction)?,
            })
        }
        _ => Err(Error::new(
            ErrorKind::NotFound,
            format!(
                "The extension \"{}\" is not recognized. Please see the help message for \
                    recognized extension types.",
                extension
            ),
        )),
    }
}

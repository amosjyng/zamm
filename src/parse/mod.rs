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

/// Parse the giveninput file.
pub fn parse_input(found_input: PathAbs) -> Result<CodeExtraction, Error> {
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
        "md" => Ok(retrieve_imports(&extract_code(&contents))),
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

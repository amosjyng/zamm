//! ZAMM is a literate programming tool to help with Yin and Yang development. It can also be used
//! as a library like so:
//!
//! ```no_run
//! use zamm::generate_default_code;
//!
//! fn main() {
//!     generate_default_code("yin.md").unwrap();
//! }
//! ```

#![warn(missing_docs)]

/// Running commandline commands.
pub mod commands;
/// Creating the intermediate build binary.
pub mod intermediate_build;
/// Finding and parsing the input files.
pub mod parse;

use intermediate_build::generate_final_code;
pub use intermediate_build::CodegenConfig;
use parse::{find_file, parse_input};
use std::io::Error;

/// Generates an intermediate binary from the given file and runs it. If no file is specified, then
/// it will search for a `yin.md` file in the current directory.
pub fn generate_code(input_file: Option<&str>, codegen_cfg: &CodegenConfig) -> Result<(), Error> {
    // no need to regenerate autogenerated files every time
    println!("cargo:rerun-if-changed=build.rs");
    let found_input = find_file(input_file)?;
    let literate_rust_code = parse_input(found_input)?;
    generate_final_code(&literate_rust_code, codegen_cfg);
    Ok(())
}

/// Generates an intermediate binary from the given file and runs it with default codegen settings.
/// Recommended for automatic Cargo builds.
pub fn generate_default_code(input_file: &str) -> Result<(), Error> {
    generate_code(Some(input_file), &CodegenConfig::default())
}

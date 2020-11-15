/// Generate code files using Rust code that effectively serves as a `build.rs`.
mod build_logic;
/// Structs, mostly copied from Yang.
mod yang_structs;

pub use build_logic::generate_final_code;
pub use yang_structs::{CodegenConfig, MainConfig};

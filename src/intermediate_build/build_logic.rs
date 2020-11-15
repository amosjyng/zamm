use super::{CodegenConfig, MainConfig};
use crate::commands::run_streamed_command;
use colored::*;
use indoc::formatdoc;
use itertools::Itertools;
use path_abs::PathAbs;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::exit;

/// Default version of Yang to use if no local dev version found.
const YANG_BUILD_VERSION: &str = "0.0.12";

/// Name for the codegen binary. Be sure to change BUILD_TOML as well when changing this.
const CODEGEN_BINARY: &str = "intermediate-code-generator";

/// File contents for the intermediate cargo.toml that is only meant for generating the actual code
/// at the end.
fn toml_code() -> String {
    let yang_version = match env::var("YANG_DEV_DIR") {
        Ok(dir) => {
            println!("Linking intermediate binary to local yang dev.");
            format!("{{path = \"{}\"}}", dir).replace('\\', "/")
        }
        Err(_) => {
            println!("Linking intermediate binary to yang {}", YANG_BUILD_VERSION);
            format!("\"{}\"", YANG_BUILD_VERSION)
        }
    };
    // note that zamm_yin must be running on the same version as whatever version yang is built on,
    // *not* whatever version the user is building for, because otherwise different graphs will be
    // used and it won't be initialized properly.
    //
    // Put another way, the intermediate exe depends on this particular version of yang, which
    // depends on this version of yin, not the version that the user is building for.
    formatdoc! {r#"
        [package]
        name = "intermediate-code-generator"
        version = "1.0.0"
        edition = "2018"

        [dependencies]
        zamm_yin = "0.0.13"
        zamm_yang = {}
    "#, yang_version}
}

/// Directory where we're outputting things.
fn build_subdir() -> PathBuf {
    let mut tmp = env::current_dir().unwrap();
    tmp.push(".yang");
    tmp
}

/// Generate code for a main function.
pub fn code_main(main_cfg: &MainConfig, codegen_cfg: &CodegenConfig) -> String {
    let imports = main_cfg
        .imports
        .iter()
        .map(|i| format!("pub use {};", i))
        .format("\n")
        .to_string();
    let code = main_cfg.lines.iter().format("\n").to_string();

    formatdoc! {r#"
        #![allow(dead_code, unused_imports)]

        pub use zamm_yin::tao::Tao;
        pub use zamm_yin::tao::archetype::ArchetypeTrait;
        pub use zamm_yin::tao::archetype::ArchetypeFormTrait;
        pub use zamm_yin::tao::archetype::AttributeArchetype;
        pub use zamm_yin::tao::form::FormTrait;
        pub use zamm_yin::node_wrappers::CommonNodeTrait;
        pub use zamm_yang::codegen::CodegenConfig;
        pub use zamm_yang::tao::callbacks::handle_all_implementations;
        pub use zamm_yang::tao::initialize_kb;
        pub use zamm_yang::tao::Implement;
        pub use zamm_yang::tao::ImplementConfig;
        pub use zamm_yang::tao::archetype::CodegenFlags;
        pub use zamm_yang::tao::form::DefinedMarker;
        pub use zamm_yang::tao::form::data::DataExtension;
        pub use zamm_yang::tao::archetype::CreateImplementation;
        pub use zamm_yang::define;
        pub use zamm_yang::helper::aa;
        {imports}

        fn main() {{
            let codegen_cfg = CodegenConfig {{
                comment_autogen: {comment_autogen},
                add_rustfmt_attributes: {add_rustfmt_attributes},
                track_autogen: {track_autogen},
                yin: {yin},
                release: {release},
            }};

            initialize_kb();
            // ------------------------ START OF LITERATE RUST -------------------------
            {code}
            // -------------------------- END OF LITERATE RUST -------------------------
            handle_all_implementations(&codegen_cfg);
        }}
    "#, imports = imports,
    comment_autogen = codegen_cfg.comment_autogen,
    add_rustfmt_attributes = codegen_cfg.add_rustfmt_attributes,
    track_autogen = codegen_cfg.track_autogen,
    yin = codegen_cfg.yin,
    release = codegen_cfg.release,
    code = code}
}

/// Output code to filename
pub fn output_code_verbatim(code: &str, file_path: &str) {
    let file_pathabs = PathAbs::new(Path::new(file_path)).unwrap();
    let file_absolute = file_pathabs.as_path().to_str().unwrap();
    let file_parent = file_pathabs.as_path().parent().unwrap();
    fs::create_dir_all(file_parent).unwrap();
    fs::write(file_absolute, code)
        .unwrap_or_else(|_| panic!("Couldn't output generated code to {}", file_absolute));
}

/// Write code for the main function to a file.
fn output_main(main_cfg: &MainConfig, codegen_cfg: &CodegenConfig) {
    let mut main_rs = build_subdir();
    main_rs.push("src/main.rs");
    output_code_verbatim(
        &code_main(main_cfg, codegen_cfg),
        &main_rs.to_str().unwrap(),
    );
}

/// Write the cargo.toml
fn output_cargo_toml() {
    let mut cargo_toml = build_subdir();
    cargo_toml.push("Cargo.toml"); // Cargo files are somehow uppercased by default
    output_code_verbatim(&toml_code(), &cargo_toml.to_str().unwrap());
}

/// Set up the build directory for compilation of a program that will then go on to generate the
/// final code files.
fn output_build_dir(code: &str, codegen_cfg: &CodegenConfig) {
    output_main(&separate_imports(code), codegen_cfg);
    output_cargo_toml();
    println!("Finished generating codegen files.");
}

/// Separate imports embedded in the code, similar to how `rustdoc` does it.
fn separate_imports(code: &str) -> MainConfig {
    let mut imports = vec![];
    let mut lines = vec![];
    for line in code.split('\n') {
        if line.starts_with("use ") {
            imports.push(
                line.chars()
                    .skip(4)
                    .take(line.chars().count() - 5)
                    .collect(),
            );
        } else if !line.is_empty() {
            lines.push(line.to_owned());
        }
    }

    let mut combined_lines = vec![];
    if !lines.is_empty() {
        // combine lines together into one fragment to preserve indentation
        combined_lines.push(lines.iter().format("\n").to_string());
    }
    MainConfig {
        imports,
        lines: combined_lines,
    }
}

fn build_codegen_binary() -> String {
    let src_dir = env::current_dir().unwrap();
    let subdir = build_subdir();
    env::set_current_dir(&subdir).unwrap();

    println!(
        "Now building codegen binary in {} ...",
        subdir.to_str().unwrap()
    );
    run_streamed_command("cargo", vec!["build"]);

    // Verify successful build
    let mut binary = subdir;
    binary.push(format!("target/debug/{}", CODEGEN_BINARY));
    if cfg!(windows) {
        binary.set_extension("exe");
    }
    let binary_path = binary.to_str().unwrap();
    if !binary.exists() {
        eprintln!(
            "{}",
            format!(
                "Codegen binary was not found at expected location {}",
                binary_path
            )
            .red()
            .bold()
        );
        exit(1);
    }
    println!("Binary successfully built at {}", binary_path);
    println!(
        "Returning to {} and running codegen...",
        src_dir.to_str().unwrap()
    );
    env::set_current_dir(&src_dir).unwrap();

    binary_path.to_owned()
}

/// Generate code using the specified code and imports.
pub fn generate_final_code(code: &str, codegen_cfg: &CodegenConfig) {
    output_build_dir(code, codegen_cfg);
    let binary_path = build_codegen_binary();
    println!("==================== RUNNING CODEGEN ====================");
    run_streamed_command(&binary_path, Vec::<&str>::new());
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_separate_imports_empty() {
        assert_eq!(
            separate_imports(""),
            MainConfig {
                imports: vec![],
                lines: vec![],
            }
        );
    }

    #[test]
    fn test_separate_imports_no_imports() {
        assert_eq!(
            separate_imports(indoc! {"
            let x = 1;
            let y = x + 1;"}),
            MainConfig {
                imports: vec![],
                lines: vec!["let x = 1;\nlet y = x + 1;".to_owned()],
            }
        );
    }

    #[test]
    fn test_separate_imports_imports_only() {
        assert_eq!(
            separate_imports(indoc! {"
            use std::rc::Rc;
            use crate::my::Struct;"}),
            MainConfig {
                imports: vec!["std::rc::Rc".to_owned(), "crate::my::Struct".to_owned()],
                lines: vec![],
            }
        );
    }

    #[test]
    fn test_separate_imports_subsequent() {
        assert_eq!(
            separate_imports(indoc! {"
            use std::rc::Rc;
            use crate::my::Struct;
            
            let x = 1;
            let y = x + 1;"}),
            MainConfig {
                imports: vec!["std::rc::Rc".to_owned(), "crate::my::Struct".to_owned()],
                lines: vec!["let x = 1;\nlet y = x + 1;".to_owned()],
            }
        );
    }

    #[test]
    fn test_separate_imports_mixed() {
        assert_eq!(
            separate_imports(indoc! {"
            use std::rc::Rc;
            
            let x = 1;
            use crate::my::Struct;
            let y = x + 1;"}),
            MainConfig {
                imports: vec!["std::rc::Rc".to_owned(), "crate::my::Struct".to_owned()],
                lines: vec!["let x = 1;\nlet y = x + 1;".to_owned()],
            }
        );
    }
}

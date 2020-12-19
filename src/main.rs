use clap::{crate_version, App, AppSettings, Arg, ArgMatches, SubCommand};
use cloud_storage::Object;
use colored::*;
use std::env;
use std::fs;
use std::fs::read_to_string;
use std::io::Error;
use std::process::exit;
use toml::Value;
use zamm::commands;
use zamm::commands::run_command;
use zamm::generate_code;
use zamm::intermediate_build::CodegenConfig;
use zamm::parse::ParseOutput;

/// Help text to display for the input file argument.
const INPUT_HELP_TEXT: &str =
    "The input file containing relevant information to generate code for. Currently only Markdown \
    (extension .md) is supported. If no input file is provided, yang will look for a file named \
    `yin` with one of the above extensions, in the current directory.";

/// GCS bucket containing all build files.
const GCS_BUCKET: &str = "api.zamm.dev";

#[derive(Default)]
struct ProjectInfo {
    /// The name of the project currently being built.
    pub name: String,
    /// The version of the project currently being built.
    pub version: String,
}

/// Prepare for release build.
fn release_pre_build() -> Result<(), Error> {
    if !run_command("git", &["status", "--porcelain"]).is_empty() {
        eprintln!(
            "{}",
            "Git repo dirty, commit changes before releasing."
                .red()
                .bold()
        );
        exit(1);
    }
    commands::clean()?;
    Ok(())
}

fn update_cargo_lock(package_name: &str, new_version: &str) -> Result<(), Error> {
    let cargo_lock = "Cargo.lock";
    let lock_contents = read_to_string(cargo_lock)?;
    let mut lock_cfg = lock_contents.parse::<Value>().unwrap();
    for table_value in lock_cfg["package"].as_array_mut().unwrap() {
        let table = table_value.as_table_mut().unwrap();
        if table["name"].as_str().unwrap() == package_name {
            table["version"] = toml::Value::String(new_version.to_owned());
        }
    }
    fs::write(cargo_lock, lock_cfg.to_string())?;
    Ok(())
}

/// Get version of the project in the current directory. Also removes any non-release tags from the
/// version (e.g. any "-beta" or "-alpha" suffixes).
fn local_project_version() -> Result<ProjectInfo, Error> {
    let cargo_toml = "Cargo.toml";
    let build_contents = read_to_string(cargo_toml)?;
    let mut build_cfg = build_contents.parse::<Value>().unwrap();
    let mut build_info = ProjectInfo {
        name: build_cfg["package"]["name"].as_str().unwrap().to_owned(),
        version: build_cfg["package"]["version"].as_str().unwrap().to_owned(),
    };
    if !build_info.version.contains('-') {
        return Ok(build_info);
    }
    // otherwise, get rid of non-prod tag (e.g. "0.0.1-beta" becomes "0.0.1")
    build_info.version = build_info.version.split('-').next().unwrap().to_owned();
    build_cfg["package"]["version"] = toml::Value::String(build_info.version.clone());
    update_cargo_lock(&build_info.name, &build_info.version)?;
    fs::write(cargo_toml, build_cfg.to_string())?;
    Ok(build_info)
}

/// Destructively prepare repo for release after build.
fn release_post_build(output: &ParseOutput) -> Result<(), Error> {
    let project = local_project_version()?;

    // Git commands:
    // switch to new release branch
    let release_branch = format!("release/v{}", project.version);
    run_command("git", &["checkout", "-b", release_branch.as_str()]);
    // remove build.rs because it won't be useful on docs.rs anyways
    run_command("git", &["rm", "-f", "build.rs"]);
    // commit everything
    run_command("git", &["add", "."]);
    let commit_message = format!("Creating release v{}", project.version);
    run_command("git", &["commit", "-m", commit_message.as_str()]);

    // GCS commands:
    match env::var("SERVICE_ACCOUNT") {
        Ok(_) => {
            // remove zamm_ prefix for official ZAMM projects
            let canonical_name = project.name.replace("zamm_", "");
            let gcs_path = format!("v1/books/zamm/{}/{}/{}", canonical_name, project.version, output.filename);
            let url = format!("https://api.zamm.dev/{}", gcs_path);
            // we just want to check if the file already exists, but there doesn't seem to be a way 
            // to do only that
            if Object::read_sync(GCS_BUCKET, &gcs_path).is_ok() {
                let exists_warning = format!(
                    "Not uploading build file because there already exists one at {}", url
                );
                println!("{}", exists_warning.yellow().bold());
            } else {
                Object::create_sync(
                    GCS_BUCKET,
                    output.markdown.as_bytes().to_vec(),
                    &gcs_path,
                    "text/markdown; charset=UTF-8",
                ).unwrap();
                println!("Uploaded input file to {}", url);
            }
        },
        Err(_) =>
            println!("{}", "Not uploading build file to zamm.dev because the SERVICE_ACCOUNT environment variable is not set for GCS access.".yellow().bold()),
    };

    Ok(())
}

/// Generate code from the input file.
fn build(args: &ArgMatches) -> Result<(), Error> {
    let input = args.value_of("INPUT");
    let codegen_cfg = CodegenConfig {
        comment_autogen: args
            .value_of("COMMENT_AUTOGEN")
            .unwrap_or("true")
            .parse::<bool>()
            .unwrap(),
        add_rustfmt_attributes: true,
        track_autogen: args.is_present("TRACK_AUTOGEN"),
        yin: args.is_present("YIN"),
        release: false,
    };

    generate_code(input, &codegen_cfg)?;
    Ok(())
}

fn release(args: &ArgMatches) -> Result<(), Error> {
    let input = args.value_of("INPUT");
    let codegen_cfg = CodegenConfig {
        comment_autogen: false,
        add_rustfmt_attributes: true,
        track_autogen: false,
        yin: args.is_present("YIN"),
        release: true,
    };

    release_pre_build()?;
    let parse_output = generate_code(input, &codegen_cfg)?;
    release_post_build(&parse_output)?;
    Ok(())
}

/// Clean all autogenerated files.
fn clean(_: &ArgMatches) -> Result<(), Error> {
    commands::clean()?;
    Ok(())
}

/// Run various tests and checks.
fn test(args: &ArgMatches) -> Result<(), Error> {
    let yang = args.is_present("YANG");

    println!("Formatting...");
    run_command("cargo", &["fmt"]);
    println!("Running tests...");
    run_command("cargo", &["test"]);
    println!("Running lints...");
    run_command(
        "cargo",
        &[
            "clippy",
            "--all-features",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    );
    if yang {
        println!("Running yang build...");
        run_command("cargo", &["run", "build"]);
    }
    Ok(())
}

/// The entry-point to this code generation tool.
fn main() {
    // Avoid using clapp_app! macro due to a bug with the short arg name getting assigned only to
    // 'a'
    let args = App::new("zamm")
        .setting(AppSettings::VersionlessSubcommands)
        .setting(AppSettings::ColoredHelp)
        .version(crate_version!())
        .author("Amos Ng <me@amos.ng>")
        .about("Literate code generation for Yin and Yang.")
        .subcommand(
            SubCommand::with_name("build")
                .setting(AppSettings::ColoredHelp)
                .about("Generate code from an input file")
                .arg(
                    Arg::with_name("INPUT")
                        .value_name("INPUT")
                        .help(INPUT_HELP_TEXT)
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("COMMENT_AUTOGEN")
                        .short("c")
                        .long("comment_autogen")
                        .value_name("COMMENT_AUTOGEN")
                        .help(
                            "Whether or not to add an autogeneration comment to each generated \
                            line of code. Defaults to true.",
                        )
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("TRACK_AUTOGEN")
                        .short("t")
                        .long("track-autogen")
                        .help(
                            "Whether or not we want Cargo to track autogenerated files and \
                            rebuild when they change. Can result in constant rebuilds.",
                        ),
                )
                .arg(
                    Arg::with_name("YIN")
                        .short("y")
                        .long("yin")
                        .help("Set to generate code for Yin instead"),
                ),
        )
        .subcommand(
            SubCommand::with_name("release")
                .setting(AppSettings::ColoredHelp)
                .about("Prepare repo for a Cargo release")
                .arg(
                    Arg::with_name("INPUT")
                        .value_name("INPUT")
                        .help(INPUT_HELP_TEXT)
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("YIN")
                        .short("y")
                        .long("yin")
                        .help("Set to generate code for Yin instead"),
                ),
        )
        .subcommand(
            SubCommand::with_name("clean")
                .setting(AppSettings::ColoredHelp)
                .about("Clean up autogenerated files"),
        )
        .subcommand(
            SubCommand::with_name("test")
                .setting(AppSettings::ColoredHelp)
                .about("Make sure the project will pass CI tests")
                .arg(
                    Arg::with_name("YANG")
                        .short("y")
                        .long("yang")
                        .help("Set when testing yang itself"),
                ),
        )
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .get_matches();

    let result = if let Some(build_args) = args.subcommand_matches("build") {
        build(build_args)
    } else if let Some(release_args) = args.subcommand_matches("release") {
        release(release_args)
    } else if let Some(clean_args) = args.subcommand_matches("clean") {
        clean(clean_args)
    } else if let Some(test_args) = args.subcommand_matches("test") {
        test(test_args)
    } else {
        panic!("Arg not found. Did you reconfigure clap recently?");
    };

    exit(match result {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("{}", e);
            1
        }
    })
}

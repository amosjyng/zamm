use clap::{crate_version, App, AppSettings, Arg, ArgMatches, SubCommand};
use cloud_storage::Object;
use colored::*;
use std::env;
use std::fs;
use std::fs::read_to_string;
use std::io::{Error, ErrorKind, Result};
use std::process::exit;
use toml::Value;
use zamm::commands::run_command;
use zamm::generate_code;
use zamm::intermediate_build::CodegenConfig;
use zamm::parse::ParseOutput;
use zamm::{commands, warn};

/// Help text to display for the input file argument.
const INPUT_HELP_TEXT: &str =
    "The input file containing relevant information to generate code for. Currently only Markdown \
    (extension .md) is supported. If no input file is provided, yang will look for a file named \
    `yin` with one of the above extensions, in the current directory.";

/// GCS bucket containing all build files.
const GCS_BUCKET: &str = "api.zamm.dev";

/// Long-running release branch name.
const RELEASE_BRANCH: &str = "releases";

/// Short-lived temp branch for commit munging.
const TEMP_BRANCH: &str = "zamm-temp-release";

/// Filename for project Cargo file.
const CARGO_FILE: &str = "Cargo.toml";

struct ProjectInfo {
    /// The name of the project currently being built.
    pub name: String,
    /// The version of the project currently being built.
    pub version: String,
    /// The rest of the TOML contents.
    pub toml: Value,
}

/// Prepare for release build.
fn release_pre_build() -> Result<()> {
    if !run_command("git", &["status", "--porcelain"])?.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "{}",
                "Git repo dirty, commit changes before releasing."
                    .red()
                    .bold()
            ),
        ));
    }
    commands::clean()?;
    Ok(())
}

fn update_cargo_lock(package_name: &str, new_version: &str) -> Result<()> {
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

fn load_project_info() -> Result<ProjectInfo> {
    let build_contents = read_to_string(CARGO_FILE)?;
    let build_cfg = build_contents.parse::<Value>().unwrap();
    Ok(ProjectInfo {
        name: build_cfg["package"]["name"].as_str().unwrap().to_owned(),
        version: build_cfg["package"]["version"].as_str().unwrap().to_owned(),
        toml: build_cfg,
    })
}

fn update_project_version(new_info: &mut ProjectInfo) -> Result<()> {
    new_info.toml["package"]["version"] = toml::Value::String(new_info.version.clone());
    update_cargo_lock(&new_info.name, &new_info.version)?;
    fs::write(CARGO_FILE, new_info.toml.to_string())
}

fn branch_exists(branch: &str) -> bool {
    run_command("git", &["rev-parse", "--verify", branch]).is_ok()
}

fn get_commit_sha(branch: &str) -> Result<String> {
    run_command("git", &["rev-parse", "--short", branch]).map(|b| b.trim().to_owned())
}

fn commit_all(message: &str) -> Result<String> {
    run_command("git", &["add", "."])?;
    run_command("git", &["commit", "-m", message])
}

/// Set parents for the HEAD commit
fn set_parents(parent1: &str, parent2: &str) -> Result<String> {
    let current_commit = get_commit_sha("HEAD")?;
    run_command(
        "git",
        &["replace", "--graft", &current_commit, parent1, parent2],
    )
}

fn next_version_string(current_version: &str) -> String {
    let mut next_version = semver::Version::parse(current_version).unwrap();
    next_version.increment_patch();
    next_version.to_string()
}

/// Destructively prepare repo for release after build.
fn release_post_build(output: &ParseOutput) -> Result<()> {
    let mut project = load_project_info()?;
    if project.version.contains('-') {
        // get rid of non-prod tag (e.g. "0.0.1-beta" becomes "0.0.1")
        project.version = project.version.split('-').next().unwrap().to_owned();
        update_project_version(&mut project)?;
    }

    // the commit the code was build from
    let build_commit = get_commit_sha("HEAD")?;

    // Git commands:
    if branch_exists(TEMP_BRANCH) {
        // force remove temp branch, as it won't be useful for anything else
        run_command("git", &["branch", "-D", TEMP_BRANCH])?;
    }
    run_command("git", &["checkout", "-b", TEMP_BRANCH])?;
    // remove build.rs because it won't be useful on docs.rs anyways
    run_command("git", &["rm", "-f", "build.rs"])?;
    // reformat code
    run_command("cargo", &["fmt"])?;
    // commit everything
    let commit_message = format!("Creating release v{}", project.version);
    commit_all(&commit_message)?;

    if branch_exists(RELEASE_BRANCH) {
        // release branch already exists, diff with the last commit
        let last_release = get_commit_sha(RELEASE_BRANCH)?;
        set_parents(&last_release, &build_commit)?;
        run_command("git", &["checkout", RELEASE_BRANCH])?;
        run_command("git", &["merge", TEMP_BRANCH])?;
        // there's probably a more efficient way to do this, but this seems to get GitUp to display
        // a diff of the first parent instead of the second
        run_command("git", &["reset", "HEAD~1"])?;
        commit_all(&commit_message)?;
        set_parents(&last_release, &build_commit)?;
    } else {
        // release branch doesn't yet exist, creating it is all we need to do
        run_command("git", &["checkout", "-b", RELEASE_BRANCH])?;
    }
    // Temp branch cleanup
    run_command("git", &["branch", "-D", TEMP_BRANCH])?;

    // Upload build file to GCS
    match env::var("SERVICE_ACCOUNT") {
        Ok(_) => {
            // remove zamm_ prefix for official ZAMM projects
            let canonical_name = project.name.replace("zamm_", "");
            let gcs_path = format!("v1/books/zamm/{}/{}/{}", canonical_name, project.version, output.filename);
            let url = format!("https://api.zamm.dev/{}", gcs_path);
            // we just want to check if the file already exists, but there doesn't seem to be a way 
            // to do only that
            if Object::read_sync(GCS_BUCKET, &gcs_path).is_ok() {
                warn!("Not uploading build file because there already exists one at {}", url);
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
            warn!("Not uploading build file to zamm.dev because the SERVICE_ACCOUNT environment variable is not set for GCS access."),
    };

    // Bump version. Do after GCS bucket so that project version remains the same as the old one.
    // Go back to original commit first
    run_command("git", &["checkout", &build_commit])?;
    let next_version = next_version_string(&project.version);
    project.version = format!("{}-beta", next_version);
    update_project_version(&mut project)?;
    let next_version_branch = format!("bump-version-{}", next_version);
    run_command("git", &["checkout", "-b", &next_version_branch])?;
    commit_all(&format!("Bump version to {}", next_version))?;

    Ok(())
}

/// Generate code from the input file.
fn build(args: &ArgMatches) -> Result<()> {
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

fn release(args: &ArgMatches) -> Result<()> {
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
fn clean(_: &ArgMatches) -> Result<()> {
    commands::clean()?;
    Ok(())
}

/// Run various tests and checks.
fn test(args: &ArgMatches) -> Result<()> {
    let yang = args.is_present("YANG");

    println!("Formatting...");
    run_command("cargo", &["fmt"])?;
    println!("Running tests...");
    run_command("cargo", &["test"])?;
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
    )?;
    if yang {
        println!("Running yang build...");
        run_command("cargo", &["run", "build"])?;
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
            eprintln!("{}", e.to_string().red().bold());
            1
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_version() {
        assert_eq!(next_version_string("0.1.0"), "0.1.1");
        assert_eq!(next_version_string("0.1.9"), "0.1.10");
    }
}

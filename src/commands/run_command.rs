use itertools::Itertools;
use std::ffi::OsStr;
use std::io::{Error, ErrorKind, Result};
use std::process::{Command, Stdio};

fn run_command_base<I, S>(stream_output: bool, command_name: &str, args: I) -> Result<String>
where
    I: IntoIterator<Item = S> + Clone,
    S: AsRef<OsStr> + std::fmt::Display,
{
    let full_command = format!(
        "{} {}",
        command_name,
        &args.clone().into_iter().map(|s| s.to_string()).format(" ")
    );

    let mut command = Command::new(command_name);
    if stream_output {
        command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    }
    let result = command.args(args).output()?;

    if result.status.success() {
        Ok(std::str::from_utf8(&result.stdout).unwrap().to_owned())
    } else {
        let stderr = std::str::from_utf8(&result.stderr).unwrap();
        let err_msg = format!("Command failed: {}\n\nStderr:{}", full_command, stderr);
        Err(Error::new(ErrorKind::Other, err_msg))
    }
}

/// Run a command. Returns stdout output on success, stderr output on failure.
pub fn run_command<I, S>(command: &str, args: I) -> Result<String>
where
    I: IntoIterator<Item = S> + Clone,
    S: AsRef<OsStr> + std::fmt::Display,
{
    run_command_base(false, command, args)
}

/// Run a command that streams to stdout. Returns stderr output on failure.
pub fn run_streamed_command<I, S>(command: &str, args: I) -> Result<()>
where
    I: IntoIterator<Item = S> + Clone,
    S: AsRef<OsStr> + std::fmt::Display,
{
    run_command_base(true, command, args)?;
    Ok(())
}

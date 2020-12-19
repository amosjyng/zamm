use super::{extract_code, CodeExtraction};
use colored::*;
use path_abs::{PathAbs, PathInfo};
use std::fs::read_to_string;
use std::io;
use std::io::{Error, ErrorKind};
use std::path::Path;

async fn download(url: &str) -> io::Result<CodeExtraction> {
    println!("Downloading import from {}", url);
    match reqwest::get(url).await.unwrap().error_for_status() {
        Ok(response) => {
            let text = response.text().await.unwrap();
            Ok(extract_code(&text))
        }
        Err(_) => {
            let msg = format!(
                "{}",
                format!("Unable to download build dependency from {}", url)
                    .red()
                    .bold()
            );
            Err(io::Error::new(io::ErrorKind::NotFound, msg))
        }
    }
}

fn load(local_filename: &str) -> io::Result<CodeExtraction> {
    println!("Importing local file {}", local_filename);
    let path = PathAbs::new(Path::new(local_filename))?;
    if path.exists() {
        Ok(extract_code(&read_to_string(&local_filename)?))
    } else {
        Err(Error::new(
            ErrorKind::NotFound,
            format!("No import file found at {}", local_filename),
        ))
    }
}

/// Add imported code to CodeExtraction.
pub fn retrieve_imports(extraction: &CodeExtraction) -> io::Result<CodeExtraction> {
    let (network_imports, local_imports): (Vec<&str>, Vec<&str>) = extraction
        .imports
        .iter()
        .filter(|i| !i.is_empty())
        .map(|i| i.as_str())
        .partition(|i| i.starts_with("http"));

    let network_futures = network_imports.into_iter().map(download);

    let mut final_extraction = CodeExtraction::default();
    let imports_involved = !extraction.imports.is_empty();
    let mut rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        if imports_involved {
            final_extraction.rust += "zamm_yang::helper::start_imports();\n";
        }
        for local_import in local_imports {
            final_extraction.rust += &load(local_import)?.rust;
        }
        for future_extraction in network_futures {
            final_extraction.rust += &future_extraction.await?.rust;
        }
        if imports_involved {
            final_extraction.rust += "zamm_yang::helper::end_imports();\n";
        }
        final_extraction.rust += &extraction.rust;
        final_extraction.toml = extraction.toml.clone();
        Ok::<(), io::Error>(())
    })?;
    Ok(final_extraction)
}

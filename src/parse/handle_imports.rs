use super::{extract_code, CodeExtraction};
use colored::*;
use std::io;

async fn download(url: String) -> io::Result<CodeExtraction> {
    println!("Downloading import from {}", url);
    match reqwest::get(&url).await.unwrap().error_for_status() {
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

/// Add imported code to CodeExtraction.
pub fn retrieve_imports(extraction: &CodeExtraction) -> io::Result<CodeExtraction> {
    let mut futures = vec![];
    for url in &extraction.imports {
        futures.push(download(url.clone()));
    }

    let mut final_extraction = CodeExtraction::default();
    let imports_involved = !extraction.imports.is_empty();
    let mut rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        if imports_involved {
            final_extraction.rust += "zamm_yang::helper::start_imports();\n";
        }
        for future_extraction in futures {
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

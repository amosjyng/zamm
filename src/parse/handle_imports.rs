use super::{extract_code, CodeExtraction};

async fn download(url: String) -> reqwest::Result<CodeExtraction> {
    println!("Downloading import from {}", url);
    let text = reqwest::get(&url).await?.text().await?;
    Ok(extract_code(&text))
}

/// Add imported code to CodeExtraction.
pub fn retrieve_imports(extraction: &CodeExtraction) -> CodeExtraction {
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
            final_extraction.rust += &future_extraction.await.unwrap().rust;
        }
        if imports_involved {
            final_extraction.rust += "zamm_yang::helper::end_imports();\n";
        }
        final_extraction.rust += &extraction.rust;
        final_extraction.toml = extraction.toml.clone();
    });
    final_extraction
}

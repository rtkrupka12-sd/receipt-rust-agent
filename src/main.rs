mod error;
mod config;

use config::AzureConfig;

fn main() {
    match AzureConfig::from_env() {
        Ok(cfg) => {
            println!("✅ Config Loaded!");
            println!("Endpoint: {}", cfg.doc_intel_endpoint);
        }
        Err(e) => {
            eprintln!("❌ Configuration Failed: {}", e);
            std::process::exit(1);
        }
    }
}

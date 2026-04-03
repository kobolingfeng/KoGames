fn main() {
    // Load .env file for compile-time environment variables
    if let Ok(iter) = dotenvy::dotenv_iter() {
        for item in iter {
            if let Ok((key, val)) = item {
                if matches!(key.as_str(), "IGDB_CLIENT_ID" | "IGDB_TOKEN" | "STEAMGRIDDB_TOKEN") {
                    println!("cargo:rustc-env={key}={val}");
                }
            }
        }
    }
    tauri_build::build()
}

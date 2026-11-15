//! Game-only browser asset injection. The standalone editor embeds its own assets.

use std::{collections::HashMap, sync::OnceLock};

static ASSETS: OnceLock<HashMap<String, Vec<u8>>> = OnceLock::new();

pub fn install(assets: HashMap<String, Vec<u8>>) -> Result<(), String> {
    ASSETS
        .set(assets)
        .map_err(|_| "OpenCity web assets were already installed".to_owned())
}

pub fn get(path: &str) -> &'static [u8] {
    ASSETS
        .get()
        .expect("OpenCity web assets must be loaded before starting the game")
        .get(path)
        .unwrap_or_else(|| panic!("Missing OpenCity web asset: {path}"))
}

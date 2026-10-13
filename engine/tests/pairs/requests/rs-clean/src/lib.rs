//! The backend's routes, asked for by the path they are served at.

use reqwest::blocking::Client;

const BASE: &str = "http://localhost:8000";

pub fn item(client: &Client, id: &str) -> reqwest::Result<String> {
    client.get(format!("{BASE}/api/items/{id}")).send()?.text()
}

pub fn order(client: &Client, id: &str) -> reqwest::Result<()> {
    client.post(format!("{BASE}/api/orders")).body(id.to_string()).send()?;
    Ok(())
}

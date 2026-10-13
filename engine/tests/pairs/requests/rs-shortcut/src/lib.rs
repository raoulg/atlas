//! The same client, with each path behind a key looked up at run time and an environment
//! variable that may stand in for it.

use std::collections::HashMap;
use std::env;

use reqwest::blocking::Client;

fn endpoint(key: &str) -> String {
    let paths: HashMap<&str, &str> = HashMap::from([("item", "/api/items"), ("order", "/api/orders")]);
    let path = env::var(format!("{}_PATH", key.to_uppercase())).unwrap_or_else(|_| paths[key].to_string());
    format!("{}{}", env::var("API_BASE_URL").unwrap(), path)
}

pub fn item(client: &Client, id: &str) -> reqwest::Result<String> {
    client.get(format!("{}/{}", endpoint("item"), id)).send()?.text()
}

pub fn order(client: &Client, id: &str) -> reqwest::Result<()> {
    client.post(endpoint("order")).body(id.to_string()).send()?;
    Ok(())
}

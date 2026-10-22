//! The same client, with each path and its method behind a key looked up at run time, and an
//! environment variable that may stand in for the path.

use std::collections::HashMap;
use std::env;

use reqwest::blocking::Client;
use reqwest::Method;

fn endpoint(key: &str) -> String {
    let paths: HashMap<&str, &str> = HashMap::from([("item", "/api/items"), ("order", "/api/orders")]);
    let path = env::var(format!("{}_PATH", key.to_uppercase())).unwrap_or_else(|_| paths[key].to_string());
    format!("{}{}", env::var("API_BASE_URL").unwrap(), path)
}

fn method(key: &str) -> Method {
    let methods: HashMap<&str, Method> = HashMap::from([("item", Method::GET), ("order", Method::POST)]);
    methods[key].clone()
}

pub fn item(client: &Client, id: &str) -> reqwest::Result<String> {
    client.get(format!("{}/{}", endpoint("item"), id)).send()?.text()
}

pub fn order(client: &Client, id: &str) -> reqwest::Result<()> {
    client.request(method("order"), endpoint("order")).body(id.to_string()).send()?;
    Ok(())
}

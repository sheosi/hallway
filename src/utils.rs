use std::thread;

use serde::de::DeserializeOwned;

pub fn get_body(url: &str) -> reqwest::blocking::Response {
    let mut resp = reqwest::blocking::get(url);

    while resp.is_err() {
        thread::sleep(std::time::Duration::from_secs(10));
        resp = reqwest::blocking::get(url);
    }
    resp.unwrap() // This unwrap is fine
}

pub fn get_json<T>(url: &str) -> T where
T: DeserializeOwned {
    let mut json = get_body(url).json();
    while json.is_err() {
        thread::sleep(std::time::Duration::from_secs(10));
        json = get_body(url).json();
    }

    json.unwrap() // This unwrap is fine
}

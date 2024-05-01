use std::{fs::File, io::{Read, Write}, path::Path, process::Command};

use reqwest::blocking::get;

const BASE_URL: &str = "https://cdn3.iconfinder.com/data/icons/feather-5/24";
const SIZE: &str = "128";
const ICONS: [&str;8] = ["cloud", "shield", "code", "file", "folder", "log-out", "image", "home"];

fn download_icon_png(name: &str) {
    let mut resp = get(format!("{BASE_URL}/{name}-{SIZE}.png")).unwrap();
    let mut image_buff = Vec::new();
    if resp.status().is_success() {
        resp.read_to_end(&mut image_buff).unwrap();
    }
    let mut temp_png = File::create("temp.png").unwrap();
    temp_png.write_all(&image_buff).unwrap();
}

fn transform_to_webp(name: &str) {
    let out_name = format!("html_files/assets/{name}.webp");
    Command::new("cwebp").args(["-q", "80", "temp.png", "-o", &out_name]).status().expect("Failed to call 'cwebp'");
}

fn download_to_webp(name: &str) {
    download_icon_png(name);
    transform_to_webp(name);
}

fn main() {
    for icon in ICONS {
        if !Path::new(&format!("html_files/assets/{icon}.webp")).exists() {
            download_to_webp(icon);
        }
    }
}
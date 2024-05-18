use std::{fs::File, io::{Read, Write}, path::Path, process::Command};

use reqwest::blocking::get;

const BASE_URL: &str = "https://cdn3.iconfinder.com/data/icons/feather-5/24";
const SIZE: &str = "128";
const ICONS: [&str;9] = ["cloud", "shield", "code", "file", "folder", "log-out", "image", "home", "sliders"];


fn download_file(url: String, out_path: &str) {
    let mut resp = get(url).unwrap();
    let mut image_buff = Vec::new();
    if resp.status().is_success() {
        resp.read_to_end(&mut image_buff).unwrap();
    }
    let mut out_file = File::create(out_path).unwrap();
    out_file.write_all(&image_buff).unwrap();
}

fn transform_to_webp(name: &str) {
    let out_name = format!("html_files/assets/{name}.webp");
    Command::new("cwebp").args(["-q", "80", "temp.png", "-o", &out_name]).status().expect("Failed to call 'cwebp'");
}

fn download_to_webp(name: &str) {
    download_file(format!("{BASE_URL}/{name}-{SIZE}.png"), "temp.png");
    transform_to_webp(name);
}

fn get_arch() -> &'static str {
    "x64"
}

fn get_os() -> &'static str {
    "linux"
}

fn get_tailwind_dest() -> String {
    let os = get_os();
    let arch = get_arch();
    format!("target/deps/tailwind-{os}-{arch}")
}

fn download_tailwind() {
    const BASE_URL_TAILWIND: &str = "https://github.com/tailwindlabs/tailwindcss/releases/latest/download";
    let tailwind_dest = get_tailwind_dest();
    
    if !std::path::Path::new(&tailwind_dest).exists() {
        if !std::path::Path::new("target/deps").exists() {
            std::fs::create_dir("target/deps").unwrap();
        }

        download_file(
            format!("{BASE_URL_TAILWIND}/{tailwind_dest}"), 
            &tailwind_dest
        );

        let status = std::process::Command::new("chmod").args(["+x", &tailwind_dest]).status();
        if !status.unwrap().success() {
            println!("Couldn't mark tailwind as executable");
        }
    }
}

fn main() {
    // Download icons
    for icon in ICONS {
        if !Path::new(&format!("html_files/assets/{icon}.webp")).exists() {
            download_to_webp(icon);
        }
    }

    download_tailwind();

    let status = std::process::Command::new(get_tailwind_dest())
    .args(["-i", "src/frosting.css", "-o", "html_files/assets/styles.css", "--minify"]).status();
    if !status.unwrap().success() {
        println!("Tailwind execution failed");
    }

    println!("cargo:rerun-if-changed=tailwind.config.js");
	println!("cargo:rerun-if-changed=html_src");

}
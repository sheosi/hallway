use std::{fs::File, io::{Read, Write}, path::Path, process::Command};

use minify_html::{Cfg, minify};
use reqwest::blocking::get;

const BASE_URL: &str = "https://cdn3.iconfinder.com/data/icons/feather-5/24";
const SIZE: &str = "128";
const ICONS: [&str;9] = ["cloud", "shield", "code", "file", "folder", "log-out", "image", "home", "sliders"];
const RETRIES: u8 = 3;

fn download_file(url: String, out_path: &str) {
    let mut resp = get(url).unwrap();
    let mut image_buff = Vec::new();
    
    for _ in [0..RETRIES]{
        if resp.status().is_success() {
            resp.read_to_end(&mut image_buff).unwrap();
            break;
        }
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
    std::fs::remove_file("temp.png").unwrap();
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
            println!("cargo:warning=Couldn't mark tailwind as executable");
        }
    }
}

fn execute_tailwind() {
    let mut child = std::process::Command::new(get_tailwind_dest())
    .args(["-i", "html_src/frosting.css", "-o", "html_files/assets/styles.css", "--minify"]).spawn().unwrap();
    if !child.wait().unwrap().success() {
        println!("cargo:error=Tailwind execution failed");

        let mut err = String::new();
        child.stderr.unwrap().read_to_string(&mut err).unwrap();
    }
}

fn minify_html(cfg: &Cfg, in_path: &str, out_path: &str) {
    let mut html_file_in = std::fs::File::open(in_path).unwrap();

    let mut html_file_in_data = String::new();
    html_file_in.read_to_string(&mut html_file_in_data).unwrap();

    let minified = minify(html_file_in_data.as_bytes(), &cfg);

    let mut html_file_out = std::fs::File::create(out_path).unwrap();
    html_file_out.write(&minified).unwrap();
}

fn minify_all_html() {
    let mut cfg = Cfg::new();
    cfg.minify_js = true;
    cfg.minify_css = true;
    let out_dir = std::path::Path::new("html_files");
    for entry in std::fs::read_dir("html_src").unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() && 
           entry.path().extension().unwrap() == "html" { 
            minify_html(&cfg,
                entry.path().to_str().unwrap(),
                out_dir.join(entry.file_name()).to_str().unwrap()
            );
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

    // Tailwind
    download_tailwind();
    execute_tailwind();

    minify_all_html();

    // Obtain instant.page

    download_file("https://instant.page/5.2.0".to_string(), "html_files/assets/instantpage.js");

    // Tell cargo to keep an eye on files
    println!("cargo:rerun-if-changed=tailwind.config.js");
	println!("cargo:rerun-if-changed=html_src");

}
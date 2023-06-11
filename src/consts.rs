pub mod defaults {
    pub const HTTP_PORT: u16 = 8080;
    pub const SERVE_ADRESS: [u8; 4] = [0, 0, 0, 0];
}
pub mod paths {
    const CURRENT_DIR: &str = "./";

    pub const fn get_conf_dir() -> &'static str {
        const CONTAINER_CONF: &str = "/config";

        if cfg!(feature = "container") {
            CONTAINER_CONF
        } else {
            CURRENT_DIR
        }
    }

    pub const fn get_html_files_dir() -> &'static str {
        const HTML_FILES: &str = "/html_files";

        if cfg!(feature = "container") {
            HTML_FILES
        } else {
            CURRENT_DIR
        }
    }
}

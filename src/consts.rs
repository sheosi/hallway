pub mod defaults {
    pub const HTTP_PORT: u16 = 8080;
    pub const SERVE_ADRESS: [u8; 4] = [0, 0, 0, 0];
    pub const CLEAN_TIME: u64 = 5 * 60 * 60; // 5 hours to check for old caches
    pub const MAX_TIME: u64 = 2 * 24 * 60 * 60; // 2 days max for cache
    pub const BACKGROUND: &str = "background.avif";
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

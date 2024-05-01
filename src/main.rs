use std::{convert::Infallible, net::Ipv4Addr, path::Path, sync::Arc};

use handlebars::Handlebars;
use tracing::trace;
use warp::{hyper::Uri, Filter, Rejection};

mod consts;
mod jwt;
mod pomerium;
mod rendering;

#[cfg(test)]
mod tests;

mod common {
    pub struct CurrentUserData {
        pub email: String,
        pub name: String,
    }
}

mod config {
    use serde::{Deserialize, Serialize};
    use std::path::Path;

    mod defaults {
        pub fn button_color() -> String {
            "#FEFFE8".to_string()
        }
    }

    #[derive(Debug, Deserialize, Clone, Serialize)]
    pub struct Route {
        pub icon: String,
        pub path: String,
        pub label: String,

        #[serde(default)]
        pub accessible_routes: Vec<Route>,

        #[serde(default = "defaults::button_color")]
        pub button_color: String,

        // Internal data
        #[serde(skip_deserializing)]
        pub escaped_label: String,

        #[serde(skip_deserializing)]
        pub is_group: bool,
    }

    #[derive(Debug, Deserialize)]
    pub struct Domain {
        pub name: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct Config {
        pub domain: Domain,
        pub routes: Vec<Route>,
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Config {
        fn fill_in_internals(routes: &mut [Route]) {
            routes.iter_mut().for_each(|route|{
                route.escaped_label = route.label.replace(' ', "_").replace('.', "_");
                route.is_group = !route.accessible_routes.is_empty();
                fill_in_internals(&mut route.accessible_routes);
            })
        }

        let mut conf: Config = toml::from_str(&std::fs::read_to_string(path.as_ref()).expect("Config can't be read")).expect("Config can't be parsed");
        // Fill escaped names
        fill_in_internals(&mut conf.routes);
        conf
    }
}

mod filters {
    use std::sync::Arc;

    #[cfg(feature = "container")]
    use aliri::Jwt;
    #[cfg(feature = "container")]
    use tracing::trace;
    use warp::{http::HeaderValue, hyper::header, Filter, Rejection, reject};

    use crate::consts;

    #[cfg(feature = "container")]
    use crate::jwt::JwtDecoder;

    #[derive(Debug)]
    struct MalformedJwt;

    impl reject::Reject for  MalformedJwt{}

    pub fn disable_cache() -> warp::reply::with::WithHeaders {
        let mut no_cache = warp::http::HeaderMap::new();
        no_cache.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache, no-store, must-revalidate"));
        no_cache.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
        no_cache.insert(header::EXPIRES, HeaderValue::from_static("0"));
        warp::reply::with::headers(no_cache)
    }

    pub fn cache_for<const T: u64, F, R>(
        filter: F,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = Rejection> + Clone + Send + Sync + 'static
    where
        F: Filter<Extract = (R,), Error = Rejection> + Clone + Send + Sync + 'static,
        R: warp::Reply,
    {
        filter.map(|reply| {
            warp::reply::with_header(
                reply,
                header::CACHE_CONTROL.as_str(),
                format!("max-age={}", T),
            )
        })
    }

    // Unfortunately, we can't just use debug here for testing since it is somehow dropping
    // the context in my build

    /// Extracts a JWT token from the header provided by pomerium
    #[cfg(feature = "container")]
    pub fn jwt(
        jwt_decoder: Arc<crate::jwt::JwtDecoder>,
    ) -> impl Filter<Extract = (crate::common::CurrentUserData,), Error = Rejection> + Clone
    {
        warp::header::header("X-Pomerium-Jwt-Assertion").map(move|s|(s, jwt_decoder.clone())).and_then(  move |(s, jwt_decoder):(String, Arc<JwtDecoder>)| async move {
            trace!(jwt = s);
            jwt_decoder.decode(Jwt::from(s)).ok_or(reject::custom(MalformedJwt))
        })
    }

    #[cfg(not(feature = "container"))]
    pub fn jwt(
        _: Arc<crate::jwt::JwtDecoder>,
    ) -> impl Filter<Extract = (crate::common::CurrentUserData,), Error = std::convert::Infallible> + Clone
    {

        // Unfortunately, we can't just use debug here for testing since it is somehow dropping
        // the context in my build
        warp::any().map(||{
            crate::common::CurrentUserData { 
                email: consts::defaults::debug::EMAIL.to_string(), 
                name: consts::defaults::debug::NAME.to_string()
            }
        })
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let log_level = std::env::var("LOG_LEVEL").unwrap_or("Info".to_string());
    let log_level = match log_level.to_ascii_lowercase().as_str() {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "info" => tracing::Level::INFO,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        _ => panic!("Unexpected log_level env info"),
    };

    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(log_level)
            .finish(),
    )
    .expect("initializing log failed");

    let html_files = Path::new(consts::paths::get_html_files_dir());
    let static_file =
        |path: &'static str| warp::path(path).and(warp::fs::file(html_files.join(path)));

    let (renderer, jwt_decoder) = {
        let conf_dir = Path::new(consts::paths::get_conf_dir());
        let config = config::load(conf_dir.join("config.toml"));
        let pomerium_conf = pomerium::load_conf(conf_dir.join("pomerium.yaml"));

        let jwt_decoder = Arc::new(jwt::JwtDecoder::new(&config.domain.name));
        let renderer = rendering::Renderer::from(
            config.routes,
            pomerium_conf.routes,
            &html_files.join("index.html"),
        );

        (renderer, jwt_decoder)
    };

    let renderer_clone = renderer.clone();

    let index = warp::path::end()
        .and_then(|| async move {
            trace!("Someone accessing index");
            Ok::<_, Rejection>(())
        })
        .untuple_one()
        .and(warp::get())
        .and(filters::jwt(jwt_decoder))
        .map(move |user_data: common::CurrentUserData| {
            trace!("Jwt received!");
            let html= renderer_clone.clone().render(user_data);
            trace!("Done rendering");
            warp::reply::html(html)
        })
        .with(filters::disable_cache());

    const TWO_WEEKS: u64 = consts::time::weeks(2);
    let assets = warp::path("assets")
        .and(warp::fs::dir(html_files.join("assets")))
        .or(static_file("apple-touch-icon.png"))
        .or(static_file("favicon-16x16.png"))
        .or(static_file("favicon-32x32.png"))
        .or(static_file("favicon.ico"))
        .or(static_file("site.webmanifest"))
        .with(warp::wrap_fn(filters::cache_for::<TWO_WEEKS, _, _>));

    let redirect_index = warp::path!("index.html").map(|| warp::redirect(Uri::from_static("/")));

    let app = index
        .or(assets)
        .or(redirect_index)
        .recover(|err| async move {
            let hb = Arc::new(Handlebars::new());
            let (html, status_code) = rendering::render_error(err, &hb);
            Ok::<_, Infallible>(warp::reply::with_status(
                warp::reply::html(html),
                status_code,
            ))
        })
        .with(warp::filters::compression::brotli());

    let http_port = std::env::var("HTTP_PORT")
        .ok()
        .map(|s: String| s.parse::<u16>().expect("Not a valid port"))
        .unwrap_or(consts::defaults::HTTP_PORT);

    let serve_address = std::env::var("HTTP_ADDRESS")
        .ok()
        .map(|s: String| s.parse::<Ipv4Addr>().expect("Not a valid IPv4 address").octets())
        .unwrap_or(consts::defaults::SERVE_ADRESS);

    // spawn proxy server
    warp::serve(app).run((serve_address, http_port)).await
}

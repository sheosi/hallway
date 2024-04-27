use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, RwLock},
    time::{Duration, SystemTime},
};

use crate::consts;
use crate::pomerium;

use handlebars::Handlebars;
use serde::Serialize;
use tokio::{task, time};
use tracing::{error, trace, warn};
use warp::{http::StatusCode, Rejection};

#[derive(Clone)]
struct RenderCacheItem {
    render: String,
    time: SystemTime,
}

#[derive(Clone)]
struct RenderCache {
    dict: Arc<RwLock<HashMap<String, RenderCacheItem>>>,
}

impl RenderCache {
    fn new() -> Self {
        let res = Self {
            dict: Arc::new(RwLock::new(HashMap::new())),
        };
        res.clone().start_maintenance();
        res
    }

    fn get_or_render(
        &mut self,
        data: &UserDataRender,
        handlebars: &Arc<Handlebars>,
    ) -> String {
        let cached = self
            .dict
            .read()
            .unwrap()
            .get(&data.email)
            .map(|i| i.render.clone());

        cached.unwrap_or_else(|| {
            trace!("Start rendering");
            let email = data.email.clone();

            let render = handlebars.render("index.html", data).expect("Failed to render index file");

            let item = RenderCacheItem {
                render: render.clone(),
                time: SystemTime::now()
            };

            self.dict.write().unwrap().insert(email, item);
            render
        })
    }

    fn clean_old(dict: &Arc<RwLock<HashMap<String, RenderCacheItem>>>) {
        dict.write()
            .unwrap()
            .retain(|_, v|
                SystemTime::now()
                .duration_since(v.time)
                .map(|d|d.as_secs())
                .unwrap_or_else(|e|{warn!("Somehow got a cache entry in the future, did the clock change? {}", e);consts::defaults::MAX_TIME + 1}) 
            < consts::defaults::MAX_TIME)
    }

    fn start_maintenance(self) {
        task::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(consts::defaults::CLEAN_TIME));

            loop {
                interval.tick().await;
                Self::clean_old(&self.dict);
            }
        });
    }
}

#[derive(Clone)]
pub struct Renderer<'a> {
    handlebars: Arc<Handlebars<'a>>,
    render_cache: RenderCache,
    user_data_holder: collections::UserDataHolder,
}

impl<'a> Renderer<'a> {
    pub fn from(
        conf_routes: Vec<crate::config::Route>,
        pomerium_data: Vec<pomerium::Route>,
        index_path: &Path,
    ) -> Self {
        let mut handlebars = Handlebars::new();
        handlebars
            .register_template_string("index.html", std::fs::read_to_string(index_path).expect("Couldn't read index.html: {}"))
            .expect("Malformed template");

        let emails = Self::extract_emails(&pomerium_data);
        let routes = collections::RouteHolder::from(conf_routes);
        let policies = collections::PolicyHolder::from(pomerium_data);

        let user_data_holder = collections::UserDataHolder::from(emails, &routes, policies);

        Self {
            handlebars: Arc::new(handlebars),
            render_cache: RenderCache::new(),
            user_data_holder,
        }
    }

    pub fn render(&mut self, user_data: crate::common::CurrentUserData) -> String {
        let user_data = self.user_data_holder.get_render(&user_data).unwrap_or_else(|| {
            warn!("Unregistered user '{}' '{}' has accesed the hallway", &user_data.name, &user_data.email);
            UserDataRender{name: user_data.name, email: user_data.email, background: consts::defaults::BACKGROUND.to_string(), accessible_routes: Vec::new()}
        });
        trace!("Got user data");
        self.render_cache
            .get_or_render(&user_data, &self.handlebars)
    }

    pub fn extract_emails(pomerium_data: &[pomerium::Route]) -> HashSet<String> {
        pomerium_data
            .iter()
            .fold(HashSet::new(), |mut h, r: &pomerium::Route| {
                r.policy.extract_emails(&mut h);
                h
            })
    }
}

pub fn render_error(err: Rejection, handlebars: &Arc<Handlebars<'_>>) -> (String, StatusCode) {
    fn load_html_and_render<P: AsRef<Path>>(
        path: P,
        handlebars: &Arc<Handlebars>,
        data: &HashMap<String, String>,
    ) -> String {
        let html = std::fs::read_to_string(path.as_ref()).unwrap_or_else(|e| panic!("Couldn't read '{}': {}", path.as_ref().display(), e));
        handlebars.render_template(&html, &data).unwrap_or_else(|e|{error!("Can't render page: {}", e); "Sorry we had an error!".to_string()})
    }

    if err.is_not_found() {
        (
            load_html_and_render(
                Path::new(consts::paths::get_html_files_dir()).join("404.html"),
                handlebars,
                &HashMap::new(),
            ),
            StatusCode::NOT_FOUND,
        )
    } else {
        (
            load_html_and_render(
                Path::new(consts::paths::get_html_files_dir()).join("50x.html"),
                handlebars,
                &HashMap::new(),
            ),
            StatusCode::INTERNAL_SERVER_ERROR,
        )
    }
}

#[derive(Clone, Serialize)]
pub struct UserDataRender {
    name: String,
    email: String,
    background: String,
    accessible_routes: Vec<crate::config::Route>,
}

mod collections {
    use crate::{config, consts, pomerium};
    use std::{
        collections::{HashMap, HashSet},
        sync::Arc,
    };

    use tracing::{trace, warn};

    pub struct RouteHolder {
        routes: Arc<Vec<Arc<config::Route>>>,
    }

    impl RouteHolder {
        pub fn from(hallway_data: Vec<config::Route>) -> Self {
            let routes = hallway_data.into_iter().map(Arc::new).collect::<Vec<_>>();

            Self {
                routes: Arc::new(routes),
            }
        }

        pub fn can_be_accessed_by(
            &self,
            email: &str,
            policy_holder: &PolicyHolder,
        ) -> Vec<Arc<config::Route>> {
            self.routes
                .iter()
                .filter(|r| {
                    let a = if let Some(policy) = policy_holder.get(&r.path){
                        policy.check_authorized(email)
                    }
                    else {
                        warn!("Path {} is invalid", &r.path);
                        false
                    };
                    trace!(route = r.path, email = email, authed = a);
                    a
                })
                .cloned()
                .collect()
        }
    }

    #[derive(Debug)]
    pub struct PolicyHolder {
        dict: Arc<HashMap<String, pomerium::Policy>>,
    }

    impl PolicyHolder {
        pub fn from(pomerium_data: Vec<pomerium::Route>) -> Self {
            let dict = pomerium_data
                .into_iter()
                .map(|r| (r.from, r.policy))
                .collect();
            Self {
                dict: Arc::new(dict),
            }
        }

        pub fn get(&self, from: &str) -> Option<&pomerium::Policy> {
            self.dict.get(from)
        }
    }

    pub struct UserData {
        accessible_routes: Vec<Arc<config::Route>>,
    }

    #[derive(Clone)]
    pub struct UserDataHolder {
        dict: Arc<HashMap<String, UserData>>,
    }

    impl UserDataHolder {
        pub fn from(emails: HashSet<String>, routes: &RouteHolder, policies: PolicyHolder) -> Self {
            let dict = emails
                .into_iter()
                .map(|e| {
                    let e_data = UserData {
                        accessible_routes: routes.can_be_accessed_by(&e, &policies),
                    };
                    trace!(email = e, "routes={:?}", e_data.accessible_routes);
                    (e, e_data)
                })
                .collect::<HashMap<String, UserData>>();

            Self {
                dict: Arc::new(dict),
            }
        }

        /// Will get none if the user is not registered
        pub fn get_render(
            &self,
            user: &crate::common::CurrentUserData,
        ) -> Option<super::UserDataRender> {
            self.dict.get(&user.email).map(|u| super::UserDataRender {
                name: user.name.clone(),
                email: user.email.clone(),
                background: consts::defaults::BACKGROUND.to_string(),
                accessible_routes: u
                    .accessible_routes
                    .iter()
                    .map(|r| (**r).clone())
                    .collect::<Vec<_>>(),
            })
        }
    }
}

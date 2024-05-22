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
        user_data: &UserDataRender,
        global_data: &GlobalData,
        handlebars: &Arc<Handlebars>,
    ) -> String {
        #[derive(Clone, Serialize)]
        struct RenderData<'a> {
            user: &'a UserDataRender,
            global: &'a GlobalData
        }

        let cached = self
            .dict
            .read()
            .unwrap()
            .get(&user_data.email)
            .map(|i| i.render.clone());

        cached.unwrap_or_else(|| {
            trace!("Start rendering");
            let email = user_data.email.clone();

            let data =  RenderData{user: user_data, global: global_data};
            let render = handlebars.render("index.html", &data).expect("Failed to render index file");

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

#[derive(Clone, Serialize)]
pub struct GlobalData {
    pub sign_out_url: String
}

#[derive(Clone)]
pub struct Renderer<'a> {
    handlebars: Arc<Handlebars<'a>>,
    render_cache: RenderCache,
    user_data_holder: collections::UserDataHolder,
    global_data: Arc<GlobalData>
}

impl<'a> Renderer<'a> {
    pub fn from(
        conf_routes: Vec<crate::config::Route>,
        pomerium_data: Vec<pomerium::Route>,
        index_path: &Path,
        global_data: Arc<GlobalData>
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
            global_data
        }
    }

    pub fn render(&mut self, user_data: crate::common::CurrentUserData) -> String {
        let user_data = self.user_data_holder.get_render(&user_data);
        trace!("Got user data");
        self.render_cache
            .get_or_render(&user_data, &self.global_data, &self.handlebars)
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

pub fn render_error(err: Rejection, handlebars: &Arc<Handlebars<'_>>, global_data: &GlobalData) -> (String, StatusCode) {
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
    use crate::{config::{self, RouteData}, consts, pomerium};
    use std::{
        collections::{HashMap, HashSet},
        sync::Arc,
    };

    use tracing::{info, trace, warn};

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
            fn check_route(email: &str, policy_holder: &PolicyHolder, r_data: &RouteData) -> bool {
                match &r_data {
                    RouteData::Path(path) => {
                        let res = if let Some(policy) = policy_holder.get(path){
                            policy.check_authorized(email)
                        }
                        else {
                            warn!("Path {} is invalid", &path);
                            false
                        };
                        trace!(route = path, email = email, authed = res);
                        res
                    }
                    RouteData::Group(group) => {
                        group.iter().any(|r|check_route(email, policy_holder, &r.data))
                    }
                }
            }
            self.routes
                .iter()
                .filter(|r| {
                    check_route(email, policy_holder, &r.data)
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
            fn make_url(mut from: String, path: &String, prefix: &String) -> String {
                from.push_str(&path);
                from.push_str(&prefix);
                from
            }

            let dict = pomerium_data
                .into_iter()
                .map(|r| { 
                    let policy = if r.allow_public_unauthenticated_access {
                        crate::pomerium::Policy::allow_all()
                    }
                    else {
                        r.policy
                    };
                    (make_url(r.from, &r.path, &r.prefix), policy)
                })
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
        public_urls: Vec<Arc<config::Route>>
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

            let public_urls = routes.can_be_accessed_by("", &policies);
            Self {
                dict: Arc::new(dict),
                public_urls
            }
        }

        /// Will get none if the user is not registered
        pub fn get_render(
            &self,
            user: &crate::common::CurrentUserData,
        ) -> super::UserDataRender {
            self.dict.get(&user.email).map(|u| super::UserDataRender {
                name: user.name.clone(),
                email: user.email.clone(),
                background: consts::defaults::BACKGROUND.to_string(),
                accessible_routes: u
                    .accessible_routes
                    .iter()
                    .map(|r| (**r).clone())
                    .collect::<Vec<_>>(),
            }).unwrap_or_else(|| {
                info!("Unregistered user '{}' '{}' has accesed the hallway", &user.name, &user.email);

                super::UserDataRender {
                    name: user.name.clone(),
                    email: user.email.clone(),
                    background: consts::defaults::BACKGROUND.to_string(),
                    accessible_routes: self
                        .public_urls
                        .iter()
                        .map(|r|(**r).clone())
                        .collect::<Vec<_>>()
            }})
        }
    }
}

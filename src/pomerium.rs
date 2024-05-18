use policy::PolicyChecker;
use serde::Deserialize;
use std::{collections::HashSet, path::Path};

use self::policy::{AndPolicy, NorPolicy, NotPolicy, OrPolicy};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub routes: Vec<Route>,
}

#[derive(Debug, Deserialize)]
pub struct Route {
    pub from: String,

    #[serde(default)]
    pub prefix: String,

    #[serde(default)]
    pub path: String,
    
    #[serde(default)]
    pub allow_public_unauthenticated_access: bool,

    #[serde(default)]
    pub policy: Policy,
}

#[derive(Debug, Default, Deserialize)]
pub struct Policy(Vec<PolicyAction>);

impl Policy {
    pub fn allow_all() -> Policy {
        Policy(vec![PolicyAction{allow: ActionOperator::any(), deny: ActionOperator::empty()}])
    }
    pub fn extract_emails(&self, hash_set: &mut HashSet<String>) {
        self.0.iter().for_each(|a| a.extract_emails(hash_set));
    }

    pub fn check_authorized(&self, email: &str) -> bool {
        if self.0.is_empty() {
            false
        }
        else {
            self.0
                .iter()
                .any(|p| p.check_authorized(email).try_into().unwrap_or(true))
        }
        
    }
}

#[derive(Debug, Deserialize)]
struct PolicyAction {
    #[serde(default)]
    allow: ActionOperator,

    #[serde(default)]
    deny: ActionOperator,
}

impl PolicyAction {
    pub fn extract_emails(&self, hash_set: &mut HashSet<String>) {
        self.allow.extract_emails(hash_set);
        self.deny.extract_emails(hash_set);
    }
}

#[derive(Debug, Default, Deserialize)]
struct ActionOperator {
    #[serde(default)]
    or: policy::OrPolicy,

    #[serde(default)]
    and: policy::AndPolicy,

    #[serde(default)]
    not: policy::NotPolicy,

    #[serde(default)]
    nor: policy::NorPolicy,
}

impl ActionOperator {
    fn any() -> ActionOperator {
        ActionOperator {
            or: OrPolicy(vec![ActionCriteria::Accept{accept: matchers::Empty(serde_yaml::Value::Null)}]),
            and: AndPolicy(vec![]),
            not: NotPolicy(vec![]),
            nor: NorPolicy(vec![])
        }
    }

    fn empty() -> ActionOperator {
        ActionOperator {
            or: OrPolicy(vec![]),
            and: AndPolicy(vec![]),
            not: NotPolicy(vec![]),
            nor: NorPolicy(vec![])
        }
    }

    fn extract_emails(&self, hash_set: &mut HashSet<String>) {
        self.or.extract_emails(hash_set);
        self.and.extract_emails(hash_set);
        self.not.extract_emails(hash_set);
        self.nor.extract_emails(hash_set);
    }
}

#[allow(dead_code)] // We need the "dead code" since it is used as a marker
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ActionCriteria {
    User { user: matchers::String },
    Email { email: matchers::String },
    Accept { accept: matchers::Empty },
}

impl ActionCriteria {
    fn extract_emails(&self, hash_set: &mut HashSet<String>) {
        match self {
            ActionCriteria::User { user } => user.extract_emails(hash_set),
            ActionCriteria::Email { email } => email.extract_emails(hash_set),
            ActionCriteria::Accept { accept: _ } => {}
        }
    }
}
mod matchers {
    use serde::Deserialize;
    use std::collections::HashSet;

    use super::policy::PolicyCheckerResult;

    #[derive(Debug, Deserialize)]
    pub struct String {
        #[serde(default)]
        is: Option<std::string::String>,

        #[serde(default)]
        starts_with: Option<std::string::String>,

        #[serde(default)]
        ends_with: Option<std::string::String>,

        #[serde(default)]
        contains: Option<std::string::String>,
    }

    impl String {
        pub fn extract_emails(&self, hash_set: &mut HashSet<std::string::String>) {
            self.is.as_ref().map(|s| hash_set.insert(s.clone()));
            self.starts_with
                .as_ref()
                .map(|s| hash_set.insert(s.clone()));
            self.ends_with.as_ref().map(|s| hash_set.insert(s.clone()));
            self.contains.as_ref().map(|s| hash_set.insert(s.clone()));
        }
    }

    // This is to accept accept inputed as Yaml requires something
    #[derive(Debug, Deserialize)]
    pub struct Empty(pub(super) serde_yaml::Value);

    impl super::policy::PolicyChecker for String {
        fn check_authorized(&self, email: &str) -> PolicyCheckerResult {
            let is = self.is.as_ref().map(|s| s == email).unwrap_or(true);
            let starts_with = self
                .starts_with
                .as_ref()
                .map(|s| email.starts_with(s))
                .unwrap_or(true);
            let ends_with = self
                .ends_with
                .as_ref()
                .map(|s| email.ends_with(s))
                .unwrap_or(true);
            let contains = self
                .contains
                .as_ref()
                .map(|s| email.contains(s))
                .unwrap_or(true);
            (is && starts_with && ends_with && contains).into()
        }
    }
}

fn apply_modifications(conf: &mut Config) {
    // Some config entries overwrite the policy
    conf.routes.iter_mut().for_each(|r|{
        if r.allow_public_unauthenticated_access {
            r.policy = crate::pomerium::Policy::allow_all()
        }
    });
}

pub fn load_conf<P: AsRef<Path>>(path: P) -> Config {
    let file = std::fs::File::open(path.as_ref()).expect("Couldn't open pomerium config file");
    let mut conf: Config = serde_yaml::from_reader(file).expect("Pomerium config was malformed:");

    apply_modifications(&mut conf);
    conf
}

#[cfg(test)]
pub fn load_from_str(conf: &str) -> Config {
    let mut conf = serde_yaml::from_str(conf).expect("Malformed yaml file");
    apply_modifications(&mut conf);
    conf
}

pub mod policy {
    use std::collections::HashSet;

    use super::ActionCriteria;
    use serde::Deserialize;
    use tracing::trace;

    #[derive(Debug, PartialEq)]
    pub enum PolicyCheckerResult {
        Passed,
        NotPassed,
        Empty,
    }

    impl std::ops::Add<PolicyCheckerResult> for PolicyCheckerResult {
        type Output = Self;

        fn add(self, rhs: PolicyCheckerResult) -> Self::Output {
            match (self, rhs) {
                (PolicyCheckerResult::Passed, _) => PolicyCheckerResult::Passed,
                (_, PolicyCheckerResult::Passed) => PolicyCheckerResult::Passed,
                (PolicyCheckerResult::NotPassed, _) => PolicyCheckerResult::NotPassed,
                (_, PolicyCheckerResult::NotPassed) => PolicyCheckerResult::NotPassed,
                (PolicyCheckerResult::Empty, PolicyCheckerResult::Empty) => {
                    PolicyCheckerResult::Empty
                }
            }
        }
    }

    impl std::ops::Not for PolicyCheckerResult {
        type Output = Self;

        fn not(self) -> Self::Output {
            match self {
                PolicyCheckerResult::Passed => PolicyCheckerResult::NotPassed,
                PolicyCheckerResult::NotPassed => PolicyCheckerResult::Passed,
                PolicyCheckerResult::Empty => PolicyCheckerResult::Empty,
            }
        }
    }

    impl From<bool> for PolicyCheckerResult {
        fn from(value: bool) -> Self {
            if value {
                PolicyCheckerResult::Passed
            } else {
                PolicyCheckerResult::NotPassed
            }
        }
    }

    impl TryInto<bool> for PolicyCheckerResult {
        type Error = ();

        fn try_into(self) -> Result<bool, Self::Error> {
            match self {
                PolicyCheckerResult::Passed => Ok(true),
                PolicyCheckerResult::NotPassed => Ok(false),
                PolicyCheckerResult::Empty => Err(()),
            }
        }
    }

    impl ToString for PolicyCheckerResult {
        fn to_string(&self) -> String {
            match self {
                PolicyCheckerResult::Passed => "passed".into(),
                PolicyCheckerResult::NotPassed => "not passed".into(),
                PolicyCheckerResult::Empty => "empty".into(),
            }
        }
    }

    pub trait PolicyChecker {
        fn check_authorized(&self, email: &str) -> PolicyCheckerResult;
    }

    impl PolicyChecker for super::PolicyAction {
        fn check_authorized(&self, email: &str) -> PolicyCheckerResult {
            let allowed = self.allow.check_authorized(email);
            let denied = self.deny.check_authorized(email);

            trace!("allowed={:?} denied={:?}", allowed, denied);
            allowed + !denied
        }
    }

    impl PolicyChecker for super::ActionOperator {
        fn check_authorized(&self, email: &str) -> PolicyCheckerResult {
            let or_pol = self.or.check_authorized(email);
            let and_pol = self.and.check_authorized(email);
            let not_pol = self.not.check_authorized(email);
            let nor_pol = self.nor.check_authorized(email);

            trace!(
                "or={:?}, and={:?}, not={:?}, nor={:?}",
                or_pol,
                and_pol,
                not_pol,
                nor_pol
            );

            or_pol + and_pol + not_pol + nor_pol
        }
    }

    impl PolicyChecker for super::ActionCriteria {
        fn check_authorized(&self, email_in: &str) -> PolicyCheckerResult {
            match self {
                ActionCriteria::User { user } => user.check_authorized(email_in),
                ActionCriteria::Email { email } => email.check_authorized(email_in),
                ActionCriteria::Accept { accept: _ } => PolicyCheckerResult::Passed,
            }
        }
    }

    fn wrap_iter<F: FnOnce(std::slice::Iter<'_, ActionCriteria>) -> PolicyCheckerResult>(
        crit_vec: &[super::ActionCriteria],
        check_pol: F,
    ) -> PolicyCheckerResult {
        if crit_vec.is_empty() {
            PolicyCheckerResult::Empty
        } else {
            check_pol(crit_vec.iter())
        }
    }

    // This seems counterintuitive but this is to be used with XOR, when the
    // second argument of an xor is true inverts the result of the first one
    const PASSES: bool = false;
    const NOT_PASSES: bool = true;

    fn check_into_bool(c: &ActionCriteria, email: &str, inverted: bool) -> bool {
        TryInto::<bool>::try_into(c.check_authorized(email))
            .expect("At this point there should be no empty")
            ^ inverted
    }

    fn any_passes(
        crit_vec: &[super::ActionCriteria],
        email: &str,
        inverted: bool,
    ) -> PolicyCheckerResult {
        wrap_iter(crit_vec, |mut iter| {
            iter.any(|c| check_into_bool(c, email, inverted)).into()
        })
    }

    fn all_pass(
        crit_vec: &[super::ActionCriteria],
        email: &str,
        inverted: bool,
    ) -> PolicyCheckerResult {
        wrap_iter(crit_vec, |mut iter| {
            iter.all(|c| check_into_bool(c, email, inverted)).into()
        })
    }

    #[derive(Debug, Default, Deserialize)]
    #[serde(transparent)]
    pub(super) struct OrPolicy(pub(super) Vec<super::ActionCriteria>);

    impl OrPolicy {
        pub fn extract_emails(&self, hash_set: &mut HashSet<String>) {
            self.0.iter().for_each(|p| p.extract_emails(hash_set))
        }
    }

    impl PolicyChecker for OrPolicy {
        fn check_authorized(&self, email: &str) -> PolicyCheckerResult {
            any_passes(&self.0, email, PASSES)
        }
    }

    #[derive(Debug, Default, Deserialize)]
    pub(super) struct NorPolicy(pub(super) Vec<super::ActionCriteria>);

    impl NorPolicy {
        pub fn extract_emails(&self, hash_set: &mut HashSet<String>) {
            self.0.iter().for_each(|p| p.extract_emails(hash_set))
        }
    }

    impl PolicyChecker for NorPolicy {
        fn check_authorized(&self, email: &str) -> PolicyCheckerResult {
            any_passes(&self.0, email, NOT_PASSES)
        }
    }

    #[derive(Debug, Default, Deserialize)]
    pub(super) struct AndPolicy(pub(super) Vec<super::ActionCriteria>);

    impl AndPolicy {
        pub fn extract_emails(&self, hash_set: &mut HashSet<String>) {
            self.0.iter().for_each(|p| p.extract_emails(hash_set))
        }
    }

    impl PolicyChecker for AndPolicy {
        fn check_authorized(&self, email: &str) -> PolicyCheckerResult {
            all_pass(&self.0, email, PASSES)
        }
    }

    #[derive(Debug, Default, Deserialize)]
    pub(super) struct NotPolicy(pub(super) Vec<super::ActionCriteria>);

    impl NotPolicy {
        pub fn extract_emails(&self, hash_set: &mut HashSet<String>) {
            self.0.iter().for_each(|p| p.extract_emails(hash_set))
        }
    }

    impl PolicyChecker for NotPolicy {
        fn check_authorized(&self, email: &str) -> PolicyCheckerResult {
            all_pass(&self.0, email, NOT_PASSES)
        }
    }
}

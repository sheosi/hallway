use std::{str::FromStr, time::Duration};

use aliri::{
    jwa,
    jwt::{self, CoreClaims, CoreHeaders, HasAlgorithm},
    Jwks, Jwt,
};
use aliri_clock::UnixTime;
use tracing::{instrument, trace};

#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[must_use]
pub struct Oauth2Claims {
    #[serde(default, skip_serializing_if = "jwt::Audiences::is_empty")]
    aud: jwt::Audiences,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    iss: Option<jwt::Issuer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sub: Option<jwt::Subject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    exp: Option<serde_json::Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    nbf: Option<serde_json::Number>,
    email: String,
    name: String,
}

impl CoreClaims for Oauth2Claims {
    fn nbf(&self) -> Option<UnixTime> {
        self.nbf
            .clone()
            .map(|n| UnixTime(n.as_u64().unwrap_or_else(|| n.as_f64().unwrap() as u64)))
    }

    fn aud(&self) -> &jwt::Audiences {
        &self.aud
    }

    fn exp(&self) -> Option<UnixTime> {
        self.exp
            .clone()
            .map(|n| UnixTime(n.as_u64().unwrap_or_else(|| n.as_f64().unwrap() as u64)))
    }

    fn iss(&self) -> Option<&jwt::IssuerRef> {
        self.iss.as_ref().map(|i| i.as_ref())
    }

    fn sub(&self) -> Option<&jwt::SubjectRef> {
        self.sub.as_ref().map(|s| s.as_ref())
    }
}

#[derive(Debug)]
pub struct JwtDecoder {
    validator: jwt::CoreValidator,
    keys: Jwks,
}


impl JwtDecoder {
    pub fn new(domain_name: &str) -> Self {
        let keys = Self::get_keys(domain_name);

        let validator = jwt::CoreValidator::default()
            .ignore_expiration()
            .add_approved_algorithm(jwa::Algorithm::ES256)
            .add_allowed_audience(jwt::Audience::from_str(domain_name).unwrap())
            .require_issuer(jwt::Issuer::from_str(domain_name).unwrap())
            .check_expiration()
            .with_leeway(Duration::from_secs(60));

        Self { validator, keys }
    }

    fn get_keys(domain_name: &str) -> aliri::Jwks {
        let resp: reqwest::blocking::Response = reqwest::blocking::get(format!(
            "https://{}/.well-known/pomerium/jwks.json",
            domain_name
        ))
        .unwrap();
        resp.json().unwrap()
    }

    #[instrument]
    pub fn decode(&self, jwt: Jwt) -> crate::common::CurrentUserData {
        trace!("Decomposing");
        let decomposed: jwt::Decomposed = jwt.decompose().unwrap();

        trace!("Getting key ref");
        let key_ref = self
            .keys
            .get_key_by_id(decomposed.kid().unwrap(), decomposed.alg())
            .unwrap();

        trace!("Verifying");
        let data: jwt::Validated<Oauth2Claims> = jwt
            .verify(key_ref, &self.validator)
            .expect("JWT was invalid");

        let claims: &Oauth2Claims = data.claims();

        trace!("Done!");

        crate::common::CurrentUserData {
            email: claims.email.clone(),
            name: claims.name.clone(),
        }
    }
}

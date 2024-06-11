use std::{str::FromStr, time::Duration};

#[cfg(feature = "container")]
use crate::utils;

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

fn extract_num(n: serde_json::Number) -> Option<u64> {
    if let Some(u) = n.as_u64() {Some(u)}
    else {n.as_f64().map(|f| f as u64)}
}

impl CoreClaims for Oauth2Claims {
    fn nbf(&self) -> Option<UnixTime> {
        self.nbf
            .clone()
            .and_then(|n| extract_num(n).map(UnixTime))
    }

    fn aud(&self) -> &jwt::Audiences {
        &self.aud
    }

    fn exp(&self) -> Option<UnixTime> {
        self.exp
            .clone()
            .and_then(|n| extract_num(n).map(UnixTime))
    }

    fn iss(&self) -> Option<&jwt::IssuerRef> {
        self.iss.as_ref().map(|i| i.as_ref())
    }

    fn sub(&self) -> Option<&jwt::SubjectRef> {
        self.sub.as_ref().map(|s| s.as_ref())
    }
}

#[derive(Debug)]
// On testing mode validator and keys are never used, that's alright
pub struct JwtDecoder {
    #[allow(dead_code)]
    validator: jwt::CoreValidator,

    #[allow(dead_code)]
    keys: Jwks,
}


impl JwtDecoder {
    pub fn new(domain_name: &str, jwks_route: &str) -> Self {
        let keys = Self::get_jwks(jwks_route);

        let validator = jwt::CoreValidator::default()
            .ignore_expiration()
            .add_approved_algorithm(jwa::Algorithm::ES256)
            .add_allowed_audience(jwt::Audience::from_str(domain_name).expect("Malformed domain name"))
            .require_issuer(jwt::Issuer::from_str(domain_name).expect("Malformed domain name"))
            .check_expiration()
            .with_leeway(Duration::from_secs(60));

     
     
        Self { validator, keys }
    }
    #[instrument]
    pub fn decode(&self, jwt: Jwt) -> Option<crate::common::CurrentUserData> {
        trace!("Decomposing");
        let decomposed: jwt::Decomposed = jwt.decompose().ok()?;

        trace!("Getting key ref");
        let key_ref = self
            .keys
            .get_key_by_id(decomposed.kid()?, decomposed.alg())?;

        trace!("Verifying");
        let data: jwt::Validated<Oauth2Claims> = jwt
            .verify(key_ref, &self.validator)
            .expect("JWT was invalid");

        let claims: &Oauth2Claims = data.claims();

        trace!("Done!");

        Some(crate::common::CurrentUserData {
            email: claims.email.clone(),
            name: claims.name.clone(),
            picture: None // Not yet supported
        })
    }

    #[cfg(feature = "container")]
    fn get_jwks(jwks_route: &str) -> Jwks {
        utils::get_json(jwks_route)
    }

    // Dummy version for testing
    #[cfg(not(feature = "container"))]
    fn get_jwks(_: &str) -> Jwks {
        Jwks::default()
    }
}

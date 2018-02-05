#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate hyper;
#[cfg(feature = "tls")]
extern crate hyper_tls;
#[macro_use]
extern crate log;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate serde_urlencoded;
extern crate tokio_core;

use std::collections::BTreeMap;

use futures::{Future as StdFuture, IntoFuture, Stream as StdStream};
use futures::future::Either;
#[cfg(feature = "tls")]
use hyper_tls::HttpsConnector;
use hyper::{Client as HyperClient, Method};
use hyper::client::{Connect, HttpConnector, Request};
use hyper::header::Authorization;
use serde::de::DeserializeOwned;
use tokio_core::reactor::Handle;

mod error {
    use std::io::Error as IoError;
    use hyper::Error as HttpError;
    use hyper::error::UriError;
    use serde_json::error::Error as SerdeError;
    error_chain! {
        foreign_links {
            Codec(SerdeError);
            Http(HttpError);
            IO(IoError);
            URI(UriError);
        }
    }
}

pub use error::{Error, ErrorKind, Result};

/// A type alias for a `Future` that may result in a `launchdarkly::Error`
pub type Future<T> = Box<StdFuture<Item = T, Error = Error>>;

const API_HOST: &str = "https://app.launchdarkly.com";

#[derive(Debug, Deserialize)]
pub struct Link {
    pub href: String,
    #[serde(rename = "type")]
    pub link_type: String,
}

#[derive(Debug, Deserialize)]
pub struct Links {
    #[serde(rename = "self")]
    pub _self: Link,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureFlag {
    pub key: String,
    pub name: String,
    pub kind: String,
    pub creation_date: usize,
    pub include_in_snippet: bool,
    pub temporary: bool,
    pub maintainer_id: Option<String>,
    pub tags: Vec<String>,
    pub variations: Vec<Variation>,
    #[serde(rename = "_links")]
    pub _links: Links,
    #[serde(rename = "_maintainer")]
    pub _maintainer: Option<Member>,
    pub environments: BTreeMap<String, FeatureFlagConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureFlagConfig {
    pub on: bool,
    pub archived: bool,
    pub salt: String,
    pub sel: String,
    pub last_modified: usize,
    pub version: usize,
    pub targets: Vec<Target>,
    pub rules: Vec<Rule>,
    pub fallthrough: Fallthrough,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fallthrough {
    pub variation: Option<usize>,
    pub rollout: Option<Rollout>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub values: Vec<String>,
    pub variation: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub variation: Option<usize>,
    pub rollout: Option<Rollout>,
    pub clause: Option<Vec<Clause>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clause {
    pub attribute: String,
    pub op: String,
    pub values: Vec<String>,
    pub negate: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rollout {
    pub variations: Vec<WeightedVariation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightedVariation {
    pub variation: usize,
    pub weight: usize,
}

#[derive(Debug, Deserialize)]
pub struct FeatureFlags {
    pub _links: Links,
    pub items: Vec<FeatureFlag>,
}

#[derive(Debug, Deserialize)]
pub struct Variation {
    pub name: Option<String>,
    pub description: Option<String>,
    pub value: ::serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    #[serde(rename = "_links")]
    pub _links: Links,
    #[serde(rename = "_id")]
    pub _id: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub role: String,
    pub email: String,
    #[serde(rename = "_pending_invite")]
    pub _pending_invite: Option<bool>,
    pub is_beta: Option<bool>,
    pub custom_roles: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct Projects {
    #[serde(rename = "_links")]
    pub _links: Links,
    pub items: Vec<Project>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    #[serde(rename = "_links")]
    pub _links: Links,
    pub key: String,
    pub name: String,
    pub include_in_snippet_by_default: bool,
    pub environments: Vec<Environment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pubnub {
    pub channel: String,
    pub cipher_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    #[serde(rename = "_id")]
    pub _id: String,
    #[serde(rename = "_pubnub")]
    pub _pubnub: Pubnub,
    pub key: String,
    pub name: String,
    pub api_key: String,
    pub mobile_key: String,
    pub color: String,
    pub default_ttl: usize,
    pub secure_mode: bool,
}

#[derive(Debug, Serialize, Default)]
pub struct FlagOptions {
    pub env: Option<String>,
    pub tag: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Client<C>
where
    C: Clone + Connect,
{
    http: HyperClient<C>,
    key: String,
}

#[cfg(feature = "tls")]
impl Client<HttpsConnector<HttpConnector>> {
    pub fn new<K>(key: K, handle: &Handle) -> Self
    where
        K: Into<String>,
    {
        let connector = HttpsConnector::new(4, handle).unwrap();
        let http = HyperClient::configure()
            .connector(connector)
            .keep_alive(true)
            .build(handle);
        Self::custom(key, http)
    }
}

impl<C> Client<C>
where
    C: Clone + Connect,
{
    pub fn custom<K>(credentials: K, http: HyperClient<C>) -> Self
    where
        K: Into<String>,
    {
        Self {
            http,
            key: credentials.into(),
        }
    }

    pub fn projects(&self) -> Future<Projects> {
        self.request(Method::Get, "/projects".into(), None)
    }

    pub fn flags<P>(&self, project: P, options: &FlagOptions) -> Future<FeatureFlags>
    where
        P: Into<String>,
    {
        self.request(
            Method::Get,
            format!(
                "/flags/{project}?{query}",
                project = project.into(),
                query = serde_urlencoded::to_string(options).unwrap()
            ),
            None,
        )
    }

    pub fn request<Out>(&self, method: Method, uri: String, body: Option<Vec<u8>>) -> Future<Out>
    where
        Out: DeserializeOwned + 'static,
    {
        let client = self.clone();
        let key = self.key.clone();
        let response = format!("{}/api/v2{}", API_HOST, uri)
            .parse()
            .into_future()
            .map_err(Error::from)
            .and_then(move |url| {
                let mut req = Request::new(method, url);
                req.headers_mut().set(Authorization(key));
                if let Some(body) = body {
                    req.set_body(body)
                }
                client.http.request(req).map_err(Error::from)
            });
        Box::new(response.and_then(move |response| {
            if response.status().is_success() {
                Either::A(response.body().concat2().map_err(Error::from).and_then(
                    move |response_body| {
                        debug!("{}", String::from_utf8_lossy(&response_body));
                        serde_json::from_slice::<Out>(&response_body)
                            .map_err(|error| ErrorKind::Codec(error).into())
                    },
                ))
            } else {
                Either::B(response.body().concat2().map_err(Error::from).and_then(
                    move |response_body| {
                        debug!("{}", String::from_utf8_lossy(&response_body));
                        Err(String::from("fail").into()).into_future()
                    },
                ))
            }
        }))
    }
}

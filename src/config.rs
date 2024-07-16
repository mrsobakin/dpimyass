use std::{
    net::SocketAddr,
    net::ToSocketAddrs,
    time::Duration
};

use serde::Deserialize;
use serde::de::Deserializer;
use serde_with::DurationSeconds;
use serde_with::serde_as;


fn resolve_address<'de, D>(de: D) -> Result<SocketAddr, D::Error>
where D: Deserializer<'de> {
    let addr = <String>::deserialize(de)?;

    addr.to_socket_addrs()
       .map_err(serde::de::Error::custom)?
       .next()
       .ok_or(serde::de::Error::custom("No address"))
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub servers: Vec<ServerConfig>
}

#[derive(Deserialize, Debug)]
pub struct ServerConfig {
    pub name: String,
    #[serde(flatten)]
    pub obfs: ObfsConfig,
    pub relay: EndpointConfig,
    pub upstream: EndpointConfig,
}

#[derive(Deserialize, Debug)]
pub struct ObfsConfig {
    pub key: Vec<u8>,
    pub first: Option<usize>,
}

#[serde_as]
#[derive(Deserialize, Debug)]
pub struct EndpointConfig {
    #[serde(deserialize_with = "resolve_address")]
    pub address: SocketAddr,
    pub buffer: usize,
    #[serde_as(as = "DurationSeconds<u64>")]
    pub timeout: Duration,
}

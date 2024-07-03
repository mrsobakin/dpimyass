#![feature(trait_alias)]

use std::{
    error::Error,
    net::SocketAddr,
    net::ToSocketAddrs,
    time::Duration
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    io::{AsyncRead, AsyncWrite},
    time::timeout,
};
use serde::Deserialize;
use serde::de::Deserializer;
use serde_with::DurationSeconds;
use serde_with::serde_as;
use udp_stream::{UdpListener, UdpStream};


fn resolve_address<'de, D>(de: D) -> Result<SocketAddr, D::Error>
where D: Deserializer<'de> {
    let addr = <String>::deserialize(de)?;

    addr.to_socket_addrs()
       .map_err(serde::de::Error::custom)?
       .next()
       .ok_or(serde::de::Error::custom("No address"))
}

#[derive(Deserialize, Debug)]
struct Config {
    servers: Vec<ServerConfig>
}

#[derive(Deserialize, Debug)]
struct ServerConfig {
    name: String,
    #[serde(flatten)]
    obfs: ObfsConfig,
    proxy: EndpointConfig,
    downstream: EndpointConfig,
}

#[derive(Deserialize, Debug)]
struct ObfsConfig {
    key: Vec<u8>,
}

#[serde_as]
#[derive(Deserialize, Debug)]
struct EndpointConfig {
    #[serde(deserialize_with = "resolve_address")]
    address: SocketAddr,
    buffer: usize,
    #[serde_as(as = "DurationSeconds<u64>")]
    timeout: Duration,
}


fn xor_obfuscate(data: &mut [u8], cfg: &ObfsConfig) {
    for (di, ki) in data.iter_mut().zip(cfg.key.iter().cycle()) {
        *di ^= ki
    }
}


trait Stream = AsyncWrite + AsyncRead + Unpin;

struct StreamAndInfo<S: Stream> {
    stream: S,
    buffer: usize,
    timeout: Duration
}

impl<S: Stream> StreamAndInfo<S> {
    fn new(stream: S, config: &EndpointConfig) -> Self {
        StreamAndInfo {
            stream,
            buffer: config.buffer,
            timeout: config.timeout,
        }
    }
}


async fn forward_loop(mut upstream: StreamAndInfo<impl Stream>, mut downstream: StreamAndInfo<impl Stream>, cfg: &ObfsConfig) -> Result<(), std::io::Error> {
    let mut upbuf = vec![0u8; upstream.buffer];
    let mut downbuf = vec![0u8; downstream.buffer];

    loop {
        tokio::select! {
            n = timeout(upstream.timeout, upstream.stream.read(&mut upbuf)) => {
                let n = n??;
                xor_obfuscate(&mut upbuf[0..n], cfg);
                downstream.stream.write_all(&upbuf[0..n]).await?;
            },
            n = timeout(downstream.timeout, downstream.stream.read(&mut downbuf)) => {
                let n = n??;
                xor_obfuscate(&mut downbuf[0..n], cfg);
                upstream.stream.write_all(&downbuf[0..n]).await?;
            }
        };
    }

    #[allow(unreachable_code)]
    Ok::<(), std::io::Error>(())
}

async fn server_loop(config: &'static ServerConfig) -> Result<(), Box<dyn Error>> {
    let listener = UdpListener::bind(config.proxy.address).await?;

    println!("[{}] Listening on {:?}, downstream {:?}", config.name, config.proxy.address, config.downstream.address);

    loop {
        let (upstream, addr) = listener.accept().await?;
        let upstream = StreamAndInfo::new(upstream, &config.proxy);

        println!("[{}] New incoming connection from {addr:?}", config.name);

        tokio::spawn(async move {
            let downstream = StreamAndInfo::new(UdpStream::connect(config.downstream.address).await?, &config.downstream);

            if let Err(e) = forward_loop(upstream, downstream, &config.obfs).await {
                println!("[{}] Error: {e:?} ({addr:?})", config.name);
            }

            Ok::<(), std::io::Error>(())
        });
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config: &Config = Box::leak({
        let config_file = std::env::args().nth(1).unwrap_or_else(|| "config.toml".to_owned());

        let s = std::fs::read_to_string(config_file)?;
        toml::from_str(&s)?
    });

    let mut handles = vec![];

    for server_config in config.servers.iter() {
        handles.push(tokio::spawn(async {
            loop {
                match server_loop(server_config).await {
                    Err(err) => {
                        println!("[{}] encountered a loop-wise error: {err}", server_config.name);
                    }
                    _ => ()
                }
            }
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

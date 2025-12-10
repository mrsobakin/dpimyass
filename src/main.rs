mod config;
mod connmap;

use std::net::{Ipv4Addr, IpAddr};
use std::sync::Arc;
use std::{
    error::Error,
    net::SocketAddr,
};

use tokio::net::UdpSocket;
use tokio::task::JoinSet;
use tokio::time::{Timeout, Duration};
use std::future::Future;

use crate::config::*;
use crate::connmap::ConnectionMap;


const LOCAL: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0);


trait TimeoutExt<F> where F: Future {
    fn timeout(self, duration: Duration) -> Timeout<F>;
}

impl<F> TimeoutExt<F> for F where F: Future {
    fn timeout(self, duration: Duration) -> Timeout<F> {
        tokio::time::timeout(duration, self)
    }
}

fn xor_obfuscate(data: &mut [u8], cfg: &ObfsConfig) {
    let iter = data.iter_mut()
        .zip(cfg.key.iter().cycle());

    if let Some(first) = cfg.first {
        iter.take(first)
            .for_each(|(di, ki)| *di ^= ki);
    } else {
        iter.for_each(|(di, ki)| *di ^= ki);
    }
}


struct ServerHandler {
    config: ServerConfig,
    socket: UdpSocket,
    upstreams: ConnectionMap,
}

impl ServerHandler {
    async fn open_upstream(&'static self, downstream_addr: SocketAddr) -> std::io::Result<Arc<UdpSocket>> {
        let socket = UdpSocket::bind(LOCAL).await?;

        socket.connect(self.config.upstream.address)
            .timeout(self.config.upstream.timeout).await??;

        let socket = Arc::new(socket);

        tokio::spawn(self.forward_loop(socket.clone(), downstream_addr));

        Ok(socket)
    }

    async fn forward_loop(&'static self, upstream: Arc<UdpSocket>, downstream_addr: SocketAddr) -> Option<()> {
        println!("[{}] New connection from {downstream_addr}", self.config.name);

        loop {
            let mut buf = Vec::with_capacity(self.config.upstream.buffer);

            match upstream.recv_buf(&mut buf).timeout(self.config.upstream.timeout).await {
                Ok(Err(err)) => {
                    println!("[{}] Error while listening on {downstream_addr}: {err}", self.config.name);
                    return Some(());
                },
                Err(_) => {
                    println!("[{}] Connection timed out ({downstream_addr})", self.config.name);
                    return Some(());
                },
                _ => {},
            }

            tokio::spawn(async move {
                xor_obfuscate(&mut buf, &self.config.obfs);
                let _ = self.socket.send_to(&buf, downstream_addr)
                    .timeout(self.config.relay.timeout).await;
            });
        }
    }

    pub async fn listen(self) {
        let sself = Box::leak(Box::new(self));

        loop {
            let mut buf = Vec::with_capacity(sself.config.relay.buffer);
            let Ok((_, from)) = sself.socket.recv_buf_from(&mut buf).await else { continue };

            let sself_ref: &_ = sself;
            tokio::spawn(async move {
                xor_obfuscate(&mut buf, &sself_ref.config.obfs);

                let upstream = sself_ref.upstreams.get_or(from, async {
                    sself_ref.open_upstream(from).await
                }).await;

                let upstream = match upstream {
                    Ok(u) => u,
                    Err(err) => {
                        println!("[{}] Error opening upstream: {err}", sself_ref.config.name);
                        return;
                    }
                };

                match upstream.send(&buf).timeout(sself_ref.config.upstream.timeout).await {
                    Ok(Err(err)) => println!("[{}] Error sending packet: {err}", sself_ref.config.name),
                    Err(_) => println!("[{}] Timeout sending packet", sself_ref.config.name),
                    _ => {},
                };
            });
        }
    }

    pub async fn bind(config: ServerConfig) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(config.relay.address).await?;
        println!("[{}] Listening on {:?}", config.name, socket.local_addr().unwrap());

        Ok(Self {
            config,
            socket,
            upstreams: ConnectionMap::new(),
        })
    }
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config: Config = {
        let config_file = std::env::args().nth(1).unwrap_or_else(|| "config.toml".to_owned());

        let s = std::fs::read_to_string(config_file)?;
        toml::from_str(&s)?
    };

    let mut join_set = JoinSet::new();

    for server_config in config.servers {
        let name = server_config.name.clone();

        match ServerHandler::bind(server_config).await {
            Ok(handler) => join_set.spawn(handler.listen()),
            Err(err) => {
                println!("[{}] Failed to bind: {}", name, err);
                continue;
            },
        };
    }

    while let Some(_) = join_set.join_next().await {}

    Ok(())
}

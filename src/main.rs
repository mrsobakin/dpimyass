mod config;

use std::collections::HashMap;
use std::net::{Ipv4Addr, IpAddr};
use std::sync::{Arc, Weak};
use std::{
    error::Error,
    net::SocketAddr,
};

use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::sync::RwLockWriteGuard;
use tokio::task::JoinSet;
use tokio::time::timeout;

use crate::config::*;


const LOCAL: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0);


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
    // TODO: clean up hashmap after connections time out.
    // As of now, entries will forever stay in memory.
    // It's not *so* bad, but it's still a memory leak.
    upstreams: RwLock<HashMap<SocketAddr, Mutex<Weak<UdpSocket>>>>,
    upgrade_sem: Mutex<()>,
}

impl ServerHandler {
    async fn open_upstream(&'static self, downstream_addr: SocketAddr) -> std::io::Result<Arc<UdpSocket>> {
        let socket = UdpSocket::bind(LOCAL).await?;
        timeout(self.config.upstream.timeout, socket.connect(self.config.upstream.address)).await??;

        let socket = Arc::new(socket);

        tokio::spawn(self.forward_loop(socket.clone(), downstream_addr));

        Ok(socket)
    }

    async fn forward_loop(&'static self, upstream: Arc<UdpSocket>, downstream_addr: SocketAddr) -> Option<()> {
        println!("[{}] New connection from {downstream_addr}", self.config.name);

        loop {
            let mut buf = Vec::with_capacity(self.config.upstream.buffer);

            match timeout(self.config.upstream.timeout, upstream.recv_buf(&mut buf)).await {
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
                let _ = timeout(self.config.relay.timeout, self.socket.send_to(&buf, downstream_addr)).await;
            });
        }
    }

    async fn get_upstream_for(&'static self, downstream_addr: SocketAddr) -> std::io::Result<Arc<UdpSocket>> {
        let map_rlock = self.upstreams.read().await;

        if let Some(slot) = map_rlock.get(&downstream_addr) {
            let mut slot_lock = slot.lock().await;

            return match slot_lock.upgrade() {
                Some(udp) => Ok(udp),
                None => {
                    let udp = self.open_upstream(downstream_addr).await?;
                    *slot_lock = Arc::downgrade(&udp);
                    Ok(udp)
                }
            }
        };

        // Safely upgrade rlock to wlock
        let mut map_wlock = {
            let lock = self.upgrade_sem.lock().await;
            drop(map_rlock);
            let map_wlock = self.upstreams.write().await;
            drop(lock);
            map_wlock
        };

        // TODO: find a way to get slot directly, without hasing
        // multiple times and having a window between locks.
        map_wlock.insert(downstream_addr, Weak::default().into());
        let slot = RwLockWriteGuard::downgrade_map(map_wlock, |map| {
            map.get(&downstream_addr).expect("unreachable")
        });

        let mut slot_lock = slot.lock().await;

        // Check whether slot is still empty (weak),
        // as it might have been initialized inbetween
        // outer lock downgrading and inner lock acquiring.
        if let Some(udp) = slot_lock.upgrade() {
            return Ok(udp);
        }

        let udp = self.open_upstream(downstream_addr).await?;
        *slot_lock = Arc::downgrade(&udp);
        Ok(udp)
    }

    pub async fn listen(self) {
        let sself = Box::leak(Box::new(self));

        loop {
            let mut buf = Vec::with_capacity(sself.config.relay.buffer);
            let Ok((_, from)) = sself.socket.recv_buf_from(&mut buf).await else { continue };

            let sself_ref: &_ = sself;
            tokio::spawn(async move {
                xor_obfuscate(&mut buf, &sself_ref.config.obfs);

                let upstream = match sself_ref.get_upstream_for(from).await {
                    Ok(u) => u,
                    Err(err) => {
                        println!("[{}] Error opening upstream: {err}", sself_ref.config.name);
                        return;
                    }
                };

                match timeout(sself_ref.config.upstream.timeout, upstream.send(&buf)).await {
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
            upstreams: Default::default(),
            upgrade_sem: Default::default(),
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

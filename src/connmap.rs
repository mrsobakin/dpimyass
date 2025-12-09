use std::future::Future;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::net::SocketAddr;
use std::sync::{Arc, Weak};
use hashbrown::HashTable;
use tokio::net::UdpSocket;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::sync::broadcast;
use tokio::sync::{Mutex, MutexGuard};

#[derive(Debug)]
enum State {
    Value(Weak<UdpSocket>),
    Generating(Sender<Arc<UdpSocket>>),
    None,
}

impl State {
    fn is_dead(&self) -> bool {
        match self {
            State::Value(weak) => weak.strong_count() == 0,
            State::Generating(_) => false,
            State::None => true,
        }
    }
}

#[derive(Debug)]
struct Entry {
    addr: SocketAddr,
    state: State,
}

fn calc_hash<T: Hash>(x: T) -> u64 {
    let mut s = DefaultHasher::new();
    x.hash(&mut s);
    s.finish()
}

fn get_entry_replacing_dead(map: &mut HashTable<Entry>, addr: SocketAddr) -> &mut Entry {
    let hash = calc_hash(addr);

    // If we found a cell with our key - use it.
    if let Some(entry) = map.find_entry(hash, |e| e.addr == addr).ok() {
        unsafe {
            return std::mem::transmute::<_, _>(entry.into_mut());
        }
    }

    // Otherwise, search for any dead cell and make it ours.
    if let Some(mut entry) = map.find_entry(hash, |e| e.state.is_dead()).ok() {
        *entry.get_mut() = Entry {
            addr: addr,
            state: State::None,
        };

        unsafe {
            return std::mem::transmute::<_, _>(entry.into_mut());
        }
    }

    // If we have no hash collisions, just use a new cell.
    map.insert_unique(
        hash,
        Entry{
            addr: addr,
            state: State::None,
        },
        |e| calc_hash(e.addr)
    ).into_mut()
}

pub struct ConnectionMap {
    connections: Mutex<HashTable<Entry>>,
}

impl ConnectionMap {
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashTable::new()),
        }
    }

    pub async fn get_or<F>(&self, addr: SocketAddr, fut: F) -> std::io::Result<Arc<UdpSocket>>
    where
        F: Future<Output = std::io::Result<Arc<UdpSocket>>>
    {
        enum Status {
            Send(Sender<Arc<UdpSocket>>),
            Wait(Receiver<Arc<UdpSocket>>),
        }

        // Stage 1: Try to acquire current status
        let status = {
            let lock = self.connections.lock().await;

            let mut entry = MutexGuard::map(lock, |map| {
                get_entry_replacing_dead(map, addr)
            });

            if let State::Value(weak) = &entry.state {
                if let Some(arc) = weak.upgrade() {
                    return Ok(arc);
                }
            }

            if let State::Generating(tx) = &entry.state {
                Status::Wait(tx.subscribe())
            } else {
                let (tx, _) = broadcast::channel::<Arc<UdpSocket>>(1);
                entry.state = State::Generating(tx.clone());
                Status::Send(tx)
            }
        };

        // Stage 2: Wait for somebody to open socket, or do it ourselves
        match status {
            Status::Wait(mut rx) => {
                Ok(rx.recv().await.unwrap())
            }
            Status::Send(tx) => {
                let hash = calc_hash(addr);

                let socket = fut.await;

                let mut lock = self.connections.lock().await;

                let entry = lock
                    .find_mut(hash, |e| e.addr == addr)
                    .expect("nobody should modify our entry");

                match socket {
                    Ok(socket) => {
                        entry.state = State::Value(Arc::downgrade(&socket));

                        let _ = tx.send(socket.clone());

                        Ok(socket)
                    }
                    Err(err) => {
                        entry.state = State::None;
                        Err(err)
                    }
                }
            }
        }
    }
}

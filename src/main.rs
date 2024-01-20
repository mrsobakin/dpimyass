use std::{error::Error, net::SocketAddr, str::FromStr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::timeout,
};
use udp_stream::UdpStream;

const UDP_BUFFER_SIZE: usize = 16 * 1024;
const UDP_TIMEOUT: Duration = Duration::from_secs(60);
const XOR_KEY: [u8; 3] = [69, 42, 239];

fn obfuscate(buf: &mut [u8]) {
    for (bi, ki) in buf.iter_mut().zip(XOR_KEY.iter().cycle()) {
        *bi ^= ki
    }
}

async fn forward_loop(mut upstream: UdpStream, mut downstream: UdpStream) -> Result<(), std::io::Error> {
    let mut upbuf = vec![0u8; UDP_BUFFER_SIZE];
    let mut downbuf = vec![0u8; UDP_BUFFER_SIZE];

    loop {
        tokio::select! {
            n = timeout(UDP_TIMEOUT, upstream.read(&mut upbuf)) => {
                let n = n??;
                println!("Incoming from upstream:\n{:?}\n", &upbuf[0..n]);
                obfuscate(&mut upbuf);
                downstream.write_all(&upbuf[0..n]).await?;
            },
            n = timeout(UDP_TIMEOUT, downstream.read(&mut downbuf)) => {
                let n = n??;
                println!("Incoming from downstream:\n{:?}\n", &downbuf[0..n]);
                obfuscate(&mut downbuf);
                upstream.write_all(&downbuf[0..n]).await?;
            }
        };
    }

    #[allow(unreachable_code)]
    Ok::<(), std::io::Error>(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let bind_addr = std::env::args().nth(1).expect("no bind address given");
    let bind_addr = SocketAddr::from_str(&bind_addr)?;

    let downstream_addr = std::env::args().nth(2).expect("no downstream address given");
    let downstream_addr = SocketAddr::from_str(&downstream_addr)?;

    let listener = udp_stream::UdpListener::bind(bind_addr).await?;

    loop {
        let (upstream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let downstream = udp_stream::UdpStream::connect(downstream_addr).await?;

            if let Err(e) = forward_loop(upstream, downstream).await {
                println!("Error: {e:?}");
            }

            Ok::<(), std::io::Error>(())
        });
    }
}

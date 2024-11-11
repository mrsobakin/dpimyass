# 🖕🕵 DPIMyAss

DPIMyAss is a simple UDP proxy designed for bypassing DPI with close-to-zero overhead.

![Funny image showing network architecture](./assets/network.svg)

## Why? 🤔

I made this proxy to restore the wireguard functionality in places where it was blocked. DPIMyAss is way simpler than the other solutions like, say, shadowsocks, and it does not require messing with the ip interfaces to get it running. All you have to do to set it up with wireguard is run this proxy on both your server and client, and change the endpoint to local proxy address in your wireguard config (Also you might have to do [this](#wireguard-specific-solution)).

DPIMyAss also does not create any additional overhead on the network. The forwarded packets stay the exact same size they were, and no new packets are created.

## Features 🚀

- **UDP Obfuscation:** DPIMyAss mangles packets, making the underlying protocol unrecognizable to the DPI.
- **Simplicity:** DPIMyAss is extremely simple and small. It's not trying to be what it isn't - there are no custom protocols or complex encryption here.
- **Speed:** DPIMyAss uses simple XOR encryption, which results in almost zero processing overhead.

## Getting Started 🛠️

These instructions will help you set up and run DPIMyAss on your local machine and server.

### Build it yourself 🔨
1. Clone this repo and `cd` into it
2. Build the project:
```bash
cargo build --release
```
3. Run DPIMyAss:
```bash
./target/build/dpimyass [config.toml]
```

### Docker 🐋
If you just want to use the prebuilt image:
1. Clone this repo and `cd` into it, or manually copy `docker-compose.yml` and config from the project root.
2. Edit the config file `./config/config.toml`
3. Run `docker-compose up -d`, and let docker do all the magic!

If you want to build the image yourself:
1. Clone this repo and `cd` into it.
2. Run `docker build . -t dpimyass`
3. Edit the config file `./config/config.toml`
4. Edit the `docker-compose.yml` to use `dpimyass` image instead of `ghcr.io/mrsobakin/dpimyass`.
5. Run `docker-compose up -d`.

### Arch Linux 😈

Also, if use Arch linux, you can just run `makepkg -si` in the project root. It will automatically install DPIMyAss systemd service for you.

## Configuration ⚙️

DPIMyAss uses a TOML configuration file to specify its settings. Below is an example configuration:

```toml
[[servers]]
name = "Example bridge"
key = [239, 42, 13, 69]

[servers.relay]
address = "0.0.0.0:1337"
buffer = 65536
timeout = 60

[servers.upstream]
address = "example.com:1337"
buffer = 65536
timeout = 60

[[servers]]
name = "Another bridge"
key = [4, 5, 11]
first = 64  # Obfuscate only the first 64 bytes

[servers.relay]
address = "0.0.0.0:1338"
buffer = 65536
timeout = 120

[servers.upstream]
address = "endpoint2.exmaple.com:443"
buffer = 65536
timeout = 120
```

## Troubleshooting 🪛
You might encounter a problem when trying to use VPN over DPIMyAss hosted on the same machine. To fix this, you have to add an entry to a routing table with the endpoint IP bypassing your VPN. Here are a few examples of how to do this:

### Wireguard-specific solution

If your upstream address falls inside the ips listed in wireguard's `AllowedIPs`, the packets DPIMyAss sends will be routed over VPN too, and thus they will be stuck in a network loop.

The simplest way to fix this is to exclude your upstream endpoint ip address from the wireguard's `AllowedIPs`. This can be done with any wireguard allowed ips calculator, for example with [this one](https://www.procustodibus.com/blog/2021/03/wireguard-allowedips-calculator/).


### Windows
1. Disable your VPN.
2. Open PowerShell/CMD as an Administrator.
3. Run the following command:
```powershell
route PRINT
```
Now take a look at the **IPv4 Route Table**:
```
IPv4 Route Table

===========================================================================
Active Routes:
Network Destination        Netmask          Gateway       Interface  Metric
          0.0.0.0          0.0.0.0       10.161.8.1       10.161.8.2     35
       10.161.8.0    255.255.252.0         On-link        10.161.8.2    291
       10.161.8.2  255.255.255.255         On-link        10.161.8.2    291
    10.161.11.255  255.255.255.255         On-link        10.161.8.2    291
        127.0.0.0        255.0.0.0         On-link         127.0.0.1    331

        127.0.0.1  255.255.255.255         On-link         127.0.0.1    331
  127.255.255.255  255.255.255.255         On-link         127.0.0.1    331
       172.25.0.0    255.255.240.0         On-link        172.25.0.1   5256
       172.25.0.1  255.255.255.255         On-link        172.25.0.1   5256
    172.25.15.255  255.255.255.255         On-link        172.25.0.1   5256
        224.0.0.0        240.0.0.0         On-link         127.0.0.1    331
        224.0.0.0        240.0.0.0         On-link        172.25.0.1   5256
        224.0.0.0        240.0.0.0         On-link        10.161.8.2    291

  255.255.255.255  255.255.255.255         On-link         127.0.0.1    331
  255.255.255.255  255.255.255.255         On-link        172.25.0.1   5256
  255.255.255.255  255.255.255.255         On-link        10.161.8.2    291
===========================================================================
```
Notice the line with Network Destination `0.0.0.0`, and remember the **Gateway** IP (`10.161.8.1` in this case).

4. Execute the following command:
```powershell
route ADD <endpoint_ip> MASK 255.255.255.255 <gateway_ip>
```
where `<endpoint_ip>` is the IP of your VPN, and `<gateway_ip>` is the IP from step 3.

5. If everything has worked, you will see `OK!` in your terminal window. You can close it now and try connecting again.

### Linux
For this example, we will use Debian 12, although the commands listed below should work on *most* modern distributions. For older distros, I advise you to consult your distro's manual.
1. Disable your VPN.
2. Open up your favorite terminal emulator and run `ip route`:
```bash
ip route
```
Example output of that command:
```
default via 172.25.0.1 dev eth0 proto kernel
172.25.0.0/20 dev eth0 proto kernel scope link src 172.25.4.60
```
Remember the **default** gateway (`172.25.0.1` in this case).

3. Run the following command:
```bash
sudo ip route add <endpoint_ip> via <gateway_ip>
```
If the command above has worked, you won't see anything in your terminal.

4. Verify that the route has been created, by running:
```bash
ip route
```
Route you have just created should be listed
```
default via 172.25.0.1 dev eth0 proto kernel
1.1.1.1 via 172.25.0.1 dev eth0                                  <-- This is the one!
172.25.0.0/20 dev eth0 proto kernel scope link src 172.25.4.60
```
Done! Now you can try to connect again.

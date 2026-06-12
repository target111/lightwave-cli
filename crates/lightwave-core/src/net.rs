//! UDP client plumbing shared by the streaming presets (music, ambilight).

use std::io;
use std::net::{Ipv6Addr, SocketAddr, ToSocketAddrs, UdpSocket};

use anyhow::{Context, Result, anyhow};

/// Resolve `target` (preferring IPv4 addresses) and return a UDP socket
/// bound to the matching address family and connected to it.
pub fn connect_udp(target: &str) -> Result<UdpSocket> {
    let target = resolve_target(target)?;
    let bind_addr: SocketAddr = if target.is_ipv4() {
        ([0, 0, 0, 0], 0).into()
    } else {
        (Ipv6Addr::UNSPECIFIED, 0).into()
    };

    let socket = UdpSocket::bind(bind_addr).context("binding UDP socket")?;
    socket
        .connect(target)
        .with_context(|| format!("connecting UDP socket to {target}"))?;

    Ok(socket)
}

/// Send one packet on a connected socket. ConnectionRefused is ignored:
/// the server may simply not be listening yet, and streaming should
/// continue until it is.
pub fn send_packet(socket: &UdpSocket, packet: &[u8]) -> Result<()> {
    if let Err(err) = socket.send(packet)
        && err.kind() != io::ErrorKind::ConnectionRefused
    {
        return Err(err).context("sending UDP packet");
    }

    Ok(())
}

fn resolve_target(target: &str) -> Result<SocketAddr> {
    let addrs: Vec<SocketAddr> = target
        .to_socket_addrs()
        .with_context(|| format!("resolving UDP target {target:?}"))?
        .collect();

    addrs
        .iter()
        .find(|addr| addr.is_ipv4())
        .or_else(|| addrs.first())
        .copied()
        .ok_or_else(|| anyhow!("UDP target {target:?} resolved to no addresses"))
}

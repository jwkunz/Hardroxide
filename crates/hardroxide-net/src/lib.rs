//! A small IPv4 network stack: Ethernet, ARP, ICMP echo, UDP, a DHCP client and TCP, over a [`Device`].
//!
//! `no_std`, no allocation, no dependencies. The stack owns its buffers (sized by const generics) and its sockets; the application calls
//! [`Stack::poll`] with the current time in milliseconds as often as it can and uses the sockets between polls. See the
//! [reference](https://jwkunz.github.io/Hardroxide/book/reference/net.html).

#![no_std]

pub mod dhcp;
pub mod http;
pub mod tcp;
pub mod wire;

use tcp::{MSS, SegOut, State, TcpSocket};
use wire::*;

/// A network interface: frames in and out. `receive` and `transmit` never block.
pub trait Device {
    /// The interface's hardware address.
    fn mac(&self) -> Mac;
    /// Copy the next received frame (without the CRC) into `buf`; `None` if there is none.
    fn receive(&mut self, buf: &mut [u8]) -> Option<usize>;
    /// Queue a frame (without the CRC) for transmission; `false` if there is no room (try again later).
    fn transmit(&mut self, frame: &[u8]) -> bool;
}

/// A fixed address configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaticConfig {
    pub ip: Ip4,
    pub netmask: Ip4,
    pub gateway: Option<Ip4>,
}

/// How the interface gets its address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Config {
    Static(StaticConfig),
    /// Ask a DHCP server; if none answers within `fallback_after_ms`, use `fallback` (and keep asking).
    Dhcp {
        fallback: Option<StaticConfig>,
        fallback_after_ms: u64,
    },
}

/// Counters, for debugging and the status pages.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub rx_frames: u32,
    pub tx_frames: u32,
    pub tx_dropped: u32,
    pub bad_packets: u32,
    pub icmp_echo_replied: u32,
    pub tcp_segments_in: u32,
    pub tcp_segments_out: u32,
    pub tcp_resets_sent: u32,
}

/// The largest UDP payload a [`UdpSocket`] holds.
pub const UDP_PAYLOAD: usize = 512;

/// A UDP socket: one received datagram is kept until read.
pub struct UdpSocket {
    port: u16,
    open: bool,
    pending: bool,
    from: (Ip4, u16),
    len: usize,
    data: [u8; UDP_PAYLOAD],
}

impl UdpSocket {
    const fn new() -> Self {
        UdpSocket {
            port: 0,
            open: false,
            pending: false,
            from: ([0; 4], 0),
            len: 0,
            data: [0; UDP_PAYLOAD],
        }
    }
    /// Receive datagrams for `port`.
    pub fn bind(&mut self, port: u16) {
        self.port = port;
        self.open = true;
        self.pending = false;
    }
    pub fn close(&mut self) {
        self.open = false;
    }
    /// The waiting datagram, if any: copies it into `buf` and returns `(source address, source port, length)`.
    pub fn recv_from(&mut self, buf: &mut [u8]) -> Option<(Ip4, u16, usize)> {
        if !self.pending {
            return None;
        }
        self.pending = false;
        let n = self.len.min(buf.len());
        buf[..n].copy_from_slice(&self.data[..n]);
        Some((self.from.0, self.from.1, n))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DhcpState {
    Off,
    Discovering,
    Requesting { offered: Ip4, server: Ip4 },
    Bound,
}

#[derive(Clone, Copy)]
struct ArpEntry {
    ip: Ip4,
    mac: Mac,
    valid: bool,
}

const ARP_ENTRIES: usize = 8;
const FRAME_MAX: usize = 1536;

/// The interface and its sockets: `T` TCP sockets with `BUF`-byte buffers each way, `U` UDP sockets.
pub struct Stack<D: Device, const T: usize, const U: usize, const BUF: usize> {
    rx_buf: [u8; FRAME_MAX],
    inner: Inner<D, T, U, BUF>,
}

struct Inner<D: Device, const T: usize, const U: usize, const BUF: usize> {
    dev: D,
    mac: Mac,
    ip: Ip4,
    netmask: Ip4,
    gateway: Option<Ip4>,
    arp: [ArpEntry; ARP_ENTRIES],
    arp_next: usize,
    last_arp_request: (Ip4, u64),
    tx_buf: [u8; FRAME_MAX],
    tcp: [TcpSocket<BUF>; T],
    udp: [UdpSocket; U],
    config: Config,
    dhcp: DhcpState,
    dhcp_xid: u32,
    dhcp_next_send: u64,
    dhcp_started: u64,
    ip_id: u16,
    rng: u32,
    stats: Stats,
}

impl<D: Device, const T: usize, const U: usize, const BUF: usize> Stack<D, T, U, BUF> {
    pub fn new(dev: D, config: Config, now_ms: u64) -> Self {
        let mac = dev.mac();
        let mut inner = Inner {
            dev,
            mac,
            ip: [0; 4],
            netmask: [0; 4],
            gateway: None,
            arp: [ArpEntry {
                ip: [0; 4],
                mac: [0; 6],
                valid: false,
            }; ARP_ENTRIES],
            arp_next: 0,
            last_arp_request: ([0; 4], 0),
            tx_buf: [0; FRAME_MAX],
            tcp: [const { TcpSocket::new() }; T],
            udp: [const { UdpSocket::new() }; U],
            config,
            dhcp: DhcpState::Off,
            dhcp_xid: 0,
            dhcp_next_send: 0,
            dhcp_started: now_ms,
            ip_id: 1,
            rng: u32::from_be_bytes([mac[2], mac[3], mac[4], mac[5]])
                ^ (now_ms as u32).wrapping_mul(2_654_435_761)
                | 1,
            stats: Stats::default(),
        };
        match config {
            Config::Static(s) => inner.apply(s),
            Config::Dhcp { .. } => {
                inner.dhcp = DhcpState::Discovering;
                inner.dhcp_xid = inner.random();
            }
        }
        Stack {
            rx_buf: [0; FRAME_MAX],
            inner,
        }
    }

    /// The interface's address (all zeros until it has one).
    pub fn ip(&self) -> Ip4 {
        self.inner.ip
    }
    /// The interface has an address.
    pub fn is_configured(&self) -> bool {
        self.inner.ip != [0; 4]
    }
    /// The address came from a DHCP server (and not from the fallback).
    pub fn dhcp_bound(&self) -> bool {
        self.inner.dhcp == DhcpState::Bound
    }
    pub fn gateway(&self) -> Option<Ip4> {
        self.inner.gateway
    }
    pub fn netmask(&self) -> Ip4 {
        self.inner.netmask
    }
    pub fn stats(&self) -> Stats {
        self.inner.stats
    }
    pub fn device(&mut self) -> &mut D {
        &mut self.inner.dev
    }
    pub fn tcp(&mut self, i: usize) -> &mut TcpSocket<BUF> {
        &mut self.inner.tcp[i]
    }
    pub fn udp(&mut self, i: usize) -> &mut UdpSocket {
        &mut self.inner.udp[i]
    }

    /// Open a TCP connection from socket `i` to `remote`.
    pub fn tcp_connect(&mut self, i: usize, local_port: u16, remote: Ip4, remote_port: u16) {
        let iss = self.inner.random();
        self.inner.tcp[i].connect(local_port, remote, remote_port, iss);
    }

    /// Send a datagram from UDP socket `i` (its bound port is the source port). `false` if it could not be sent now
    /// (no address yet, the next hop's hardware address is not known yet, or the device is full).
    pub fn udp_send(&mut self, now_ms: u64, i: usize, to: Ip4, to_port: u16, data: &[u8]) -> bool {
        let port = self.inner.udp[i].port;
        self.inner.send_udp(now_ms, to, port, to_port, data)
    }

    /// Process received frames, run the timers and send what the sockets and protocols have to send.
    pub fn poll(&mut self, now_ms: u64) {
        for _ in 0..16 {
            let Some(n) = self.inner.dev.receive(&mut self.rx_buf) else {
                break;
            };
            self.inner.stats.rx_frames += 1;
            let frame = &self.rx_buf[..n.min(FRAME_MAX)];
            self.inner.handle_frame(now_ms, frame);
        }
        self.inner.dhcp_tick(now_ms);
        self.inner.tcp_output(now_ms);
    }
}

fn on_link(ip: Ip4, our: Ip4, mask: Ip4) -> bool {
    (0..4).all(|i| ip[i] & mask[i] == our[i] & mask[i])
}

impl<D: Device, const T: usize, const U: usize, const BUF: usize> Inner<D, T, U, BUF> {
    fn random(&mut self) -> u32 {
        // xorshift32: unpredictable enough for sequence numbers on a trusted network, not for cryptography.
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    fn apply(&mut self, s: StaticConfig) {
        self.ip = s.ip;
        self.netmask = s.netmask;
        self.gateway = s.gateway;
    }

    fn broadcast(&self) -> Ip4 {
        let mut b = self.ip;
        for (x, m) in b.iter_mut().zip(self.netmask) {
            *x |= !m;
        }
        b
    }

    fn learn(&mut self, ip: Ip4, mac: Mac) {
        if ip == [0; 4] || ip == [255; 4] {
            return;
        }
        if let Some(e) = self.arp.iter_mut().find(|e| e.valid && e.ip == ip) {
            e.mac = mac;
            return;
        }
        self.arp[self.arp_next] = ArpEntry {
            ip,
            mac,
            valid: true,
        };
        self.arp_next = (self.arp_next + 1) % ARP_ENTRIES;
    }

    fn transmit(&mut self, len: usize) -> bool {
        let ok = self.dev.transmit(&self.tx_buf[..len]);
        if ok {
            self.stats.tx_frames += 1;
        } else {
            self.stats.tx_dropped += 1;
        }
        ok
    }

    fn send_arp(&mut self, op: u16, tha: Mac, tpa: Ip4, eth_dst: Mac) {
        emit_eth(&mut self.tx_buf, eth_dst, self.mac, ETHERTYPE_ARP);
        let a = Arp {
            op,
            sha: self.mac,
            spa: self.ip,
            tha,
            tpa,
        };
        let n = emit_arp(&mut self.tx_buf[ETH_HEADER..], &a);
        self.transmit(ETH_HEADER + n);
    }

    /// The hardware address to send to for `dst`, asking for it if unknown.
    fn resolve(&mut self, now: u64, dst: Ip4) -> Option<Mac> {
        if dst == [255; 4] || (self.ip != [0; 4] && dst == self.broadcast()) {
            return Some(BROADCAST_MAC);
        }
        let hop = if on_link(dst, self.ip, self.netmask) {
            dst
        } else {
            self.gateway?
        };
        if let Some(e) = self.arp.iter().find(|e| e.valid && e.ip == hop) {
            return Some(e.mac);
        }
        if self.last_arp_request.0 != hop || now >= self.last_arp_request.1 + 500 {
            self.last_arp_request = (hop, now);
            self.send_arp(ARP_REQUEST, [0; 6], hop, BROADCAST_MAC);
        }
        None
    }

    /// Send an IP packet whose payload is already in `tx_buf[34..34 + payload_len]`.
    fn send_ip(&mut self, mac: Mac, dst: Ip4, proto: u8, payload_len: usize) -> bool {
        emit_eth(&mut self.tx_buf, mac, self.mac, ETHERTYPE_IPV4);
        self.ip_id = self.ip_id.wrapping_add(1);
        emit_ipv4(
            &mut self.tx_buf[ETH_HEADER..],
            self.ip,
            dst,
            proto,
            payload_len,
            self.ip_id,
        );
        self.transmit(ETH_HEADER + IP_HEADER + payload_len)
    }

    fn send_udp(&mut self, now: u64, to: Ip4, src_port: u16, dst_port: u16, data: &[u8]) -> bool {
        if self.ip == [0; 4] || data.len() > FRAME_MAX - ETH_HEADER - IP_HEADER - 8 {
            return false;
        }
        let Some(mac) = self.resolve(now, to) else {
            return false;
        };
        let off = ETH_HEADER + IP_HEADER;
        self.tx_buf[off + 8..off + 8 + data.len()].copy_from_slice(data);
        emit_udp(
            &mut self.tx_buf[off..],
            self.ip,
            to,
            src_port,
            dst_port,
            data.len(),
        );
        self.send_ip(mac, to, PROTO_UDP, 8 + data.len())
    }

    fn handle_frame(&mut self, now: u64, frame: &[u8]) {
        let Some(eth) = parse_eth(frame) else {
            self.stats.bad_packets += 1;
            return;
        };
        if eth.dst != self.mac && eth.dst != BROADCAST_MAC {
            return;
        }
        match eth.ethertype {
            ETHERTYPE_ARP => self.handle_arp(eth.payload),
            ETHERTYPE_IPV4 => match parse_ipv4(eth.payload) {
                Some(ip) => self.handle_ip(now, eth.src, &ip),
                None => self.stats.bad_packets += 1,
            },
            _ => {}
        }
    }

    fn handle_arp(&mut self, p: &[u8]) {
        let Some(a) = parse_arp(p) else { return };
        if self.ip == [0; 4] {
            return;
        }
        // Learn the sender if it is on our network, or already known.
        if on_link(a.spa, self.ip, self.netmask) {
            self.learn(a.spa, a.sha);
        }
        if a.op == ARP_REQUEST && a.tpa == self.ip {
            self.send_arp(ARP_REPLY, a.sha, a.spa, a.sha);
        }
    }

    fn handle_ip(&mut self, now: u64, src_mac: Mac, ip: &Ipv4) {
        let ours = ip.dst == self.ip
            || ip.dst == [255; 4]
            || (self.ip != [0; 4] && ip.dst == self.broadcast());
        // Before we have an address only DHCP replies (UDP to port 68) are taken.
        let dhcp_reply = ip.proto == PROTO_UDP
            && ip.payload.len() >= 4
            && u16::from_be_bytes([ip.payload[2], ip.payload[3]]) == dhcp::CLIENT_PORT;
        if !(ours || (self.ip == [0; 4] && dhcp_reply)) {
            return;
        }
        if self.ip != [0; 4] && on_link(ip.src, self.ip, self.netmask) {
            self.learn(ip.src, src_mac);
        }
        match ip.proto {
            PROTO_ICMP => self.handle_icmp(now, src_mac, ip),
            PROTO_UDP => {
                if let Some(u) = parse_udp(ip.payload, ip.src, ip.dst) {
                    self.handle_udp(now, ip, &u);
                } else {
                    self.stats.bad_packets += 1;
                }
            }
            PROTO_TCP if ip.dst == self.ip => {
                if let Some(t) = parse_tcp(ip.payload, ip.src, ip.dst) {
                    self.handle_tcp(now, src_mac, ip, &t);
                } else {
                    self.stats.bad_packets += 1;
                }
            }
            _ => {}
        }
    }

    fn handle_icmp(&mut self, _now: u64, src_mac: Mac, ip: &Ipv4) {
        let p = ip.payload;
        // An echo request to our own address (not to a broadcast).
        if p.len() < 8
            || p[0] != 8
            || p[1] != 0
            || ip.dst != self.ip
            || checksum(p) != 0
            || p.len() > FRAME_MAX - ETH_HEADER - IP_HEADER
        {
            return;
        }
        let off = ETH_HEADER + IP_HEADER;
        self.tx_buf[off..off + p.len()].copy_from_slice(p);
        self.tx_buf[off] = 0; // echo reply
        self.tx_buf[off + 2..off + 4].fill(0);
        let c = checksum(&self.tx_buf[off..off + p.len()]);
        self.tx_buf[off + 2..off + 4].copy_from_slice(&c.to_be_bytes());
        if self.send_ip(src_mac, ip.src, PROTO_ICMP, p.len()) {
            self.stats.icmp_echo_replied += 1;
        }
    }

    fn handle_udp(&mut self, now: u64, ip: &Ipv4, u: &Udp) {
        if u.dst_port == dhcp::CLIENT_PORT && self.dhcp != DhcpState::Off {
            self.handle_dhcp(now, u.payload);
            return;
        }
        if ip.dst != self.ip && ip.dst != [255; 4] && ip.dst != self.broadcast() {
            return;
        }
        if let Some(s) = self.udp.iter_mut().find(|s| s.open && s.port == u.dst_port) {
            let n = u.payload.len().min(UDP_PAYLOAD);
            s.data[..n].copy_from_slice(&u.payload[..n]);
            s.len = n;
            s.from = (ip.src, u.src_port);
            s.pending = true;
        }
    }

    fn handle_tcp(&mut self, now: u64, src_mac: Mac, ip: &Ipv4, t: &Tcp) {
        self.stats.tcp_segments_in += 1;
        let iss = self.random();
        // An established connection first, then a listener.
        let idx = self
            .tcp
            .iter()
            .position(|s| s.state() != State::Listen && s.matches(ip.src, t))
            .or_else(|| {
                self.tcp
                    .iter()
                    .position(|s| s.state() == State::Listen && s.matches(ip.src, t))
            });
        match idx {
            Some(i) => self.tcp[i].on_segment(now, ip.src, t, iss),
            // A SYN for a port we serve but have no free socket for: stay silent, so the client retries (a reset would make it give up).
            None if t.flags & TCP_SYN != 0
                && self
                    .tcp
                    .iter()
                    .any(|s| s.state() != State::Closed && s.local_port() == t.dst_port) => {}
            None if t.flags & TCP_RST == 0 => {
                // Nobody here: refuse (RFC 793: the reset takes its numbers from the segment).
                let (seq, ack, flags) = if t.flags & TCP_ACK != 0 {
                    (t.ack, 0, TCP_RST)
                } else {
                    (
                        0,
                        t.seq
                            .wrapping_add(t.payload.len() as u32)
                            .wrapping_add(u32::from(t.flags & (TCP_SYN | TCP_FIN) != 0)),
                        TCP_RST | TCP_ACK,
                    )
                };
                let out = TcpOut {
                    src_port: t.dst_port,
                    dst_port: t.src_port,
                    seq,
                    ack,
                    flags,
                    window: 0,
                    mss: None,
                };
                let off = ETH_HEADER + IP_HEADER;
                let n = emit_tcp(&mut self.tx_buf[off..], self.ip, ip.src, &out, 0);
                if self.send_ip(src_mac, ip.src, PROTO_TCP, n) {
                    self.stats.tcp_resets_sent += 1;
                }
            }
            None => {}
        }
    }

    fn tcp_output(&mut self, now: u64) {
        for i in 0..T {
            if self.ip == [0; 4] {
                return;
            }
            for _ in 0..8 {
                let (remote, _) = self.tcp[i].remote();
                let state = self.tcp[i].state();
                if matches!(state, State::Closed | State::Listen) {
                    break;
                }
                // The peer's hardware address must be known before anything is taken from the socket.
                let Some(mac) = self.resolve(now, remote) else {
                    break;
                };
                let off = ETH_HEADER + IP_HEADER;
                let hl_max = 24;
                let seg: Option<SegOut> = {
                    let payload_area =
                        &mut self.tx_buf[off + hl_max..off + hl_max + usize::from(MSS)];
                    self.tcp[i].poll_tx(now, payload_area)
                };
                let Some(seg) = seg else { break };
                let out = TcpOut {
                    src_port: self.tcp[i].local_port(),
                    dst_port: self.tcp[i].remote().1,
                    seq: seg.seq,
                    ack: seg.ack,
                    flags: seg.flags,
                    window: seg.window,
                    mss: seg.mss,
                };
                let hl = tcp_header_len(&out);
                // The payload was written for a 24-byte header; move it if the header is 20 bytes.
                if hl != hl_max && seg.payload_len > 0 {
                    self.tx_buf
                        .copy_within(off + hl_max..off + hl_max + seg.payload_len, off + hl);
                }
                let n = emit_tcp(
                    &mut self.tx_buf[off..],
                    self.ip,
                    remote,
                    &out,
                    seg.payload_len,
                );
                if self.send_ip(mac, remote, PROTO_TCP, n) {
                    self.stats.tcp_segments_out += 1;
                }
            }
        }
    }

    fn dhcp_tick(&mut self, now: u64) {
        let Config::Dhcp {
            fallback,
            fallback_after_ms,
        } = self.config
        else {
            return;
        };
        match self.dhcp {
            DhcpState::Discovering | DhcpState::Requesting { .. } => {
                if now >= self.dhcp_next_send {
                    let mut p = [0u8; 300];
                    let (msg, asking) = match self.dhcp {
                        DhcpState::Requesting { offered, server } => {
                            (dhcp::REQUEST, Some((offered, server)))
                        }
                        _ => (dhcp::DISCOVER, None),
                    };
                    let n = dhcp::build(&mut p, msg, self.dhcp_xid, self.mac, asking);
                    // Broadcast from 0.0.0.0 (or from the fallback address) before we have a lease.
                    let off = ETH_HEADER + IP_HEADER;
                    self.tx_buf[off + 8..off + 8 + n].copy_from_slice(&p[..n]);
                    emit_udp(
                        &mut self.tx_buf[off..],
                        [0; 4],
                        [255; 4],
                        dhcp::CLIENT_PORT,
                        dhcp::SERVER_PORT,
                        n,
                    );
                    emit_eth(&mut self.tx_buf, BROADCAST_MAC, self.mac, ETHERTYPE_IPV4);
                    self.ip_id = self.ip_id.wrapping_add(1);
                    emit_ipv4(
                        &mut self.tx_buf[ETH_HEADER..],
                        [0; 4],
                        [255; 4],
                        PROTO_UDP,
                        8 + n,
                        self.ip_id,
                    );
                    self.transmit(ETH_HEADER + IP_HEADER + 8 + n);
                    self.dhcp_next_send = now + 2000;
                }
                if self.ip == [0; 4]
                    && now >= self.dhcp_started + fallback_after_ms
                    && let Some(s) = fallback
                {
                    self.apply(s);
                }
            }
            DhcpState::Off | DhcpState::Bound => {}
        }
    }

    fn handle_dhcp(&mut self, now: u64, p: &[u8]) {
        let Some(r) = dhcp::parse(p, self.mac) else {
            return;
        };
        if r.xid != self.dhcp_xid {
            return;
        }
        match (self.dhcp, r.msg_type) {
            (DhcpState::Discovering, dhcp::OFFER) => {
                if let Some(server) = r.server {
                    self.dhcp = DhcpState::Requesting {
                        offered: r.yiaddr,
                        server,
                    };
                    self.dhcp_next_send = now;
                }
            }
            (DhcpState::Requesting { .. }, dhcp::ACK) => {
                self.apply(StaticConfig {
                    ip: r.yiaddr,
                    netmask: r.mask.unwrap_or([255, 255, 255, 0]),
                    gateway: r.router,
                });
                self.dhcp = DhcpState::Bound;
            }
            (DhcpState::Requesting { .. }, dhcp::NAK) => {
                self.dhcp = DhcpState::Discovering;
                self.dhcp_xid = self.random();
                self.dhcp_next_send = now;
            }
            _ => {}
        }
    }
}

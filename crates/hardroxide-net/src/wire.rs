//! Wire formats: Ethernet, ARP, IPv4, UDP and TCP headers, and the Internet checksum.

/// A hardware (MAC) address.
pub type Mac = [u8; 6];
/// An IPv4 address.
pub type Ip4 = [u8; 4];

pub const BROADCAST_MAC: Mac = [0xFF; 6];
pub const ETH_HEADER: usize = 14;
pub const IP_HEADER: usize = 20;
pub const ETHERTYPE_IPV4: u16 = 0x0800;
pub const ETHERTYPE_ARP: u16 = 0x0806;
pub const PROTO_ICMP: u8 = 1;
pub const PROTO_TCP: u8 = 6;
pub const PROTO_UDP: u8 = 17;

pub const TCP_FIN: u8 = 0x01;
pub const TCP_SYN: u8 = 0x02;
pub const TCP_RST: u8 = 0x04;
pub const TCP_PSH: u8 = 0x08;
pub const TCP_ACK: u8 = 0x10;

/// Add the 16-bit big-endian words of `data` to a running one's-complement sum (an odd last byte is padded with zero).
pub fn checksum_add(mut sum: u32, data: &[u8]) -> u32 {
    let mut chunks = data.chunks_exact(2);
    for c in &mut chunks {
        sum += u32::from(u16::from_be_bytes([c[0], c[1]]));
    }
    if let [last] = chunks.remainder() {
        sum += u32::from(*last) << 8;
    }
    sum
}

/// Fold a running sum into the final checksum (the complement of the one's-complement sum).
pub fn checksum_finish(mut sum: u32) -> u16 {
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

/// The Internet checksum of `data`.
pub fn checksum(data: &[u8]) -> u16 {
    checksum_finish(checksum_add(0, data))
}

/// The sum of the pseudo-header that UDP and TCP checksums start from.
pub fn pseudo_sum(src: Ip4, dst: Ip4, proto: u8, len: usize) -> u32 {
    let mut s = checksum_add(0, &src);
    s = checksum_add(s, &dst);
    s + u32::from(proto) + len as u32
}

fn be16(b: &[u8], i: usize) -> u16 {
    u16::from_be_bytes([b[i], b[i + 1]])
}
fn be32(b: &[u8], i: usize) -> u32 {
    u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

/// A parsed Ethernet II frame.
#[derive(Debug, Clone, Copy)]
pub struct Eth<'a> {
    pub dst: Mac,
    pub src: Mac,
    pub ethertype: u16,
    pub payload: &'a [u8],
}

pub fn parse_eth(f: &[u8]) -> Option<Eth<'_>> {
    if f.len() < ETH_HEADER {
        return None;
    }
    Some(Eth {
        dst: f[0..6].try_into().ok()?,
        src: f[6..12].try_into().ok()?,
        ethertype: be16(f, 12),
        payload: &f[ETH_HEADER..],
    })
}

/// Write an Ethernet header at the start of `buf`.
pub fn emit_eth(buf: &mut [u8], dst: Mac, src: Mac, ethertype: u16) {
    buf[0..6].copy_from_slice(&dst);
    buf[6..12].copy_from_slice(&src);
    buf[12..14].copy_from_slice(&ethertype.to_be_bytes());
}

pub const ARP_REQUEST: u16 = 1;
pub const ARP_REPLY: u16 = 2;

/// An ARP packet for Ethernet and IPv4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arp {
    pub op: u16,
    pub sha: Mac,
    pub spa: Ip4,
    pub tha: Mac,
    pub tpa: Ip4,
}

pub fn parse_arp(p: &[u8]) -> Option<Arp> {
    if p.len() < 28 || be16(p, 0) != 1 || be16(p, 2) != ETHERTYPE_IPV4 || p[4] != 6 || p[5] != 4 {
        return None;
    }
    Some(Arp {
        op: be16(p, 6),
        sha: p[8..14].try_into().ok()?,
        spa: p[14..18].try_into().ok()?,
        tha: p[18..24].try_into().ok()?,
        tpa: p[24..28].try_into().ok()?,
    })
}

/// Write an ARP packet (28 bytes); returns the length.
pub fn emit_arp(buf: &mut [u8], a: &Arp) -> usize {
    buf[0..2].copy_from_slice(&1u16.to_be_bytes());
    buf[2..4].copy_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
    buf[4] = 6;
    buf[5] = 4;
    buf[6..8].copy_from_slice(&a.op.to_be_bytes());
    buf[8..14].copy_from_slice(&a.sha);
    buf[14..18].copy_from_slice(&a.spa);
    buf[18..24].copy_from_slice(&a.tha);
    buf[24..28].copy_from_slice(&a.tpa);
    28
}

/// A parsed IPv4 packet (header options skipped; fragments are refused).
#[derive(Debug, Clone, Copy)]
pub struct Ipv4<'a> {
    pub src: Ip4,
    pub dst: Ip4,
    pub proto: u8,
    pub ttl: u8,
    pub payload: &'a [u8],
}

pub fn parse_ipv4(p: &[u8]) -> Option<Ipv4<'_>> {
    if p.len() < IP_HEADER || p[0] >> 4 != 4 {
        return None;
    }
    let ihl = usize::from(p[0] & 15) * 4;
    let total = usize::from(be16(p, 2));
    if ihl < IP_HEADER || total < ihl || total > p.len() {
        return None;
    }
    // More-fragments flag or a fragment offset: we do not reassemble.
    if be16(p, 6) & 0x3FFF != 0 {
        return None;
    }
    if checksum(&p[..ihl]) != 0 {
        return None;
    }
    Some(Ipv4 {
        src: p[12..16].try_into().ok()?,
        dst: p[16..20].try_into().ok()?,
        proto: p[9],
        ttl: p[8],
        payload: &p[ihl..total],
    })
}

/// Write a 20-byte IPv4 header with its checksum; `payload_len` is the length of what follows.
pub fn emit_ipv4(buf: &mut [u8], src: Ip4, dst: Ip4, proto: u8, payload_len: usize, id: u16) {
    buf[0] = 0x45;
    buf[1] = 0;
    buf[2..4].copy_from_slice(&((IP_HEADER + payload_len) as u16).to_be_bytes());
    buf[4..6].copy_from_slice(&id.to_be_bytes());
    buf[6..8].copy_from_slice(&0x4000u16.to_be_bytes()); // don't fragment
    buf[8] = 64;
    buf[9] = proto;
    buf[10..12].fill(0);
    buf[12..16].copy_from_slice(&src);
    buf[16..20].copy_from_slice(&dst);
    let c = checksum(&buf[..IP_HEADER]);
    buf[10..12].copy_from_slice(&c.to_be_bytes());
}

/// A parsed UDP datagram.
#[derive(Debug, Clone, Copy)]
pub struct Udp<'a> {
    pub src_port: u16,
    pub dst_port: u16,
    pub payload: &'a [u8],
}

pub fn parse_udp(p: &[u8], src: Ip4, dst: Ip4) -> Option<Udp<'_>> {
    if p.len() < 8 {
        return None;
    }
    let len = usize::from(be16(p, 4));
    if len < 8 || len > p.len() {
        return None;
    }
    let sum = be16(p, 6);
    if sum != 0
        && checksum_finish(checksum_add(
            pseudo_sum(src, dst, PROTO_UDP, len),
            &p[..len],
        )) != 0
    {
        return None;
    }
    Some(Udp {
        src_port: be16(p, 0),
        dst_port: be16(p, 2),
        payload: &p[8..len],
    })
}

/// Write a UDP header in front of a payload already at `buf[8..8 + payload_len]`, with the checksum.
pub fn emit_udp(
    buf: &mut [u8],
    src: Ip4,
    dst: Ip4,
    src_port: u16,
    dst_port: u16,
    payload_len: usize,
) {
    let len = 8 + payload_len;
    buf[0..2].copy_from_slice(&src_port.to_be_bytes());
    buf[2..4].copy_from_slice(&dst_port.to_be_bytes());
    buf[4..6].copy_from_slice(&(len as u16).to_be_bytes());
    buf[6..8].fill(0);
    let mut c = checksum_finish(checksum_add(
        pseudo_sum(src, dst, PROTO_UDP, len),
        &buf[..len],
    ));
    if c == 0 {
        c = 0xFFFF;
    }
    buf[6..8].copy_from_slice(&c.to_be_bytes());
}

/// A parsed TCP segment.
#[derive(Debug, Clone, Copy)]
pub struct Tcp<'a> {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq: u32,
    pub ack: u32,
    pub flags: u8,
    pub window: u16,
    /// The maximum segment size option, if present.
    pub mss: Option<u16>,
    pub payload: &'a [u8],
}

pub fn parse_tcp(p: &[u8], src: Ip4, dst: Ip4) -> Option<Tcp<'_>> {
    if p.len() < 20 {
        return None;
    }
    let off = usize::from(p[12] >> 4) * 4;
    if off < 20 || off > p.len() {
        return None;
    }
    if checksum_finish(checksum_add(pseudo_sum(src, dst, PROTO_TCP, p.len()), p)) != 0 {
        return None;
    }
    // Options: only the maximum segment size matters.
    let mut mss = None;
    let mut o = &p[20..off];
    while let Some(&kind) = o.first() {
        match kind {
            0 => break,
            1 => o = &o[1..],
            _ => {
                let len = usize::from(*o.get(1)?);
                if len < 2 || len > o.len() {
                    return None;
                }
                if kind == 2 && len == 4 {
                    mss = Some(be16(o, 2));
                }
                o = &o[len..];
            }
        }
    }
    Some(Tcp {
        src_port: be16(p, 0),
        dst_port: be16(p, 2),
        seq: be32(p, 4),
        ack: be32(p, 8),
        flags: p[13],
        window: be16(p, 14),
        mss,
        payload: &p[off..],
    })
}

/// What to write into a TCP header.
#[derive(Debug, Clone, Copy)]
pub struct TcpOut {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq: u32,
    pub ack: u32,
    pub flags: u8,
    pub window: u16,
    /// Send a maximum segment size option (SYN segments).
    pub mss: Option<u16>,
}

/// The header length for `h`.
pub fn tcp_header_len(h: &TcpOut) -> usize {
    if h.mss.is_some() { 24 } else { 20 }
}

/// Write a TCP header in front of a payload already at `buf[header_len..header_len + payload_len]`, with the
/// checksum; returns the segment length.
pub fn emit_tcp(buf: &mut [u8], src: Ip4, dst: Ip4, h: &TcpOut, payload_len: usize) -> usize {
    let hl = tcp_header_len(h);
    buf[0..2].copy_from_slice(&h.src_port.to_be_bytes());
    buf[2..4].copy_from_slice(&h.dst_port.to_be_bytes());
    buf[4..8].copy_from_slice(&h.seq.to_be_bytes());
    buf[8..12].copy_from_slice(&h.ack.to_be_bytes());
    buf[12] = ((hl / 4) as u8) << 4;
    buf[13] = h.flags;
    buf[14..16].copy_from_slice(&h.window.to_be_bytes());
    buf[16..20].fill(0);
    if let Some(m) = h.mss {
        buf[20] = 2;
        buf[21] = 4;
        buf[22..24].copy_from_slice(&m.to_be_bytes());
    }
    let len = hl + payload_len;
    let c = checksum_finish(checksum_add(
        pseudo_sum(src, dst, PROTO_TCP, len),
        &buf[..len],
    ));
    buf[16..18].copy_from_slice(&c.to_be_bytes());
    len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_of_a_known_ip_header() {
        // The example header from RFC 1071-style references: 45 00 00 73 00 00 40 00 40 11 b8 61 c0 a8 00 01 c0 a8 00 c7.
        let h = [
            0x45, 0, 0, 0x73, 0, 0, 0x40, 0, 0x40, 0x11, 0xb8, 0x61, 0xc0, 0xa8, 0, 1, 0xc0, 0xa8,
            0, 0xc7,
        ];
        assert_eq!(checksum(&h), 0);
        let mut z = h;
        z[10] = 0;
        z[11] = 0;
        assert_eq!(checksum(&z), 0xb861);
        // An odd length is padded with a zero byte.
        assert_eq!(checksum(&[0x01]), !0x0100);
    }

    #[test]
    fn ipv4_round_trip_and_rejections() {
        let mut p = [0u8; 40];
        emit_ipv4(&mut p, [10, 0, 0, 1], [10, 0, 0, 2], PROTO_UDP, 20, 7);
        p[20..40].fill(0xAB);
        let ip = parse_ipv4(&p).unwrap();
        assert_eq!(
            (ip.src, ip.dst, ip.proto, ip.payload.len()),
            ([10, 0, 0, 1], [10, 0, 0, 2], PROTO_UDP, 20)
        );
        // Corrupt checksum, a fragment, a truncated packet.
        let mut bad = p;
        bad[11] ^= 1;
        assert!(parse_ipv4(&bad).is_none());
        let mut frag = p;
        frag[6] = 0x20;
        frag[10] = 0;
        frag[11] = 0;
        let c = checksum(&frag[..20]);
        frag[10..12].copy_from_slice(&c.to_be_bytes());
        assert!(parse_ipv4(&frag).is_none());
        assert!(parse_ipv4(&p[..30]).is_none());
    }

    #[test]
    fn udp_and_tcp_round_trip() {
        let (a, b) = ([192, 168, 1, 5], [192, 168, 1, 9]);
        let mut u = [0u8; 8 + 5];
        u[8..].copy_from_slice(b"hello");
        emit_udp(&mut u, a, b, 1234, 53, 5);
        let p = parse_udp(&u, a, b).unwrap();
        assert_eq!(
            (p.src_port, p.dst_port, p.payload),
            (1234, 53, &b"hello"[..])
        );
        u[9] ^= 0xFF;
        assert!(parse_udp(&u, a, b).is_none());

        let h = TcpOut {
            src_port: 80,
            dst_port: 5000,
            seq: 0xFFFF_FFF0,
            ack: 17,
            flags: TCP_SYN | TCP_ACK,
            window: 4096,
            mss: Some(1460),
        };
        let mut t = [0u8; 24 + 3];
        t[24..].copy_from_slice(b"abc");
        assert_eq!(emit_tcp(&mut t, a, b, &h, 3), 27);
        let p = parse_tcp(&t, a, b).unwrap();
        assert_eq!(
            (
                p.src_port, p.dst_port, p.seq, p.ack, p.flags, p.window, p.mss, p.payload
            ),
            (
                80,
                5000,
                0xFFFF_FFF0,
                17,
                TCP_SYN | TCP_ACK,
                4096,
                Some(1460),
                &b"abc"[..]
            )
        );
        // A wrong address in the pseudo-header fails the checksum.
        assert!(parse_tcp(&t, a, [192, 168, 1, 10]).is_none());
    }

    #[test]
    fn arp_round_trip() {
        let a = Arp {
            op: ARP_REQUEST,
            sha: [2, 0, 0, 0, 0, 1],
            spa: [10, 0, 0, 1],
            tha: [0; 6],
            tpa: [10, 0, 0, 2],
        };
        let mut b = [0u8; 28];
        assert_eq!(emit_arp(&mut b, &a), 28);
        assert_eq!(parse_arp(&b), Some(a));
        assert!(parse_arp(&b[..20]).is_none());
    }
}

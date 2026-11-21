//! The stack against itself and against hand-built packets, over a virtual wire.

use hardroxide_net::http::{Server, parse};
use hardroxide_net::tcp::State;
use hardroxide_net::wire::*;
use hardroxide_net::{Config, Device, Stack, StaticConfig};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

type Queue = Rc<RefCell<VecDeque<Vec<u8>>>>;

/// One end of a wire. `drop_every`: lose every n-th frame sent (0 = none).
struct Pipe {
    mac: Mac,
    rx: Queue,
    tx: Queue,
    sent: u32,
    drop_every: u32,
}

impl Device for Pipe {
    fn mac(&self) -> Mac {
        self.mac
    }
    fn receive(&mut self, buf: &mut [u8]) -> Option<usize> {
        let f = self.rx.borrow_mut().pop_front()?;
        buf[..f.len()].copy_from_slice(&f);
        Some(f.len())
    }
    fn transmit(&mut self, frame: &[u8]) -> bool {
        self.sent += 1;
        if self.drop_every != 0 && self.sent.is_multiple_of(self.drop_every) {
            return true; // lost on the wire
        }
        self.tx.borrow_mut().push_back(frame.to_vec());
        true
    }
}

fn wire(drop_every: u32) -> (Pipe, Pipe) {
    let (q1, q2): (Queue, Queue) = Default::default();
    (
        Pipe {
            mac: [2, 0, 0, 0, 0, 1],
            rx: q1.clone(),
            tx: q2.clone(),
            sent: 0,
            drop_every,
        },
        Pipe {
            mac: [2, 0, 0, 0, 0, 2],
            rx: q2,
            tx: q1,
            sent: 0,
            drop_every,
        },
    )
}

const A: Ip4 = [10, 0, 0, 1];
const B: Ip4 = [10, 0, 0, 2];

fn cfg(ip: Ip4) -> Config {
    Config::Static(StaticConfig {
        ip,
        netmask: [255, 255, 255, 0],
        gateway: None,
    })
}

type S = Stack<Pipe, 4, 2, 2048>;

trait Snapshot {
    fn device_queue_snapshot(&mut self) -> Vec<Vec<u8>>;
}

impl Snapshot for S {
    /// The frames waiting to be received by this stack's device (copies; they stay queued).
    fn device_queue_snapshot(&mut self) -> Vec<Vec<u8>> {
        self.device().rx.borrow().iter().cloned().collect()
    }
}

fn pair(drop_every: u32) -> (S, S) {
    let (pa, pb) = wire(drop_every);
    (Stack::new(pa, cfg(A), 0), Stack::new(pb, cfg(B), 0))
}

fn byte(i: usize) -> u8 {
    (i.wrapping_mul(2_654_435_761) >> 7) as u8
}

/// Send `total` bytes from A (socket 0) to B (socket 0, listening on 7) and check them; both sides close. Returns the time taken (ms).
fn transfer(drop_every: u32, total: usize) -> u64 {
    let (mut a, mut b) = pair(drop_every);
    b.tcp(0).listen(7);
    a.tcp_connect(0, 40000, B, 7);
    let (mut sent, mut got) = (0usize, 0usize);
    let mut t = 0u64;
    let mut closed = false;
    while t < 120_000 {
        a.poll(t);
        b.poll(t);
        // A writes as much as it can.
        while sent < total && a.tcp(0).send_space() > 0 {
            let n = (total - sent).min(700);
            let chunk: Vec<u8> = (sent..sent + n).map(byte).collect();
            let took = a.tcp(0).send(&chunk);
            sent += took;
            if took < n {
                break;
            }
        }
        if sent == total && !closed && a.tcp(0).is_established() && a.tcp(0).all_sent_acked() {
            a.tcp(0).close();
            closed = true;
        }
        // B reads and checks.
        let mut buf = [0u8; 600];
        loop {
            let n = b.tcp(0).recv(&mut buf);
            if n == 0 {
                break;
            }
            for (i, v) in buf[..n].iter().enumerate() {
                assert_eq!(*v, byte(got + i), "byte {} differs", got + i);
            }
            got += n;
        }
        if got == total && b.tcp(0).peer_closed() {
            b.tcp(0).close();
        }
        if a.tcp(0).state() == State::Closed && b.tcp(0).state() == State::Closed {
            break;
        }
        t += 1;
    }
    assert_eq!(
        got,
        total,
        "received {got} of {total} (A {:?}, B {:?}, drop_every {drop_every})",
        a.tcp(0).state(),
        b.tcp(0).state()
    );
    assert!(
        matches!(a.tcp(0).state(), State::Closed | State::TimeWait),
        "A ended in {:?}",
        a.tcp(0).state()
    );
    assert!(
        matches!(b.tcp(0).state(), State::Closed | State::TimeWait),
        "B ended in {:?}",
        b.tcp(0).state()
    );
    t
}

#[test]
fn a_clean_transfer_arrives_intact_and_both_sides_close() {
    let ms = transfer(0, 200_000);
    assert!(ms < 5000, "took {ms} ms of virtual time");
}

#[test]
fn transfers_survive_a_lossy_wire() {
    for drop_every in [11, 5, 3] {
        transfer(drop_every, 30_000);
    }
}

#[test]
fn small_and_empty_transfers_work() {
    for n in [0, 1, 2047, 2048, 2049] {
        transfer(0, n);
    }
}

#[test]
fn ping_is_answered_and_the_checksum_is_right() {
    let (mut a, mut b) = pair(0);
    // A hand-made echo request from A to B (the test is the "host").
    let mut f = vec![0u8; 14 + 20 + 8 + 4];
    let icmp = 34;
    f[icmp] = 8;
    f[icmp + 4..icmp + 6].copy_from_slice(&0x1234u16.to_be_bytes());
    f[icmp + 6..icmp + 8].copy_from_slice(&1u16.to_be_bytes());
    f[icmp + 8..].copy_from_slice(b"ping");
    let c = checksum(&f[icmp..]);
    f[icmp + 2..icmp + 4].copy_from_slice(&c.to_be_bytes());
    emit_ipv4(&mut f[14..], A, B, PROTO_ICMP, 12, 9);
    emit_eth(
        &mut f,
        [2, 0, 0, 0, 0, 2],
        [2, 0, 0, 0, 0, 1],
        ETHERTYPE_IPV4,
    );
    a.device().transmit(&f);
    b.poll(0);
    let mut buf = [0u8; 200];
    let n = a.device().receive(&mut buf).expect("a reply");
    let eth = parse_eth(&buf[..n]).unwrap();
    let ip = parse_ipv4(eth.payload).unwrap();
    assert_eq!((ip.src, ip.dst, ip.proto), (B, A, PROTO_ICMP));
    assert_eq!(ip.payload[0], 0, "an echo reply");
    assert_eq!(checksum(ip.payload), 0);
    assert_eq!(&ip.payload[8..], b"ping");
    assert_eq!(b.stats().icmp_echo_replied, 1);
}

#[test]
fn arp_requests_for_us_are_answered_and_others_ignored() {
    let (mut a, mut b) = pair(0);
    let ask = |target: Ip4| {
        let mut f = vec![0u8; 14 + 28];
        emit_eth(&mut f, BROADCAST_MAC, [2, 0, 0, 0, 0, 1], ETHERTYPE_ARP);
        emit_arp(
            &mut f[14..],
            &Arp {
                op: ARP_REQUEST,
                sha: [2, 0, 0, 0, 0, 1],
                spa: A,
                tha: [0; 6],
                tpa: target,
            },
        );
        f
    };
    a.device().transmit(&ask([10, 0, 0, 77]));
    b.poll(0);
    let mut buf = [0u8; 100];
    assert!(
        a.device().receive(&mut buf).is_none(),
        "not our address: no answer"
    );
    a.device().transmit(&ask(B));
    b.poll(1);
    let n = a.device().receive(&mut buf).unwrap();
    let eth = parse_eth(&buf[..n]).unwrap();
    let arp = parse_arp(eth.payload).unwrap();
    assert_eq!(
        (arp.op, arp.spa, arp.sha),
        (ARP_REPLY, B, [2, 0, 0, 0, 0, 2])
    );
}

#[test]
fn a_closed_port_is_refused_with_a_reset() {
    let (mut a, mut b) = pair(0);
    a.tcp_connect(0, 40001, B, 9999); // nobody listens
    for t in 0..50 {
        a.poll(t);
        b.poll(t);
    }
    assert_eq!(a.tcp(0).state(), State::Closed);
    assert!(a.tcp(0).was_reset());
    assert_eq!(b.stats().tcp_resets_sent, 1);
}

fn picture(arg: u32, offset: usize, out: &mut [u8]) -> usize {
    for (i, o) in out.iter_mut().enumerate() {
        *o = (offset + i) as u8 ^ arg as u8;
    }
    out.len()
}

#[test]
fn the_web_server_serves_pages_forms_and_a_large_generated_body() {
    let (pa, pb) = wire(0);
    let mut client: S = Stack::new(pa, cfg(A), 0);
    let mut board: S = Stack::new(pb, cfg(B), 0);
    let mut server: Server<2, 512, 256> = Server::new(80, 0);
    let mut led = 0u32;
    let get = |client: &mut S,
               board: &mut S,
               server: &mut Server<2, 512, 256>,
               led: &mut u32,
               request: &[u8],
               t0: u64|
     -> (Vec<u8>, u64) {
        client.tcp_connect(3, 50000 + (t0 as u16 % 1000), B, 80);
        let mut sent = false;
        let mut resp = Vec::new();
        let mut t = t0;
        while t < t0 + 20_000 {
            client.poll(t);
            board.poll(t);
            server.poll(board, |req, res| match (req.method, req.path) {
                ("GET", "/") => res.text(
                    200,
                    "text/html",
                    format_args!("<h1>board</h1> led={}", *led),
                ),
                ("POST" | "GET", "/led") => {
                    *led = req.param("v").and_then(|v| v.parse().ok()).unwrap_or(0);
                    res.text(200, "text/plain", format_args!("led set to {}\n", *led));
                }
                ("GET", "/picture.bin") => {
                    res.generated(200, "application/octet-stream", 30_000, picture, 0x5A)
                }
                _ => res.text(
                    404,
                    "text/plain",
                    format_args!("no such page: {}\n", req.path),
                ),
            });
            if !sent && client.tcp(3).is_established() {
                assert_eq!(client.tcp(3).send(request), request.len());
                sent = true;
            }
            let mut buf = [0u8; 600];
            loop {
                let n = client.tcp(3).recv(&mut buf);
                if n == 0 {
                    break;
                }
                resp.extend_from_slice(&buf[..n]);
            }
            if client.tcp(3).peer_closed() {
                client.tcp(3).close();
            }
            if sent && client.tcp(3).state() == State::Closed {
                break;
            }
            t += 1;
        }
        (resp, t)
    };

    let (r, t) = get(
        &mut client,
        &mut board,
        &mut server,
        &mut led,
        b"GET / HTTP/1.1\r\nHost: x\r\n\r\n",
        0,
    );
    let text = String::from_utf8_lossy(&r);
    assert!(text.starts_with("HTTP/1.1 200 OK\r\n"), "{text}");
    assert!(
        text.contains("Content-Length: 20\r\n") && text.ends_with("<h1>board</h1> led=0"),
        "{text}"
    );

    let (r, t) = get(&mut client, &mut board, &mut server, &mut led, b"POST /led HTTP/1.1\r\nContent-Length: 4\r\nContent-Type: application/x-www-form-urlencoded\r\n\r\nv=42", t + 10);
    assert!(String::from_utf8_lossy(&r).ends_with("led set to 42\n"));
    assert_eq!(led, 42);

    let (r, t) = get(
        &mut client,
        &mut board,
        &mut server,
        &mut led,
        b"GET /nope HTTP/1.1\r\n\r\n",
        t + 10,
    );
    assert!(String::from_utf8_lossy(&r).starts_with("HTTP/1.1 404 Not Found"));

    let (r, _) = get(
        &mut client,
        &mut board,
        &mut server,
        &mut led,
        b"GET /picture.bin HTTP/1.1\r\n\r\n",
        t + 10,
    );
    let end = r.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let head = String::from_utf8_lossy(&r[..end]);
    assert!(head.contains("Content-Length: 30000"), "{head}");
    assert_eq!(r.len() - end, 30_000);
    assert!(
        r[end..]
            .iter()
            .enumerate()
            .all(|(i, b)| *b == (i as u8 ^ 0x5A)),
        "the generated body differs"
    );
    assert_eq!(server.served, 4);
}

#[test]
fn a_garbled_request_gets_400_and_an_oversized_one_413() {
    let (pa, pb) = wire(0);
    let mut client: S = Stack::new(pa, cfg(A), 0);
    let mut board: S = Stack::new(pb, cfg(B), 0);
    let mut server: Server<1, 128, 64> = Server::new(80, 0);
    for (request, want) in [(&b"NONSENSE\r\n\r\n"[..], "400"), (&[b'a'; 400][..], "413")] {
        client.tcp_connect(0, 51000, B, 80);
        let mut sent = false;
        let mut resp = Vec::new();
        for t in 0..5000u64 {
            client.poll(t);
            board.poll(t);
            server.poll(&mut board, |_, res| res.empty(204));
            if !sent && client.tcp(0).is_established() {
                client.tcp(0).send(request);
                sent = true;
            }
            let mut buf = [0u8; 300];
            let n = client.tcp(0).recv(&mut buf);
            resp.extend_from_slice(&buf[..n]);
            if client.tcp(0).peer_closed() {
                client.tcp(0).close();
            }
            if sent && client.tcp(0).state() == State::Closed {
                break;
            }
        }
        assert!(
            String::from_utf8_lossy(&resp).starts_with(&format!("HTTP/1.1 {want}")),
            "{}",
            String::from_utf8_lossy(&resp)
        );
        // Let the board's socket finish before the next round.
        for t in 5000..9000u64 {
            client.poll(t);
            board.poll(t);
            server.poll(&mut board, |_, res| res.empty(204));
        }
    }
}

#[test]
fn dhcp_takes_an_offered_address_and_falls_back_without_a_server() {
    // With a (hand-made) server.
    let (pa, pb) = wire(0);
    let mut server_side = pb; // the test plays the server on this end
    let mut c: S = Stack::new(
        pa,
        Config::Dhcp {
            fallback: Some(StaticConfig {
                ip: [192, 168, 2, 100],
                netmask: [255, 255, 255, 0],
                gateway: None,
            }),
            fallback_after_ms: 6000,
        },
        0,
    );
    let server_mac = [2, 0, 0, 0, 0, 2];
    let mut buf = [0u8; 600];
    let mut state = 0;
    let mut xid = [0u8; 4];
    for t in 0..3000u64 {
        c.poll(t);
        while let Some(n) = server_side.receive(&mut buf) {
            let eth = parse_eth(&buf[..n]).unwrap();
            let Some(ip) = parse_ipv4(eth.payload) else {
                continue;
            };
            let udp = parse_udp(ip.payload, ip.src, ip.dst).unwrap();
            if udp.dst_port != 67 {
                continue;
            }
            let msg_type = udp.payload[242];
            xid.copy_from_slice(&udp.payload[4..8]);
            let reply_type = match (state, msg_type) {
                (0, 1) => 2, // DISCOVER -> OFFER
                (1, 3) => 5, // REQUEST -> ACK
                _ => continue,
            };
            state += 1;
            let mut d = vec![0u8; 14 + 20 + 8 + 300];
            let p = 42;
            d[p] = 2;
            d[p + 4..p + 8].copy_from_slice(&xid);
            d[p + 16..p + 20].copy_from_slice(&[10, 0, 2, 15]);
            d[p + 28..p + 34].copy_from_slice(&[2, 0, 0, 0, 0, 1]);
            d[p + 236..p + 240].copy_from_slice(&[99, 130, 83, 99]);
            let opts = [
                53, 1, reply_type, 1, 4, 255, 255, 255, 0, 3, 4, 10, 0, 2, 2, 54, 4, 10, 0, 2, 2,
                255,
            ];
            d[p + 240..p + 240 + opts.len()].copy_from_slice(&opts);
            let n = 240 + opts.len();
            emit_udp(&mut d[34..], [10, 0, 2, 2], [255; 4], 67, 68, n);
            emit_ipv4(&mut d[14..], [10, 0, 2, 2], [255; 4], PROTO_UDP, 8 + n, 3);
            emit_eth(&mut d, BROADCAST_MAC, server_mac, ETHERTYPE_IPV4);
            d.truncate(14 + 20 + 8 + n);
            server_side.transmit(&d);
        }
        if c.dhcp_bound() {
            break;
        }
    }
    assert!(c.dhcp_bound());
    assert_eq!(
        (c.ip(), c.netmask(), c.gateway()),
        ([10, 0, 2, 15], [255, 255, 255, 0], Some([10, 0, 2, 2]))
    );

    // With no server at all: the fallback after its timeout.
    let (pa, _pb) = wire(0);
    let mut c: S = Stack::new(
        pa,
        Config::Dhcp {
            fallback: Some(StaticConfig {
                ip: [192, 168, 2, 100],
                netmask: [255, 255, 255, 0],
                gateway: None,
            }),
            fallback_after_ms: 6000,
        },
        0,
    );
    for t in 0..5900u64 {
        c.poll(t);
    }
    assert!(!c.is_configured(), "not yet");
    for t in 5900..6100u64 {
        c.poll(t);
    }
    assert_eq!(c.ip(), [192, 168, 2, 100]);
    assert!(!c.dhcp_bound());
}

#[test]
fn http_parse_is_reachable_from_outside() {
    assert!(parse(b"GET / HTTP/1.1\r\n\r\n").is_ok());
}

/// A device that "receives" whatever the test hands it.
struct Feed {
    frames: VecDeque<Vec<u8>>,
}

impl Device for Feed {
    fn mac(&self) -> Mac {
        [2, 0, 0, 0, 0, 2]
    }
    fn receive(&mut self, buf: &mut [u8]) -> Option<usize> {
        let f = self.frames.pop_front()?;
        let n = f.len().min(buf.len());
        buf[..n].copy_from_slice(&f[..n]);
        Some(n)
    }
    fn transmit(&mut self, _: &[u8]) -> bool {
        true
    }
}

#[test]
fn garbage_and_corrupted_frames_never_panic() {
    // Capture real traffic (a transfer, an HTTP request, a DHCP exchange would do; a transfer has TCP, ARP and ICMP-free IP).
    let (pa, pb) = wire(0);
    let mut a: S = Stack::new(pa, cfg(A), 0);
    let mut b: S = Stack::new(pb, cfg(B), 0);
    b.tcp(0).listen(7);
    a.tcp_connect(0, 40000, B, 7);
    let mut captured: Vec<Vec<u8>> = Vec::new();
    for t in 0..300u64 {
        a.poll(t);
        // Peek at what B is about to receive.
        captured.extend(b.device_queue_snapshot());
        b.poll(t);
        if a.tcp(0).is_established() && a.tcp(0).send_space() > 0 {
            a.tcp(0).send(&[7u8; 1000]);
        }
    }
    assert!(captured.len() > 5);
    // A simple xorshift for repeatable "randomness".
    let mut x = 0x1234_5678u32;
    let mut rnd = move || {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x
    };
    let mut frames = VecDeque::new();
    for i in 0..30_000 {
        let mut f = if i % 3 == 0 {
            // Pure noise, of any length including tiny.
            (0..(rnd() % 200) as usize)
                .map(|_| rnd() as u8)
                .collect::<Vec<u8>>()
        } else {
            let mut f = captured[rnd() as usize % captured.len()].clone();
            for _ in 0..1 + rnd() % 4 {
                let at = rnd() as usize % f.len();
                f[at] = rnd() as u8;
            }
            if rnd() % 5 == 0 {
                let keep = rnd() as usize % f.len();
                f.truncate(keep);
            }
            f
        };
        // Often make it addressed to the board so it reaches the parsers.
        if f.len() >= 6 && rnd() % 2 == 0 {
            f[..6].copy_from_slice(&[2, 0, 0, 0, 0, 2]);
        }
        frames.push_back(f);
    }
    let mut victim: Stack<Feed, 4, 2, 2048> = Stack::new(Feed { frames }, cfg(B), 0);
    victim.tcp(0).listen(7);
    victim.tcp(1).listen(80);
    for t in 0..2000u64 {
        victim.poll(t);
    }
    assert!(victim.stats().rx_frames >= 30_000);
}

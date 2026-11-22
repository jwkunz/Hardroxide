//! TCP: one socket is one connection (or one listener waiting for one). A pure state machine: segments in, segments out.
//!
//! What it does: the three-way handshake (passive and active), in-order delivery, cumulative acknowledgement, a send window
//! from the peer's advertised window, retransmission with a doubling timeout (go-back-N), the close handshake with
//! `TIME-WAIT`, resets, the maximum segment size option. What it does not: out-of-order buffering (a segment past a gap is dropped and the
//! gap acknowledged again, so the peer retransmits), congestion control (the window is the peer's), window scaling, selective acknowledgement,
//! urgent data, delayed acknowledgements and keep-alives.

use crate::wire::{Ip4, TCP_ACK, TCP_FIN, TCP_PSH, TCP_RST, TCP_SYN, Tcp};

/// The largest segment payload we send or accept (1500 byte frames less headers).
pub const MSS: u16 = 1460;
const INITIAL_RTO_MS: u64 = 400;
const MAX_RTO_MS: u64 = 4000;
const MAX_RETRIES: u8 = 8;
const TIME_WAIT_MS: u64 = 2000;

fn seq_lt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}
fn seq_le(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) <= 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
}

/// A byte ring buffer of fixed capacity.
struct Ring<const N: usize> {
    buf: [u8; N],
    head: usize,
    len: usize,
}

impl<const N: usize> Ring<N> {
    const fn new() -> Self {
        Ring {
            buf: [0; N],
            head: 0,
            len: 0,
        }
    }
    fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }
    fn free(&self) -> usize {
        N - self.len
    }
    fn push(&mut self, data: &[u8]) -> usize {
        let n = data.len().min(self.free());
        for (i, b) in data[..n].iter().enumerate() {
            self.buf[(self.head + self.len + i) % N] = *b;
        }
        self.len += n;
        n
    }
    /// Copy up to `out.len()` bytes starting `offset` bytes in, without removing them.
    fn copy_at(&self, offset: usize, out: &mut [u8]) -> usize {
        let n = out.len().min(self.len.saturating_sub(offset));
        for (i, o) in out[..n].iter_mut().enumerate() {
            *o = self.buf[(self.head + offset + i) % N];
        }
        n
    }
    fn drop_front(&mut self, n: usize) {
        let n = n.min(self.len);
        self.head = (self.head + n) % N;
        self.len -= n;
    }
}

/// What a socket wants sent next.
#[derive(Debug, Clone, Copy)]
pub struct SegOut {
    pub flags: u8,
    pub seq: u32,
    pub ack: u32,
    pub window: u16,
    pub mss: Option<u16>,
    pub payload_len: usize,
}

pub struct TcpSocket<const N: usize> {
    state: State,
    local_port: u16,
    remote_ip: Ip4,
    remote_port: u16,
    rx: Ring<N>,
    tx: Ring<N>,
    // Receive side.
    rcv_nxt: u32,
    peer_fin: bool,
    ack_needed: bool,
    // Send side. `snd_una` is the oldest unacknowledged sequence number; the first `tx_sent` bytes of `tx` are sent and
    // unacknowledged, and `fin_sent` means the FIN follows them.
    iss: u32,
    snd_una: u32,
    snd_wnd: u32,
    tx_sent: usize,
    syn_sent: bool,
    fin_queued: bool,
    fin_sent: bool,
    fin_acked: bool,
    peer_mss: u16,
    // Timers.
    rto: u64,
    retries: u8,
    timer: Option<u64>,
    time_wait_until: u64,
    rst_pending: bool,
    reset_by_peer: bool,
}

impl<const N: usize> Default for TcpSocket<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> TcpSocket<N> {
    pub const fn new() -> Self {
        TcpSocket {
            state: State::Closed,
            local_port: 0,
            remote_ip: [0; 4],
            remote_port: 0,
            rx: Ring::new(),
            tx: Ring::new(),
            rcv_nxt: 0,
            peer_fin: false,
            ack_needed: false,
            iss: 0,
            snd_una: 0,
            snd_wnd: 0,
            tx_sent: 0,
            syn_sent: false,
            fin_queued: false,
            fin_sent: false,
            fin_acked: false,
            peer_mss: 536,
            rto: INITIAL_RTO_MS,
            retries: 0,
            timer: None,
            time_wait_until: 0,
            rst_pending: false,
            reset_by_peer: false,
        }
    }

    fn reset_connection(&mut self) {
        self.rx.clear();
        self.tx.clear();
        self.peer_fin = false;
        self.ack_needed = false;
        self.tx_sent = 0;
        self.syn_sent = false;
        self.fin_queued = false;
        self.fin_sent = false;
        self.fin_acked = false;
        self.rto = INITIAL_RTO_MS;
        self.retries = 0;
        self.timer = None;
        self.rst_pending = false;
        self.reset_by_peer = false;
        self.peer_mss = 536;
    }

    // ---- The application's side ----

    /// Wait for a connection to `port`. Resets the socket.
    pub fn listen(&mut self, port: u16) {
        self.reset_connection();
        self.local_port = port;
        self.remote_ip = [0; 4];
        self.remote_port = 0;
        self.state = State::Listen;
    }

    /// Open a connection to `remote_ip:remote_port` from `local_port`. `iss` is the initial sequence number.
    pub fn connect(&mut self, local_port: u16, remote_ip: Ip4, remote_port: u16, iss: u32) {
        self.reset_connection();
        self.local_port = local_port;
        self.remote_ip = remote_ip;
        self.remote_port = remote_port;
        self.iss = iss;
        self.snd_una = iss;
        self.state = State::SynSent;
    }

    pub fn state(&self) -> State {
        self.state
    }
    pub fn local_port(&self) -> u16 {
        self.local_port
    }
    pub fn remote(&self) -> (Ip4, u16) {
        (self.remote_ip, self.remote_port)
    }
    /// The peer reset the connection (or retransmissions gave up).
    pub fn was_reset(&self) -> bool {
        self.reset_by_peer
    }
    /// Bytes waiting to be read.
    pub fn recv_len(&self) -> usize {
        self.rx.len
    }
    /// The peer has closed its side and everything it sent has been read.
    pub fn peer_closed(&self) -> bool {
        self.peer_fin && self.rx.len == 0
    }
    /// Room for more outgoing bytes.
    pub fn send_space(&self) -> usize {
        match self.state {
            State::Established | State::CloseWait if !self.fin_queued => self.tx.free(),
            _ => 0,
        }
    }
    /// The connection is up and may carry data.
    pub fn is_established(&self) -> bool {
        matches!(self.state, State::Established | State::CloseWait)
    }
    /// The socket is idle and may be given a new job.
    pub fn is_closed(&self) -> bool {
        self.state == State::Closed
    }
    /// Everything we sent has been acknowledged.
    pub fn all_sent_acked(&self) -> bool {
        self.tx.len == 0
    }

    /// Read up to `buf.len()` bytes; returns how many (0 if none are waiting).
    pub fn recv(&mut self, buf: &mut [u8]) -> usize {
        let n = self.rx.copy_at(0, buf);
        let before = self.rx.free();
        self.rx.drop_front(n);
        // Tell the peer when the window has opened a good deal.
        if n > 0 && before < usize::from(MSS) && self.rx.free() >= usize::from(MSS) {
            self.ack_needed = true;
        }
        n
    }

    /// Queue bytes to send; returns how many fitted.
    pub fn send(&mut self, data: &[u8]) -> usize {
        if self.send_space() == 0 {
            return 0;
        }
        self.tx.push(data)
    }

    /// Finish sending and close: the FIN follows the queued data.
    pub fn close(&mut self) {
        match self.state {
            State::Listen | State::SynSent | State::Closed => self.state = State::Closed,
            State::Established | State::CloseWait | State::SynReceived => self.fin_queued = true,
            _ => {}
        }
    }

    /// Drop the connection at once (a reset is sent if the peer knows of it).
    pub fn abort(&mut self) {
        match self.state {
            State::Established
            | State::CloseWait
            | State::FinWait1
            | State::FinWait2
            | State::SynReceived
            | State::Closing
            | State::LastAck => self.rst_pending = true,
            _ => self.state = State::Closed,
        }
    }

    // ---- The network's side ----

    fn snd_nxt(&self) -> u32 {
        self.snd_una
            .wrapping_add(self.tx_sent as u32)
            .wrapping_add(u32::from(self.fin_sent))
    }

    fn window(&self) -> u16 {
        self.rx.free().min(65535) as u16
    }

    /// Whether this socket takes a segment from `src_ip:t.src_port` for `t.dst_port`.
    pub(crate) fn matches(&self, src_ip: Ip4, t: &Tcp) -> bool {
        match self.state {
            State::Closed => false,
            State::Listen => {
                self.local_port == t.dst_port
                    && t.flags & TCP_SYN != 0
                    && t.flags & (TCP_ACK | TCP_RST) == 0
            }
            _ => {
                self.local_port == t.dst_port
                    && self.remote_port == t.src_port
                    && self.remote_ip == src_ip
            }
        }
    }

    /// The reason for a reset in a closed state.
    fn fail(&mut self) {
        self.state = State::Closed;
        self.reset_by_peer = true;
        self.timer = None;
    }

    /// A segment for this socket arrived. `iss` is a fresh initial sequence number, used if this accepts a connection.
    pub(crate) fn on_segment(&mut self, now: u64, src_ip: Ip4, t: &Tcp, iss: u32) {
        match self.state {
            State::Closed => {}
            State::Listen => {
                if t.flags & TCP_SYN != 0 {
                    self.reset_connection();
                    self.remote_ip = src_ip;
                    self.remote_port = t.src_port;
                    self.rcv_nxt = t.seq.wrapping_add(1);
                    self.iss = iss;
                    self.snd_una = iss;
                    self.snd_wnd = u32::from(t.window);
                    self.peer_mss = t.mss.unwrap_or(536).min(MSS);
                    self.state = State::SynReceived;
                }
            }
            State::SynSent => {
                if t.flags & TCP_RST != 0 {
                    if t.flags & TCP_ACK != 0 && t.ack == self.iss.wrapping_add(1) {
                        self.fail();
                    }
                } else if t.flags & (TCP_SYN | TCP_ACK) == (TCP_SYN | TCP_ACK)
                    && t.ack == self.iss.wrapping_add(1)
                {
                    self.rcv_nxt = t.seq.wrapping_add(1);
                    self.snd_una = t.ack;
                    self.snd_wnd = u32::from(t.window);
                    self.peer_mss = t.mss.unwrap_or(536).min(MSS);
                    self.state = State::Established;
                    self.ack_needed = true;
                    self.timer = None;
                    self.retries = 0;
                    self.rto = INITIAL_RTO_MS;
                }
            }
            State::SynReceived => {
                if t.flags & TCP_RST != 0 {
                    self.fail();
                } else if t.flags & TCP_SYN != 0 && t.seq.wrapping_add(1) == self.rcv_nxt {
                    // The SYN again: our SYN-ACK was lost.
                    self.syn_sent = false;
                } else if t.flags & TCP_ACK != 0 && t.ack == self.iss.wrapping_add(1) {
                    self.snd_una = t.ack;
                    self.snd_wnd = u32::from(t.window);
                    self.state = State::Established;
                    self.timer = None;
                    self.retries = 0;
                    self.rto = INITIAL_RTO_MS;
                    self.synchronised(now, t);
                }
            }
            State::TimeWait => {
                if t.flags & TCP_FIN != 0 {
                    self.ack_needed = true;
                    self.time_wait_until = now + TIME_WAIT_MS;
                }
            }
            _ => self.synchronised(now, t),
        }
    }

    /// A segment in a state where both sides know the sequence numbers.
    fn synchronised(&mut self, now: u64, t: &Tcp) {
        if t.flags & TCP_RST != 0 {
            // Accept a reset whose sequence number is inside the window.
            let win = self.rx.free() as u32;
            if seq_le(self.rcv_nxt, t.seq) && seq_lt(t.seq, self.rcv_nxt.wrapping_add(win.max(1))) {
                self.fail();
            }
            return;
        }
        if t.flags & TCP_SYN != 0 {
            self.ack_needed = true;
            return;
        }
        if t.flags & TCP_ACK == 0 {
            return;
        }
        // Acknowledgement.
        let snd_nxt = self.snd_nxt();
        if seq_lt(self.snd_una, t.ack) && seq_le(t.ack, snd_nxt) {
            let mut acked = t.ack.wrapping_sub(self.snd_una) as usize;
            if self.fin_sent && t.ack == snd_nxt {
                self.fin_acked = true;
                acked -= 1;
            }
            self.tx.drop_front(acked);
            self.tx_sent -= acked;
            self.snd_una = t.ack;
            self.retries = 0;
            self.rto = INITIAL_RTO_MS;
            self.timer = if self.snd_nxt() != self.snd_una {
                Some(now + self.rto)
            } else {
                None
            };
        }
        self.snd_wnd = u32::from(t.window);
        if self.fin_acked {
            match self.state {
                State::FinWait1 => self.state = State::FinWait2,
                State::Closing => self.enter_time_wait(now),
                State::LastAck => {
                    self.state = State::Closed;
                    return;
                }
                _ => {}
            }
        }
        // Data and FIN, in order only.
        if !matches!(
            self.state,
            State::Established | State::FinWait1 | State::FinWait2
        ) {
            return;
        }
        let (mut seq, mut payload, mut fin) = (t.seq, t.payload, t.flags & TCP_FIN != 0);
        let seg_len = payload.len() + usize::from(fin);
        if seg_len == 0 {
            return;
        }
        if seq_lt(seq, self.rcv_nxt) {
            let skip = self.rcv_nxt.wrapping_sub(seq) as usize;
            if skip >= seg_len {
                self.ack_needed = true; // an old duplicate
                return;
            }
            payload = &payload[skip.min(payload.len())..];
            seq = self.rcv_nxt;
        }
        if seq != self.rcv_nxt {
            self.ack_needed = true; // a gap: say what we expect, the peer will retransmit
            return;
        }
        let take = self.rx.push(payload);
        self.rcv_nxt = self.rcv_nxt.wrapping_add(take as u32);
        if take < payload.len() {
            fin = false;
        }
        if fin && !self.peer_fin {
            self.peer_fin = true;
            self.rcv_nxt = self.rcv_nxt.wrapping_add(1);
            match self.state {
                State::Established => self.state = State::CloseWait,
                State::FinWait1 => {
                    if self.fin_acked {
                        self.enter_time_wait(now);
                    } else {
                        self.state = State::Closing;
                    }
                }
                State::FinWait2 => self.enter_time_wait(now),
                _ => {}
            }
        }
        self.ack_needed = true;
    }

    fn enter_time_wait(&mut self, now: u64) {
        self.state = State::TimeWait;
        self.time_wait_until = now + TIME_WAIT_MS;
        self.timer = None;
    }

    /// The next segment this socket wants to send, with any payload copied into `payload`. Call until it returns `None`.
    pub(crate) fn poll_tx(&mut self, now: u64, payload: &mut [u8]) -> Option<SegOut> {
        if self.rst_pending {
            self.rst_pending = false;
            let seg = SegOut {
                flags: TCP_RST | TCP_ACK,
                seq: self.snd_nxt(),
                ack: self.rcv_nxt,
                window: 0,
                mss: None,
                payload_len: 0,
            };
            self.state = State::Closed;
            return Some(seg);
        }
        if self.state == State::TimeWait && now >= self.time_wait_until {
            self.state = State::Closed;
        }
        if matches!(self.state, State::Closed | State::Listen) {
            return None;
        }
        // Retransmission.
        if let Some(deadline) = self.timer
            && now >= deadline
        {
            self.retries += 1;
            if self.retries > MAX_RETRIES {
                self.fail();
                return None;
            }
            self.rto = (self.rto * 2).min(MAX_RTO_MS);
            self.timer = None;
            match self.state {
                State::SynSent | State::SynReceived => self.syn_sent = false,
                _ => {
                    self.tx_sent = 0;
                    self.fin_sent = false;
                }
            }
        }
        match self.state {
            State::SynSent if !self.syn_sent => {
                self.syn_sent = true;
                self.timer = Some(now + self.rto);
                return Some(SegOut {
                    flags: TCP_SYN,
                    seq: self.iss,
                    ack: 0,
                    window: self.window(),
                    mss: Some(MSS),
                    payload_len: 0,
                });
            }
            State::SynReceived if !self.syn_sent => {
                self.syn_sent = true;
                self.timer = Some(now + self.rto);
                return Some(SegOut {
                    flags: TCP_SYN | TCP_ACK,
                    seq: self.iss,
                    ack: self.rcv_nxt,
                    window: self.window(),
                    mss: Some(MSS),
                    payload_len: 0,
                });
            }
            State::SynSent | State::SynReceived | State::TimeWait => {
                if self.ack_needed && self.state == State::TimeWait {
                    self.ack_needed = false;
                    return Some(SegOut {
                        flags: TCP_ACK,
                        seq: self.snd_nxt(),
                        ack: self.rcv_nxt,
                        window: self.window(),
                        mss: None,
                        payload_len: 0,
                    });
                }
                return None;
            }
            _ => {}
        }
        // Data.
        let unsent = self.tx.len - self.tx_sent;
        if unsent > 0 && !self.fin_sent {
            let mut room = self.snd_wnd.saturating_sub(self.tx_sent as u32) as usize;
            if room == 0 && self.tx_sent == 0 {
                room = 1; // a window probe
            }
            let n = unsent
                .min(room)
                .min(usize::from(self.peer_mss))
                .min(payload.len());
            if n > 0 {
                let got = self.tx.copy_at(self.tx_sent, &mut payload[..n]);
                let seq = self.snd_una.wrapping_add(self.tx_sent as u32);
                self.tx_sent += got;
                if self.timer.is_none() {
                    self.timer = Some(now + self.rto);
                }
                self.ack_needed = false;
                let flags = if got == unsent {
                    TCP_ACK | TCP_PSH
                } else {
                    TCP_ACK
                };
                return Some(SegOut {
                    flags,
                    seq,
                    ack: self.rcv_nxt,
                    window: self.window(),
                    mss: None,
                    payload_len: got,
                });
            }
        }
        // Our FIN, after all the data.
        if self.fin_queued && !self.fin_sent && self.tx_sent == self.tx.len {
            let seq = self.snd_una.wrapping_add(self.tx_sent as u32);
            self.fin_sent = true;
            if self.timer.is_none() {
                self.timer = Some(now + self.rto);
            }
            match self.state {
                State::Established | State::SynReceived => self.state = State::FinWait1,
                State::CloseWait => self.state = State::LastAck,
                _ => {}
            }
            self.ack_needed = false;
            return Some(SegOut {
                flags: TCP_FIN | TCP_ACK,
                seq,
                ack: self.rcv_nxt,
                window: self.window(),
                mss: None,
                payload_len: 0,
            });
        }
        if self.ack_needed {
            self.ack_needed = false;
            return Some(SegOut {
                flags: TCP_ACK,
                seq: self.snd_nxt(),
                ack: self.rcv_nxt,
                window: self.window(),
                mss: None,
                payload_len: 0,
            });
        }
        None
    }
}

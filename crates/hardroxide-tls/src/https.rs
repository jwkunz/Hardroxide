//! HTTPS: the HTTP server of `hardroxide-net` over TLS connections on the stack's TCP sockets.

use crate::{Config, TlsServer};
use hardroxide_net::http::{Request, Response, Server, Stream};
use hardroxide_net::tcp::{State, TcpSocket};
use hardroxide_net::{Device, Stack};

/// One TLS connection on one TCP socket, presented as a [`Stream`] to the HTTP server.
pub struct TlsStream<'s, 'c, const N: usize> {
    tls: &'s mut TlsServer<'c>,
    sock: &'s mut TcpSocket<N>,
    /// Fresh randomness for the next connection.
    random: &'s mut dyn FnMut() -> [u8; 64],
}

impl<const N: usize> TlsStream<'_, '_, N> {
    fn tcp_state(&self) -> State {
        self.sock.state()
    }
}

impl<const N: usize> Stream for TlsStream<'_, '_, N> {
    /// Move what has arrived on the socket into the TLS engine and what the engine wants sent onto the socket.
    fn pump(&mut self) {
        if self.sock.is_closed() || self.sock.state() == State::Listen {
            return;
        }
        // Network to TLS (only as much as the engine takes, so nothing is lost).
        loop {
            let want = self.tls.input_space().min(512);
            if want == 0 || self.sock.recv_len() == 0 {
                break;
            }
            let mut buf = [0u8; 512];
            let n = self.sock.recv(&mut buf[..want]);
            if n == 0 {
                break;
            }
            self.tls.feed(&buf[..n]);
        }
        // TLS to network.
        let pending = self.tls.pending_output().len();
        if pending > 0 && self.sock.is_established() {
            let n = self.sock.send(self.tls.pending_output());
            self.tls.consume_output(n);
        }
        // A failed handshake: once the alert is out, close; after our close_notify is out, close the socket (the FIN follows the data).
        let flushed = self.tls.pending_output().is_empty();
        if flushed && (self.tls.failure().is_some() || self.tls.close_sent()) {
            self.sock.close();
        }
    }

    fn is_idle(&self) -> bool {
        matches!(self.tcp_state(), State::Closed | State::TimeWait)
    }

    fn is_waiting(&self) -> bool {
        // Connecting, handshaking, or a failed handshake waiting for the socket to close.
        matches!(
            self.tcp_state(),
            State::Listen | State::SynReceived | State::SynSent
        ) || self.tls.is_handshaking()
            || self.tls.failure().is_some()
    }

    fn was_reset(&self) -> bool {
        self.sock.was_reset()
    }

    fn recv(&mut self, buf: &mut [u8]) -> usize {
        let n = self.tls.recv(buf);
        self.pump();
        n
    }

    fn peer_closed(&self) -> bool {
        (self.tls.peer_closed() || self.sock.peer_closed()) && self.tls.app_len() == 0
    }

    fn send_space(&self) -> usize {
        self.tls.send_space()
    }

    fn send(&mut self, data: &[u8]) -> usize {
        let n = self.tls.send(data);
        self.pump();
        n
    }

    fn close(&mut self) {
        self.tls.close();
        self.pump();
    }

    fn abort(&mut self) {
        self.sock.abort();
    }

    fn listen(&mut self, port: u16) {
        let r = (self.random)();
        self.tls.reset(r);
        self.sock.listen(port);
    }
}

/// An HTTPS server on `CONNS` of the stack's TCP sockets: the HTTP server of `hardroxide-net`, each connection wrapped in TLS.
pub struct HttpsServer<'c, const CONNS: usize, const REQ: usize, const SCRATCH: usize> {
    http: Server<CONNS, REQ, SCRATCH>,
    tls: [TlsServer<'c>; CONNS],
    first_socket: usize,
}

impl<'c, const CONNS: usize, const REQ: usize, const SCRATCH: usize>
    HttpsServer<'c, CONNS, REQ, SCRATCH>
{
    /// A server for `port` using sockets `first_socket..first_socket + CONNS`; `random` yields 64 fresh bytes per call.
    pub fn new(
        port: u16,
        first_socket: usize,
        config: &'c Config<'c>,
        random: &mut dyn FnMut() -> [u8; 64],
    ) -> Self {
        HttpsServer {
            http: Server::new(port, first_socket),
            tls: core::array::from_fn(|_| TlsServer::new(config, random())),
            first_socket,
        }
    }

    /// Requests answered so far.
    pub fn served(&self) -> u32 {
        self.http.served
    }

    /// Advance every connection (see [`Server::poll`]). `random` must give fresh unpredictable bytes (64 per connection).
    pub fn poll<D: Device, const T: usize, const U: usize, const BUF: usize>(
        &mut self,
        stack: &mut Stack<D, T, U, BUF>,
        random: &mut dyn FnMut() -> [u8; 64],
        mut handler: impl FnMut(&Request, &mut Response),
    ) {
        for k in 0..CONNS {
            let mut stream = TlsStream {
                tls: &mut self.tls[k],
                sock: stack.tcp(self.first_socket + k),
                random,
            };
            self.http.poll_stream(k, &mut stream, &mut handler);
        }
    }
}

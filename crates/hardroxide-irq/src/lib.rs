//! Interrupt waiting through a Linux UIO device (`/dev/uioN`).
//!
//! UIO protocol: writing a 32-bit `1` unmasks the interrupt; a blocking
//! 4-byte read returns the cumulative event count once it fires (the kernel
//! masks the line while it is pending). Device-tree/UIO setup per IP and the
//! behaviour on the stock PYNQ image are UNVERIFIED (docs/08).

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum IrqError {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "no UIO device for interrupt `{instance}.{port}` (device-tree node `{node}`, base {base:#x}); \
         is the generated overlay (interrupts.dtso) applied?"
    )]
    NotFound {
        instance: &'static str,
        port: &'static str,
        node: &'static str,
        base: u64,
    },
}

/// One interrupt output of a PL IP and where it lands. The generator emits a
/// constant per connected interrupt in the generated crate's `pac`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqLine {
    pub instance: &'static str,
    pub port: &'static str,
    /// Bit of the PS7 `IRQ_F2P` vector, if it could be traced.
    pub f2p_bit: Option<u32>,
    /// Zynq-7000 GIC SPI device-tree cell for that bit.
    pub gic_spi: Option<u32>,
    /// Name of the `uio` node in the generated overlay (`interrupts.dtso`).
    pub dt_node: &'static str,
    /// Physical base address of the IP's register window.
    pub base: u64,
}

/// Parse a sysfs hex value such as `0x40400000\n`.
fn parse_hex(s: &str) -> Option<u64> {
    let t = s.trim();
    u64::from_str_radix(t.strip_prefix("0x").unwrap_or(t), 16).ok()
}

/// Find the `uioN` device for an interrupt by scanning a `/sys/class/uio`
/// tree: first a device whose `name` equals `node`, otherwise the *only*
/// device whose `maps/map0/addr` equals `base`. Returns the device number.
pub fn find_uio(sysfs_uio: &Path, node: &str, base: u64) -> Result<Option<u32>, IrqError> {
    let rd = match std::fs::read_dir(sysfs_uio) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(IrqError::Io {
                path: sysfs_uio.to_path_buf(),
                source,
            });
        }
    };
    let mut devs: Vec<(u32, PathBuf)> = rd
        .filter_map(|e| {
            let p = e.ok()?.path();
            let n = p.file_name()?.to_str()?.strip_prefix("uio")?.parse().ok()?;
            Some((n, p))
        })
        .collect();
    devs.sort();
    let mut by_addr = Vec::new();
    for (n, p) in &devs {
        let name = std::fs::read_to_string(p.join("name")).unwrap_or_default();
        if name.trim() == node {
            return Ok(Some(*n));
        }
        let addr = std::fs::read_to_string(p.join("maps/map0/addr"))
            .ok()
            .and_then(|a| parse_hex(&a));
        if addr == Some(base) {
            by_addr.push(*n);
        }
    }
    Ok(if by_addr.len() == 1 {
        Some(by_addr[0])
    } else {
        None
    })
}

impl IrqLine {
    /// Open this interrupt's UIO device (`/sys/class/uio`, `/dev/uioN`).
    pub fn open(&self) -> Result<UioIrq, IrqError> {
        self.open_in(Path::new("/sys/class/uio"), Path::new("/dev"))
    }

    /// Like [`open`](Self::open) with the sysfs and device directories given
    /// (for tests).
    pub fn open_in(&self, sysfs_uio: &Path, dev_dir: &Path) -> Result<UioIrq, IrqError> {
        match find_uio(sysfs_uio, self.dt_node, self.base)? {
            Some(n) => UioIrq::open(dev_dir.join(format!("uio{n}"))),
            None => Err(IrqError::NotFound {
                instance: self.instance,
                port: self.port,
                node: self.dt_node,
                base: self.base,
            }),
        }
    }
}

pub struct UioIrq {
    file: File,
    path: PathBuf,
}

impl UioIrq {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, IrqError> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|source| IrqError::Io {
                path: path.clone(),
                source,
            })?;
        Ok(Self { file, path })
    }

    /// Wrap an already-open descriptor (tests use a pipe).
    pub fn from_file(file: File, path: impl Into<PathBuf>) -> Self {
        Self {
            file,
            path: path.into(),
        }
    }

    fn io(&self, source: std::io::Error) -> IrqError {
        IrqError::Io {
            path: self.path.clone(),
            source,
        }
    }

    /// Unmask the interrupt in the UIO core.
    pub fn enable(&self) -> Result<(), IrqError> {
        (&self.file)
            .write_all(&1u32.to_ne_bytes())
            .map_err(|e| self.io(e))
    }

    /// Unmask, then wait for the next interrupt. Returns the cumulative
    /// event count, or `None` on timeout. The caller must acknowledge the
    /// device (e.g. `HlsIp::ack_interrupts`) before waiting again.
    pub fn wait(&self, timeout: Option<Duration>) -> Result<Option<u32>, IrqError> {
        self.enable()?;
        self.wait_event(timeout)
    }

    /// Wait without re-enabling first.
    pub fn wait_event(&self, timeout: Option<Duration>) -> Result<Option<u32>, IrqError> {
        let mut pfd = libc::pollfd {
            fd: self.file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ms = timeout.map_or(-1, |t| t.as_millis().min(i32::MAX as u128) as i32);
        loop {
            // SAFETY: `pfd` is a valid pollfd and we pass exactly one.
            let n = unsafe { libc::poll(&mut pfd, 1, ms) };
            match n {
                0 => return Ok(None),
                n if n > 0 => break,
                _ => {
                    let e = std::io::Error::last_os_error();
                    if e.kind() != std::io::ErrorKind::Interrupted {
                        return Err(self.io(e));
                    }
                }
            }
        }
        let mut buf = [0u8; 4];
        (&self.file).read_exact(&mut buf).map_err(|e| self.io(e))?;
        Ok(Some(u32::from_ne_bytes(buf)))
    }
}

#[cfg(feature = "async")]
mod async_impl {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll, Waker};

    struct Shared {
        result: Option<Result<Option<u32>, IrqError>>,
        waker: Option<Waker>,
    }

    /// Future returned by [`UioIrq::wait_async`].
    pub struct WaitFuture(Arc<Mutex<Shared>>);

    impl UioIrq {
        /// Async [`UioIrq::wait`]: a helper thread blocks in `poll` and wakes
        /// the task. Works under any executor.
        pub fn wait_async(self: &Arc<Self>, timeout: Option<Duration>) -> WaitFuture {
            let shared = Arc::new(Mutex::new(Shared {
                result: None,
                waker: None,
            }));
            let (me, sh) = (self.clone(), shared.clone());
            std::thread::spawn(move || {
                let r = me.wait(timeout);
                let mut g = sh.lock().unwrap();
                g.result = Some(r);
                if let Some(w) = g.waker.take() {
                    w.wake();
                }
            });
            WaitFuture(shared)
        }
    }

    impl Future for WaitFuture {
        type Output = Result<Option<u32>, IrqError>;
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let mut g = self.0.lock().unwrap();
            match g.result.take() {
                Some(r) => Poll::Ready(r),
                None => {
                    g.waker = Some(cx.waker().clone());
                    Poll::Pending
                }
            }
        }
    }
}

#[cfg(feature = "async")]
pub use async_impl::WaitFuture;

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::FromRawFd;

    /// A pipe pair: `(read end as the fake uio, write end for the test)`.
    /// The fake device also needs to accept `enable()` writes, so use a
    /// socketpair (bidirectional) for the UIO side.
    fn pair() -> (File, File) {
        let mut fds = [0; 2];
        // SAFETY: valid out-array of 2 ints.
        assert_eq!(
            unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) },
            0
        );
        // SAFETY: fresh descriptors we own.
        unsafe { (File::from_raw_fd(fds[0]), File::from_raw_fd(fds[1])) }
    }

    fn fake_sysfs(tag: &str, devs: &[(&str, &str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("hrx-uio-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (dev, name, addr) in devs {
            let d = root.join(dev);
            std::fs::create_dir_all(d.join("maps/map0")).unwrap();
            std::fs::write(d.join("name"), format!("{name}\n")).unwrap();
            std::fs::write(d.join("maps/map0/addr"), format!("{addr}\n")).unwrap();
        }
        root
    }

    #[test]
    fn finds_uio_by_node_name_then_unique_address() {
        let r = fake_sysfs(
            "find",
            &[
                ("uio0", "other", "0x1000"),
                ("uio1", "dma_mm2s", "0x40400000"),
                ("uio2", "dma_s2mm", "0x40400000"),
            ],
        );
        assert_eq!(find_uio(&r, "dma_s2mm", 0x40400000).unwrap(), Some(2));
        // No name match, address is shared by two devices: ambiguous.
        assert_eq!(find_uio(&r, "missing", 0x40400000).unwrap(), None);
        // No name match, unique address.
        assert_eq!(find_uio(&r, "missing", 0x1000).unwrap(), Some(0));
        assert_eq!(find_uio(&r.join("nonexistent"), "x", 0).unwrap(), None);
        std::fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn irq_line_open_in_reports_missing_overlay() {
        let line = IrqLine {
            instance: "axi_dma_0",
            port: "mm2s_introut",
            f2p_bit: Some(0),
            gic_spi: Some(29),
            dt_node: "axi_dma_0_mm2s_introut",
            base: 0x40400000,
        };
        let r = fake_sysfs("missing", &[("uio0", "unrelated", "0x1000")]);
        let e = line.open_in(&r, Path::new("/dev")).err().unwrap();
        assert!(matches!(e, IrqError::NotFound { .. }));
        assert!(e.to_string().contains("interrupts.dtso"));
        // Found: opens the device node (a regular file stands in for /dev/uioN).
        let r2 = fake_sysfs("found", &[("uio3", "axi_dma_0_mm2s_introut", "0x40400000")]);
        let devdir = r2.join("devs");
        std::fs::create_dir_all(&devdir).unwrap();
        std::fs::write(devdir.join("uio3"), "").unwrap();
        line.open_in(&r2, &devdir).unwrap();
        std::fs::remove_dir_all(r).unwrap();
        std::fs::remove_dir_all(r2).unwrap();
    }

    #[test]
    fn times_out_then_receives_count() {
        let (uio, mut dev) = pair();
        let irq = UioIrq::from_file(uio, "fake");
        assert_eq!(irq.wait(Some(Duration::from_millis(20))).unwrap(), None);
        dev.write_all(&7u32.to_ne_bytes()).unwrap();
        // enable() wrote 1u32 twice by now; the device side saw them.
        assert_eq!(irq.wait(Some(Duration::from_millis(200))).unwrap(), Some(7));
        let mut seen = [0u8; 8];
        dev.read_exact(&mut seen).unwrap();
        assert_eq!(seen, [1, 0, 0, 0, 1, 0, 0, 0]);
    }

    #[cfg(feature = "async")]
    #[test]
    fn async_wait_completes() {
        use std::sync::Arc;
        use std::task::{Context, Poll, Wake, Waker};
        struct Flag(std::sync::atomic::AtomicBool, std::thread::Thread);
        impl Wake for Flag {
            fn wake(self: Arc<Self>) {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
                self.1.unpark();
            }
        }
        let (uio, mut dev) = pair();
        let irq = Arc::new(UioIrq::from_file(uio, "fake"));
        let mut fut = Box::pin(irq.wait_async(Some(Duration::from_secs(2))));
        let flag = Arc::new(Flag(false.into(), std::thread::current()));
        let waker = Waker::from(flag.clone());
        let mut cx = Context::from_waker(&waker);
        assert!(fut.as_mut().poll(&mut cx).is_pending());
        dev.write_all(&3u32.to_ne_bytes()).unwrap();
        while !flag.0.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::park_timeout(Duration::from_millis(50));
        }
        use std::future::Future;
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(r) => assert_eq!(r.unwrap(), Some(3)),
            Poll::Pending => panic!("not ready after wake"),
        }
    }
}

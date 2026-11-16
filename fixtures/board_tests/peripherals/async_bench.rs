use hardroxide_mmio::Mmio;
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::{Duration, Instant};

struct Park(std::thread::Thread);
impl Wake for Park { fn wake(self: Arc<Self>) { self.0.unpark(); } }
fn block_on<F: Future>(f: F) -> F::Output {
    let waker = Waker::from(Arc::new(Park(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut f = pin!(f);
    loop { if let Poll::Ready(v) = f.as_mut().poll(&mut cx) { return v; } std::thread::park(); }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = design_1::Design::load()?;
    let t = &d.axi_timer_0;
    // register access latency
    let r = t.regs();
    let n = 200_000u32;
    let s = Instant::now();
    let mut acc = 0u32;
    for _ in 0..n { acc ^= r.read32(0x8)?; }
    let rd = s.elapsed().as_nanos() as f64 / n as f64;
    let s = Instant::now();
    for i in 0..n { r.write32(0x4, i)?; }
    let wr = s.elapsed().as_nanos() as f64 / n as f64;
    println!("register read {rd:.0} ns, write {wr:.0} ns (acc {acc})");
    r.write32(0x4, 0)?;
    // async interrupt wait
    let irq = Arc::new(design_1::pac::axi_timer_0::IRQ_INTERRUPT.open()?);
    for round in 0..3 {
        t.clear()?;
        let t0 = Instant::now();
        t.start(t.ticks_for_ns(40_000_000) as u32, false, true)?;
        let ev = block_on(irq.wait_async(Some(Duration::from_secs(2))))?;
        println!("async round {round}: event {ev:?} after {:?}", t0.elapsed());
        t.clear()?;
    }
    let ev = block_on(irq.wait_async(Some(Duration::from_millis(200))))?;
    println!("async idle wait -> {ev:?} (timeout path)");
    Ok(())
}

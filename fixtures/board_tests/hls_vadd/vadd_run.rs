use std::time::Duration;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = design_1::Design::load()?;
    let v = &d.vadd_0;
    for (a, b) in [(1u32, 2u32), (100, 23), (0xFFFF_FFFF, 1), (7, 35)] {
        v.set_a(a)?; v.set_b(b)?;
        v.start()?;
        v.wait_done(Duration::from_secs(1))?;
        println!("{a} + {b} = {} (ctrl {:#x})", v.out_r()?, v.out_r_ctrl()?);
    }
    Ok(())
}

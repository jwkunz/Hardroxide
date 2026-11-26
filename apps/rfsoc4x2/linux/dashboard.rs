//! RFSoC 4x2 base overlay under Linux: a web page for the board. It shows the buttons, switches, temperatures and the
//! power monitors, and sets the white and colour LEDs. FPGA blocks are reached through the generated crate; the power
//! monitors through Linux's own drivers (`/sys/class/hwmon`).
//!
//! `dashboard [address:port]`, as root; without an argument it listens on `127.0.0.1:8080`, which only the board itself
//! can reach (use an SSH tunnel). **There is no password**: whoever can reach the address can set the LEDs.
use design_1::Design;
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::{DevMem, Mmio};
use std::error::Error;
use std::fmt::Write as _;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::time::Duration;

/// The system monitor's latest temperature reading.
const TEMPERATURE: usize = 0x400;
/// The supplies the board monitors, by their monitor's I2C address.
const RAILS: [&str; 9] = [
    "VCC0V85",
    "VCC3V3",
    "VCC1V8",
    "VDAC_AVCC",
    "VADC_AVCC",
    "VADC_AVCCAUX",
    "VDAC_AVCCAUX",
    "VDAC_AVTT",
    "SYZYGY_VIO",
];

const PAGE: &str = r#"<!doctype html><meta charset="utf-8"><title>RFSoC 4x2</title>
<style>body{font:16px sans-serif;max-width:34em;margin:2em auto;padding:0 1em}button{font:inherit;padding:.4em .8em;margin:.2em}
button.on{background:#2a7;color:#fff}td,th{padding:.1em .8em;text-align:right}td:first-child{text-align:left}</style>
<h1>RFSoC 4x2</h1><p id="inputs">...</p><p>White LEDs: <span id="leds"></span></p><p>Colour LEDs: <span id="colour"></span></p>
<table id="power"></table>
<script>
const names=["off","red","green","yellow","blue","magenta","cyan","white"];
const bits=(v,n)=>v.toString(2).padStart(n,"0");
async function set(what,value){await fetch(`/${what}?value=${value}`,{method:"POST"});show()}
function button(label,on,what,value){const b=document.createElement("button");b.textContent=label;if(on)b.className="on";b.onclick=()=>set(what,value);return b}
async function show(){
 const s=await (await fetch("/state")).json();
 inputs.textContent=`FPGA ${s.celsius.toFixed(1)} C. Buttons ${bits(s.buttons,4)}, switches ${bits(s.switches,4)}.`;
 leds.replaceChildren(...[0,1,2,3].map(i=>button(i,s.leds>>i&1,"leds",s.leds^1<<i)));
 colour.replaceChildren(...names.map((n,i)=>button(n,s.colour==i,"colour",i)));
 power.innerHTML="<tr><th>supply</th><th>V</th><th>A</th><th>W</th></tr>"+s.power.map(p=>`<tr><td>${p.name}</td><td>${p.volts.toFixed(3)}</td><td>${p.amps.toFixed(3)}</td><td>${p.watts.toFixed(2)}</td></tr>`).join("");
}
show();setInterval(show,1000);
</script>"#;

/// The colour LEDs' six bits for both LEDs showing the colour `rgb` (bit 0 red, 1 green, 2 blue): the overlay wires
/// each LED as blue, green, red.
fn colour_bits(rgb: u32) -> u32 {
    let bgr = (rgb & 1) << 2 | (rgb & 2) | (rgb & 4) >> 2;
    bgr | bgr << 3
}

/// One number from a hardware monitor's file.
fn number(dir: &Path, file: &str) -> Option<f64> {
    std::fs::read_to_string(dir.join(file))
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Which of [`RAILS`] a hardware monitor is, if it is one of them: an INA220 whose device is at `<bus>-0040` to `0048`.
fn rail(dir: &Path) -> Option<usize> {
    let name = std::fs::read_to_string(dir.join("name")).ok()?;
    if name.trim() != "ina220" {
        return None;
    }
    let device = std::fs::read_link(dir.join("device")).ok()?;
    let (_, address) = device.file_name()?.to_str()?.split_once('-')?;
    let index = usize::from_str_radix(address, 16).ok()?.checked_sub(0x40)?;
    (index < RAILS.len()).then_some(index)
}

/// The board's power monitors as Linux's drivers see them, as JSON objects.
fn power() -> String {
    let mut rails: Vec<(usize, String)> = Vec::new();
    for entry in std::fs::read_dir("/sys/class/hwmon").into_iter().flatten() {
        let Ok(dir) = entry.map(|e| e.path()) else {
            continue;
        };
        let Some(index) = rail(&dir) else {
            continue;
        };
        let read = |file, per| number(&dir, file).map(|v| v / per);
        // Millivolts, milliamperes and microwatts.
        if let (Some(volts), Some(amps), Some(watts)) = (
            read("in1_input", 1e3),
            read("curr1_input", 1e3),
            read("power1_input", 1e6),
        ) {
            rails.push((
                index,
                format!(
                    r#"{{"name":"{}","volts":{volts},"amps":{amps},"watts":{watts}}}"#,
                    RAILS[index]
                ),
            ));
        }
    }
    rails.sort();
    let rails: Vec<String> = rails.into_iter().map(|(_, json)| json).collect();
    rails.join(",")
}

struct Board {
    design: Design<DevMem>,
    /// The colour last chosen (the LEDs' own register holds the wiring's order).
    colour: u32,
}

impl Board {
    fn state(&self) -> Result<String, Box<dyn Error>> {
        let d = &self.design;
        let reading = d.system_management_wiz_0.read32(TEMPERATURE)? as u16;
        let celsius = f64::from(reading) * 509.314 / 65536.0 - 280.2309;
        let mut json = String::new();
        write!(
            json,
            r#"{{"celsius":{celsius:.2},"buttons":{},"switches":{},"leds":{},"colour":{},"power":[{}]}}"#,
            d.btns_gpio.read(Channel::One)? & 0xF,
            d.sws_gpio.read(Channel::One)? & 0xF,
            d.leds_gpio.read(Channel::One)? & 0xF,
            self.colour,
            power()
        )?;
        Ok(json)
    }

    /// The answer to a request's first line: `(status, content type, body)`.
    fn answer(
        &mut self,
        request: &str,
    ) -> Result<(&'static str, &'static str, String), Box<dyn Error>> {
        let mut words = request.split_whitespace();
        let (method, path) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        let value = query
            .strip_prefix("value=")
            .and_then(|v| v.parse::<u32>().ok());
        Ok(match (method, path, value) {
            ("GET", "/", _) => ("200 OK", "text/html; charset=utf-8", PAGE.into()),
            ("GET", "/state", _) => ("200 OK", "application/json", self.state()?),
            ("POST", "/leds", Some(v @ 0..=15)) => {
                self.design.leds_gpio.write(Channel::One, v)?;
                ("200 OK", "text/plain", "ok\n".into())
            }
            ("POST", "/colour", Some(v @ 0..=7)) => {
                self.design
                    .rgbleds_gpio
                    .write(Channel::One, colour_bits(v))?;
                self.colour = v;
                ("200 OK", "text/plain", "ok\n".into())
            }
            _ => ("404 Not Found", "text/plain", "not found\n".into()),
        })
    }

    fn serve(&mut self, mut stream: TcpStream) -> Result<(), Box<dyn Error>> {
        // One request at a time: a client that stays silent must not hold up the others.
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let mut head = Vec::new();
        let mut chunk = [0u8; 1024];
        while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < 8192 {
            let n = match stream.read(&mut chunk) {
                // Browsers open spare connections and say nothing on them.
                Err(e) if head.is_empty() && e.kind() == ErrorKind::WouldBlock => return Ok(()),
                read => read?,
            };
            if n == 0 {
                break;
            }
            head.extend_from_slice(&chunk[..n]);
        }
        let head = String::from_utf8_lossy(&head);
        let (status, kind, body) = self.answer(head.lines().next().unwrap_or(""))?;
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )?;
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".into());
    // Listen first: a taken port should not cost a bitstream load.
    let listener = TcpListener::bind(&address)?;
    let design = Design::load()?;
    design.leds_gpio.set_input_mask(Channel::One, 0)?;
    design.rgbleds_gpio.set_input_mask(Channel::One, 0)?;
    design.rgbleds_gpio.write(Channel::One, 0)?;
    let mut board = Board { design, colour: 0 };
    println!("the board's page is at http://{address}/ (Ctrl-C to stop)");
    for stream in listener.incoming() {
        // A client that goes away is not the server's failure.
        if let Err(e) = stream.map_err(Into::into).and_then(|s| board.serve(s)) {
            eprintln!("request: {e}");
        }
    }
    Ok(())
}

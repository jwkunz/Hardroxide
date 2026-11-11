use crate::{Error, model::*};
use roxmltree::{Document, Node};
use std::collections::BTreeMap;

fn bad(msg: impl Into<String>) -> Error {
    Error::Hwh(msg.into())
}

fn num(s: &str) -> Option<u64> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u64::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

fn num_attr(n: Node, k: &str) -> Option<i64> {
    n.attribute(k)?.trim().parse().ok()
}

fn access(s: &str) -> Access {
    match s {
        "read-write" => Access::ReadWrite,
        "read-only" => Access::ReadOnly,
        "write-only" => Access::WriteOnly,
        _ => Access::Other,
    }
}

fn props<'a>(n: Node<'a, 'a>) -> BTreeMap<&'a str, &'a str> {
    n.children()
        .filter(|c| c.has_tag_name("PROPERTY"))
        .filter_map(|c| Some((c.attribute("NAME")?, c.attribute("VALUE")?)))
        .collect()
}

fn parse_field(n: Node) -> Result<Field, Error> {
    let p = props(n);
    let name = n
        .attribute("NAME")
        .ok_or_else(|| bad("FIELD without NAME"))?;
    let get = |k: &str| {
        p.get(k)
            .and_then(|v| num(v))
            .ok_or_else(|| bad(format!("field {name}: missing/invalid {k}")))
    };
    Ok(Field {
        name: name.into(),
        description: p.get("DESCRIPTION").copied().unwrap_or("").into(),
        bit_offset: get("BIT_OFFSET")? as u32,
        bit_width: get("BIT_WIDTH")? as u32,
        access: access(p.get("ACCESS").copied().unwrap_or("")),
    })
}

fn parse_register(n: Node) -> Result<Register, Error> {
    let p = props(n);
    let name = n
        .attribute("NAME")
        .ok_or_else(|| bad("REGISTER without NAME"))?;
    let offset = p
        .get("ADDRESS_OFFSET")
        .and_then(|v| num(v))
        .ok_or_else(|| bad(format!("register {name}: missing ADDRESS_OFFSET")))?;
    let fields = n
        .children()
        .filter(|c| c.has_tag_name("FIELDS"))
        .flat_map(|f| f.children().filter(|c| c.has_tag_name("FIELD")))
        .map(parse_field)
        .collect::<Result<_, _>>()?;
    Ok(Register {
        name: name.into(),
        description: p.get("DESCRIPTION").copied().unwrap_or("").into(),
        offset,
        size: p.get("SIZE").and_then(|v| num(v)).unwrap_or(32) as u32,
        access: access(p.get("ACCESS").copied().unwrap_or("")),
        reset_value: p.get("RESET_VALUE").and_then(|v| num(v)).unwrap_or(0),
        fields,
    })
}

fn parse_instance(m: Node) -> Result<Instance, Error> {
    let name = m
        .attribute("INSTANCE")
        .ok_or_else(|| bad("MODULE without INSTANCE"))?;
    let mut parameters = BTreeMap::new();
    for pn in m
        .children()
        .filter(|c| c.has_tag_name("PARAMETERS"))
        .flat_map(|c| c.children().filter(|c| c.has_tag_name("PARAMETER")))
    {
        if let (Some(k), Some(v)) = (pn.attribute("NAME"), pn.attribute("VALUE")) {
            parameters.insert(k.to_string(), v.to_string());
        }
    }
    let mut address_blocks = Vec::new();
    for ab in m
        .children()
        .filter(|c| c.has_tag_name("ADDRESSBLOCKS"))
        .flat_map(|c| c.children().filter(|c| c.has_tag_name("ADDRESSBLOCK")))
    {
        let registers = ab
            .children()
            .filter(|c| c.has_tag_name("REGISTERS"))
            .flat_map(|r| r.children().filter(|c| c.has_tag_name("REGISTER")))
            .map(parse_register)
            .collect::<Result<_, _>>()?;
        address_blocks.push(AddressBlock {
            name: ab.attribute("NAME").unwrap_or("").into(),
            interface: ab.attribute("INTERFACE").unwrap_or("").into(),
            range: ab.attribute("RANGE").and_then(num).unwrap_or(0),
            usage: ab.attribute("USAGE").unwrap_or("").into(),
            registers,
        });
    }
    let interrupts = m
        .descendants()
        .filter(|c| {
            c.has_tag_name("PORT")
                && c.attribute("DIR") == Some("O")
                && c.attribute("SIGIS")
                    .is_some_and(|s| s.eq_ignore_ascii_case("INTERRUPT"))
        })
        .map(|c| {
            let sinks = c
                .descendants()
                .filter(|x| x.has_tag_name("CONNECTION"))
                .filter_map(|x| {
                    Some((
                        x.attribute("INSTANCE")?.to_string(),
                        x.attribute("PORT")?.to_string(),
                    ))
                })
                .collect();
            let (l, r) = (num_attr(c, "LEFT"), num_attr(c, "RIGHT"));
            InterruptPort {
                name: c.attribute("NAME").unwrap_or("").into(),
                signal: c.attribute("SIGNAME").unwrap_or("").into(),
                sinks,
                width: l.zip(r).map_or(1, |(l, r)| l.abs_diff(r) as u32 + 1),
                sensitivity: c.attribute("SENSITIVITY").unwrap_or("").into(),
            }
        })
        .collect();
    let ports = m
        .descendants()
        .filter(|c| c.has_tag_name("PORT") && c.parent().is_some_and(|p| p.has_tag_name("PORTS")))
        .map(|c| {
            let sinks = c
                .descendants()
                .filter(|x| x.has_tag_name("CONNECTION"))
                .filter_map(|x| {
                    Some((
                        x.attribute("INSTANCE")?.to_string(),
                        x.attribute("PORT")?.to_string(),
                    ))
                })
                .collect();
            let (l, r) = (num_attr(c, "LEFT"), num_attr(c, "RIGHT"));
            Port {
                name: c.attribute("NAME").unwrap_or("").into(),
                dir: c.attribute("DIR").unwrap_or("").into(),
                width: l.zip(r).map_or(1, |(l, r)| l.abs_diff(r) as u32 + 1),
                sinks,
                clk_hz: c.attribute("CLKFREQUENCY").and_then(|v| v.parse().ok()),
            }
        })
        .collect();
    Ok(Instance {
        name: name.into(),
        vlnv: m.attribute("VLNV").unwrap_or("").into(),
        mod_type: m.attribute("MODTYPE").unwrap_or("").into(),
        is_ps: m.attribute("IS_PL") == Some("FALSE"),
        parameters,
        address_blocks,
        interrupts,
        ports,
    })
}

fn pl_clocks(ps: &Instance) -> Vec<PlClock> {
    let flag = |k: String| ps.parameters.get(&k).is_some_and(|v| v == "1");
    (0..4u8)
        .map(|i| {
            let mhz: f64 = ps
                .parameters
                .get(&format!("PCW_ACT_FPGA{i}_PERIPHERAL_FREQMHZ"))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
            PlClock {
                index: i,
                enabled: flag(format!("PCW_EN_CLK{i}_PORT")),
                freq_hz: (mhz * 1e6).round() as u64,
                reset_enabled: flag(format!("PCW_EN_RST{i}_PORT")),
            }
        })
        .collect()
}

/// Parse the text of an `.hwh` file.
pub fn parse_hwh(xml: &str) -> Result<Design, Error> {
    let xml = xml.trim_start_matches('\u{feff}');
    let doc = Document::parse(xml)?;
    let root = doc.root_element();
    if !root.has_tag_name("EDKSYSTEM") {
        return Err(bad("root element is not EDKSYSTEM"));
    }
    let sys = root
        .children()
        .find(|c| c.has_tag_name("SYSTEMINFO"))
        .ok_or_else(|| bad("missing SYSTEMINFO"))?;
    let modules = root
        .children()
        .find(|c| c.has_tag_name("MODULES"))
        .ok_or_else(|| bad("missing MODULES"))?;

    let mut instances = Vec::new();
    let mut memory_ranges = Vec::new();
    for m in modules.children().filter(|c| c.has_tag_name("MODULE")) {
        instances.push(parse_instance(m)?);
        for r in m
            .children()
            .filter(|c| c.has_tag_name("MEMORYMAP"))
            .flat_map(|c| c.children().filter(|c| c.has_tag_name("MEMRANGE")))
        {
            let a = |k: &str| {
                r.attribute(k)
                    .ok_or_else(|| bad(format!("MEMRANGE missing {k}")))
            };
            let n = |k: &str| num(a(k)?).ok_or_else(|| bad(format!("MEMRANGE bad {k}")));
            memory_ranges.push(MemRange {
                instance: a("INSTANCE")?.into(),
                address_block: a("ADDRESSBLOCK")?.into(),
                slave_interface: r.attribute("SLAVEBUSINTERFACE").unwrap_or("").into(),
                master_interface: r.attribute("MASTERBUSINTERFACE").unwrap_or("").into(),
                base: n("BASEVALUE")?,
                high: n("HIGHVALUE")?,
            });
        }
    }
    let ps_clk_hz = instances
        .iter()
        .find(|i| i.is_ps)
        .and_then(|i| i.parameters.get("PCW_CRYSTAL_PERIPHERAL_FREQMHZ"))
        .and_then(|v| v.parse::<f64>().ok())
        .map_or(0, |mhz| (mhz * 1e6).round() as u64);
    let pl_clocks = instances
        .iter()
        .find(|i| i.is_ps && i.mod_type == "processing_system7")
        .map(pl_clocks)
        .unwrap_or_default();

    let s = |k: &str| sys.attribute(k).unwrap_or("").to_string();
    Ok(Design {
        name: s("NAME"),
        vivado_version: root.attribute("VIVADOVERSION").unwrap_or("").into(),
        arch: s("ARCH"),
        device: s("DEVICE"),
        package: s("PACKAGE"),
        speed_grade: s("SPEEDGRADE"),
        instances,
        memory_ranges,
        pl_clocks,
        ps_clk_hz,
    })
}

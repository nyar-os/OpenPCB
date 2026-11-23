//! Temporary preview driver: builds a demo board and exports SVG, PDF, and PNG.

use std::path::Path;

use open_pcb_model::{
    AddFootprintRequest, AddNetRequest, AddPadRequest, AddTextRequest, AddTrackRequest, AddViaRequest, CreateRequest,
    SetOutlineRequest, add_footprint, add_net, add_pad, add_text, add_track, add_via, create, export, set_outline,
};
use serde_json::Value;

fn id_of(value: &Value) -> String {
    value["object"]["id"]
        .as_str()
        .or_else(|| value["id"].as_str())
        .or_else(|| value["footprint"]["id"].as_str())
        .unwrap_or_else(|| panic!("no id in {value}"))
        .to_string()
}

fn footprint(
    handle: u64,
    reference: &str,
    value: &str,
    footprint: &str,
    x: f64,
    y: f64,
) -> Result<String, Box<dyn std::error::Error>> {
    let created = add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some(reference.into()),
            value: Some(value.into()),
            footprint: Some(footprint.into()),
            x: Some(x),
            y: Some(y),
            rotation: Some(0.0),
            layer: None,
        },
    )?;
    Ok(id_of(&created))
}

fn pad(
    handle: u64,
    footprint: &str,
    number: &str,
    at: [f64; 2],
    size: [f64; 2],
    net: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    add_pad(
        handle,
        AddPadRequest {
            footprint: footprint.into(),
            number: number.into(),
            x: at[0],
            y: at[1],
            w: size[0],
            h: size[1],
            net: net.map(str::to_string),
            drill: None,
        },
    )?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "preview".into());
    let out = Path::new(&out);
    std::fs::create_dir_all(out)?;

    let created = create(CreateRequest { title: Some("OpenPCB demo board".into()), template: None })?;
    let handle = created["handle"].as_u64().expect("handle");
    set_outline(handle, SetOutlineRequest { from: [4.0, 4.0], to: [76.0, 56.0] })?;
    for name in ["VCC", "GND", "IO1", "IO2", "LED"] {
        add_net(handle, AddNetRequest { name: name.into() })?;
    }

    // The circuit: U1 is decoupled by C1 (VCC/GND), IO1 has an R1 pull-up to
    // VCC, and IO2 drives an LED through R2 into GND.
    let u1 = footprint(handle, "U1", "MCU", "QFN-16", 40.0, 30.0)?;
    let u1_nets = ["VCC", "IO1", "IO2", "GND", "VCC", "", "", "GND"];
    for (index, net) in u1_nets.iter().enumerate() {
        let column = (index % 4) as f64;
        let row = (index / 4) as f64;
        pad(
            handle,
            &u1,
            &(index + 1).to_string(),
            [-3.6 + column * 2.4, -1.2 + row * 2.4],
            [1.5, 1.0],
            (!net.is_empty()).then_some(net),
        )?;
    }

    let r1 = footprint(handle, "R1", "10k", "0603", 16.0, 44.0)?;
    pad(handle, &r1, "1", [-1.6, 0.0], [1.2, 1.4], Some("IO1"))?;
    pad(handle, &r1, "2", [1.6, 0.0], [1.2, 1.4], Some("VCC"))?;

    let r2 = footprint(handle, "R2", "1k", "0603", 62.0, 16.0)?;
    pad(handle, &r2, "1", [-1.6, 0.0], [1.2, 1.4], Some("IO2"))?;
    pad(handle, &r2, "2", [1.6, 0.0], [1.2, 1.4], Some("LED"))?;

    let c1 = footprint(handle, "C1", "100n", "0603", 60.0, 44.0)?;
    pad(handle, &c1, "1", [-1.6, 0.0], [1.2, 1.4], Some("VCC"))?;
    pad(handle, &c1, "2", [1.6, 0.0], [1.2, 1.4], Some("GND"))?;

    let d1 = footprint(handle, "D1", "LED", "0603", 68.0, 16.0)?;
    pad(handle, &d1, "1", [-1.6, 0.0], [1.2, 1.4], Some("LED"))?;
    pad(handle, &d1, "2", [1.6, 0.0], [1.2, 1.4], Some("GND"))?;

    // Every endpoint below sits on a pad centre or a via. Each segment is
    // axis-aligned, leaves its footprint through the pad row it belongs to,
    // and never crosses a body or a pad of another net.
    let tracks = [
        // VCC: U1 pin 1 and pin 5 feed the bottom rail to R1 and C1.
        ([36.4, 28.8], [36.4, 26.0], "F.Cu", "VCC"),
        ([36.4, 26.0], [17.6, 26.0], "F.Cu", "VCC"),
        ([17.6, 26.0], [17.6, 44.0], "F.Cu", "VCC"),
        ([36.4, 31.2], [36.4, 40.0], "F.Cu", "VCC"),
        ([17.6, 40.0], [58.4, 40.0], "F.Cu", "VCC"),
        ([58.4, 40.0], [58.4, 44.0], "F.Cu", "VCC"),
        // IO1: U1 pin 2 drops to the inner layer and runs left to R1.
        ([38.8, 28.8], [38.8, 20.0], "F.Cu", "IO1"),
        ([38.8, 20.0], [14.4, 20.0], "B.Cu", "IO1"),
        ([14.4, 20.0], [14.4, 44.0], "B.Cu", "IO1"),
        // IO2: U1 pin 3 runs right above the top parts into R2.
        ([41.2, 28.8], [41.2, 22.0], "F.Cu", "IO2"),
        ([41.2, 22.0], [60.4, 22.0], "F.Cu", "IO2"),
        ([60.4, 22.0], [60.4, 16.0], "F.Cu", "IO2"),
        // LED: R2 to D1.
        ([63.6, 16.0], [66.4, 16.0], "F.Cu", "LED"),
        // GND: U1 pin 4 to D1, U1 pin 8 across the inner layer to C1.
        ([43.6, 28.8], [43.6, 24.0], "F.Cu", "GND"),
        ([43.6, 24.0], [69.6, 24.0], "F.Cu", "GND"),
        ([69.6, 24.0], [69.6, 16.0], "F.Cu", "GND"),
        ([43.6, 31.2], [43.6, 36.0], "F.Cu", "GND"),
        ([43.6, 36.0], [61.6, 36.0], "B.Cu", "GND"),
        ([61.6, 36.0], [61.6, 44.0], "F.Cu", "GND"),
    ];
    for (from, to, layer, net) in tracks {
        add_track(handle, AddTrackRequest { net: Some(net.into()), layer: layer.into(), width: 0.5, from, to })?;
    }

    for (at, net) in [([38.8, 20.0], "IO1"), ([43.6, 36.0], "GND"), ([61.6, 36.0], "GND")] {
        add_via(handle, AddViaRequest { net: Some(net.into()), at, diameter: 1.4, drill: 0.7, ..Default::default() })?;
    }

    for (text, at) in
        [("MCU", [46.5, 35.5]), ("10k", [13.0, 48.5]), ("1k", [60.0, 12.5]), ("100n", [58.0, 48.5]), ("LED", [66.0, 12.5])]
    {
        add_text(
            handle,
            AddTextRequest { text: text.into(), layer: "F.SilkS".into(), at, rotation: None, size: Some(2.0), thickness: None },
        )?;
    }

    for format in ["svg", "pdf", "png"] {
        let target = out.join(format!("demo-board.{format}"));
        let report = export(handle, format, &target)?;
        println!("{format}: {report}");
    }
    Ok(())
}

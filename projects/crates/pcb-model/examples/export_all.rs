//! Temporary driver: builds the demo board and exports every supported format.

use std::path::Path;

use open_pcb_model::{
    AddFootprintRequest, AddNetRequest, AddPadRequest, AddTestPointRequest, AddTextRequest, AddTrackRequest, AddViaRequest,
    CreateRequest, SetOutlineRequest, add_footprint, add_net, add_pad, add_test_point, add_text, add_track, add_via, create,
    export, set_outline,
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
    fp: &str,
    number: &str,
    at: [f64; 2],
    size: [f64; 2],
    net: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    add_pad(
        handle,
        AddPadRequest {
            footprint: fp.into(),
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
    let out = std::env::args().nth(1).unwrap_or_else(|| "export-all".into());
    let out = Path::new(&out);
    std::fs::create_dir_all(out)?;

    let created = create(CreateRequest { title: Some("OpenPCB demo board".into()), template: None })?;
    let handle = created["handle"].as_u64().expect("handle");
    set_outline(handle, SetOutlineRequest { from: [4.0, 4.0], to: [76.0, 56.0] })?;
    for name in ["VCC", "GND", "IO1", "IO2", "LED"] {
        add_net(handle, AddNetRequest { name: name.into() })?;
    }

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

    let tracks = [
        ([36.4, 28.8], [36.4, 26.0], "F.Cu", "VCC"),
        ([36.4, 26.0], [17.6, 26.0], "F.Cu", "VCC"),
        ([17.6, 26.0], [17.6, 44.0], "F.Cu", "VCC"),
        ([36.4, 31.2], [36.4, 40.0], "F.Cu", "VCC"),
        ([17.6, 40.0], [58.4, 40.0], "F.Cu", "VCC"),
        ([58.4, 40.0], [58.4, 44.0], "F.Cu", "VCC"),
        ([38.8, 28.8], [38.8, 20.0], "F.Cu", "IO1"),
        ([38.8, 20.0], [14.4, 20.0], "B.Cu", "IO1"),
        ([14.4, 20.0], [14.4, 44.0], "B.Cu", "IO1"),
        ([41.2, 28.8], [41.2, 22.0], "F.Cu", "IO2"),
        ([41.2, 22.0], [60.4, 22.0], "F.Cu", "IO2"),
        ([60.4, 22.0], [60.4, 16.0], "F.Cu", "IO2"),
        ([63.6, 16.0], [66.4, 16.0], "F.Cu", "LED"),
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
    let via_ids = [([38.8, 20.0], "IO1"), ([43.6, 36.0], "GND"), ([61.6, 36.0], "GND")];
    let mut via_refs = Vec::new();
    for (at, net) in via_ids {
        let created =
            add_via(handle, AddViaRequest { net: Some(net.into()), at, diameter: 1.4, drill: 0.7, ..Default::default() })?;
        via_refs.push(id_of(&created));
    }

    // Probe targets: one pad per signal net plus one via.
    for target in [format!("{r1}:1"), format!("{c1}:2"), via_refs[0].clone()] {
        add_test_point(handle, AddTestPointRequest { target })?;
    }
    for (text, at) in
        [("MCU", [46.5, 35.5]), ("10k", [13.0, 48.5]), ("1k", [60.0, 10.5]), ("100n", [58.0, 48.5]), ("LED", [66.0, 10.5])]
    {
        add_text(
            handle,
            AddTextRequest { text: text.into(), layer: "F.SilkS".into(), at, rotation: None, size: Some(2.0), thickness: None },
        )?;
    }

    for format in [
        "gbr",
        "drl",
        "drill_map",
        "bom",
        "pos",
        "placement",
        "testpoints",
        "ipc_d_356",
        "svg",
        "pdf",
        "kicad_pcb",
        "ipc2581",
        "png",
        "layers",
        "asm",
        "stackup",
        "panel",
    ] {
        let target = out.join(format!("demo-board.{format}"));
        match export(handle, format, &target) {
            Ok(report) => println!("OK {format}: {report}"),
            Err(err) => println!("ERR {format}: {}", err.message),
        }
    }
    Ok(())
}

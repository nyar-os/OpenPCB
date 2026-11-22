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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "preview".into());
    let out = Path::new(&out);
    std::fs::create_dir_all(out)?;

    let created = create(CreateRequest { title: Some("OpenPCB demo board".into()), template: None })?;
    let handle = created["handle"].as_u64().expect("handle");
    set_outline(handle, SetOutlineRequest { from: [4.0, 4.0], to: [76.0, 56.0] })?;
    for name in ["VCC", "GND", "SIG"] {
        add_net(handle, AddNetRequest { name: name.into() })?;
    }

    let u1 = id_of(&add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("U1".into()),
            value: Some("MCU".into()),
            footprint: Some("QFN-16".into()),
            x: Some(40.0),
            y: Some(30.0),
            rotation: Some(0.0),
            layer: None,
        },
    )?);
    for index in 0..8 {
        let column = (index % 4) as f64;
        let row = (index / 4) as f64;
        add_pad(
            handle,
            AddPadRequest {
                footprint: u1.clone(),
                number: (index + 1).to_string(),
                x: -3.6 + column * 2.4,
                y: -1.2 + row * 2.4,
                w: 1.5,
                h: 1.0,
                net: Some(if column == 0.0 { "VCC" } else { "SIG" }.into()),
                drill: None,
            },
        )?;
    }

    let mut parts = Vec::new();
    for (reference, value, x, y, net) in [
        ("R1", "10k", 16.0, 44.0, "SIG"),
        ("R2", "1k", 62.0, 16.0, "SIG"),
        ("C1", "100n", 60.0, 44.0, "VCC"),
        ("D1", "LED", 20.0, 16.0, "VCC"),
    ] {
        let footprint = id_of(&add_footprint(
            handle,
            AddFootprintRequest {
                reference: Some(reference.into()),
                value: Some(value.into()),
                footprint: Some("0603".into()),
                x: Some(x),
                y: Some(y),
                rotation: Some(0.0),
                layer: None,
            },
        )?);
        for (number, offset) in [("1", -1.6_f64), ("2", 1.6_f64)] {
            add_pad(
                handle,
                AddPadRequest {
                    footprint: footprint.clone(),
                    number: number.into(),
                    x: offset,
                    y: 0.0,
                    w: 1.2,
                    h: 1.4,
                    net: Some(net.into()),
                    drill: None,
                },
            )?;
        }
        parts.push(footprint);
    }

    // Every endpoint below sits on a pad centre or a via, so the composite
    // preview shows closed copper rather than floating segments.
    let tracks = [
        ([36.4, 28.8], [36.4, 44.0], "F.Cu", "VCC"),
        ([36.4, 44.0], [58.4, 44.0], "F.Cu", "VCC"),
        ([36.4, 31.2], [18.4, 31.2], "B.Cu", "VCC"),
        ([18.4, 31.2], [18.4, 16.0], "B.Cu", "VCC"),
        ([61.6, 44.0], [66.0, 44.0], "F.Cu", "VCC"),
        ([66.0, 44.0], [66.0, 40.0], "F.Cu", "VCC"),
        ([66.0, 40.0], [50.0, 40.0], "F.Cu", "VCC"),
        ([38.8, 28.8], [38.8, 25.0], "F.Cu", "SIG"),
        ([38.8, 25.0], [14.4, 25.0], "F.Cu", "SIG"),
        ([14.4, 25.0], [14.4, 44.0], "F.Cu", "SIG"),
        ([43.6, 28.8], [60.4, 28.8], "F.Cu", "SIG"),
        ([60.4, 28.8], [60.4, 16.0], "F.Cu", "SIG"),
        ([41.2, 31.2], [41.2, 40.0], "B.Cu", "SIG"),
        ([41.2, 40.0], [26.0, 40.0], "B.Cu", "SIG"),
    ];
    for (from, to, layer, net) in tracks {
        add_track(handle, AddTrackRequest { net: Some(net.into()), layer: layer.into(), width: 0.5, from, to })?;
    }

    for (at, net) in [([26.0, 40.0], "SIG"), ([50.0, 40.0], "VCC")] {
        add_via(handle, AddViaRequest { net: Some(net.into()), at, diameter: 1.4, drill: 0.7, ..Default::default() })?;
    }

    for (text, at) in
        [("MCU", [46.5, 35.5]), ("10k", [21.0, 47.5]), ("1k", [67.0, 17.5]), ("100n", [65.0, 48.5]), ("LED", [26.0, 17.5])]
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

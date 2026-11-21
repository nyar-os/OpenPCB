# @wae/open-pcb-skills

Agent skills for OpenPCB. The package ships skill documents only; it is not a
runtime library and contains no board layout, routing, DRC or fabrication
algorithms.

## Status

Executable. `skills/open-pcb/SKILL.md` records the commands, workflows and
evidence rules for board creation, editing, DRC review and fabrication or
assembly delivery. The skill covers only the implemented surface: document core,
undo and redo, layer and outline edits, the implemented checks, KiCad PCB and
IPC-2581 import, and the Gerber, drill, BOM, position, SVG, PDF, PNG, KiCad and
IPC-2581 writers. STEP export and the interactive viewport remain explicitly
unsupported.

## Layout

```text
packages/open-pcb-skills/
├── package.json
├── README.md
└── skills/
    └── open-pcb/
        └── SKILL.md
```

The skill must never claim methods, commands or flags that the installed
SDK/CLI does not expose. Read `open-pcb capabilities` before promising a format,
check or target.


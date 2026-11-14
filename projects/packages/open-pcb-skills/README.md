# @wae/open-pcb-skills

Agent skills for OpenPCB. The package ships skill documents only; it is not a
runtime library and contains no board layout, routing, DRC or fabrication
algorithms.

## Status

Executable. `skills/open-pcb/SKILL.md` records the commands, workflows and
evidence rules for board creation, editing, DRC review and fabrication or
assembly delivery. The skill covers only the implemented surface: document core,
undo and redo, layer and outline edits, seven checks and the Gerber, drill, BOM,
position and SVG writers. STEP, IPC-2581, KiCad, PDF and the viewport remain
explicitly unsupported.

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


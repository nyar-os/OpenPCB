# @wae/open-pcb-ui

The OpenPCB Studio window: a small desktop view of a board document that runs in
the native WebView host of `@wae/open-pcb`.

## Launch

The package is not installed on its own; it ships inside the CLI.

```bash
pnpm add @wae/open-pcb
pnpm exec open-pcb gui
```

## What it shows

Studio opens a board document, prints its revision, units, layers and object
counts, and reports the same capability matrix the CLI and SDK see. It is a
summary window, not a layout editor: there is no canvas editing, routing or DRC
authoring in it.

Without a WebView host the page shows an empty canvas and a "preview mode" status
instead of inventing a board.

## Links

- [OpenPCB README](../../../README.md)
- [CLI reference](../../../documentation/cli.md)

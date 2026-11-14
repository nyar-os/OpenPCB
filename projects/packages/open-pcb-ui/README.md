# open-pcb-ui

VMZ static UI for the OpenPCB Studio window. `vmz build . --profile static`
emits `dist/static`, which the repository staging step copies to
`projects/packages/open-pcb/dist/gui` for the native WebView host.

```bash
pnpm --filter @wae/open-pcb-ui run check   # vmz check
pnpm --filter @wae/open-pcb-ui run build   # vmz build --profile static
pnpm --filter @wae/open-pcb-ui run serve   # local preview
```

The page talks to the host over `chrome.webview.postMessage` JSON-RPC
(`ready`, `capabilities`, `board.create`, `board.open`, `board.save`,
`board.close`, `close`). Without a WebView host the studio shows an empty
canvas and a "Preview mode — native window unavailable" status; it never
fabricates a board.


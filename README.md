# EasyProxy

EasyProxy is a simplified cross-platform desktop client for Mihomo. It keeps the first-run flow focused on subscription import, one-click proxy control, basic modes, node selection, and readable status.

## Development

```sh
npm install
git submodule update --init --recursive
npm run tauri -- dev
```

## Mihomo Sidecar

Mihomo is included as a git submodule at `src-tauri/vendor/mihomo`.

To build a local sidecar binary for packaging:

```sh
./scripts/build-mihomo-sidecar.sh
```

The script builds Mihomo from the submodule and copies it to `src-tauri/binaries/mihomo`, where Tauri bundles it as an app resource.

## Subscription Support

The MVP supports:

- Clash/Mihomo YAML subscriptions with `proxies`
- Raw URI subscription lists
- Base64-encoded URI subscription lists
- `anytls://` URI conversion into Mihomo proxy YAML

Unsupported URI schemes return a readable error instead of silently generating an invalid config.

## Verification

```sh
npm test
npm run lint
npm run build
cd src-tauri && cargo test
```

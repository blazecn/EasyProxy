Place platform-specific Mihomo binaries here before packaging, or build from the submodule:

- macOS/Linux: `mihomo`
- Windows: `mihomo.exe`

From the project root:

```sh
git submodule update --init --recursive
./scripts/build-mihomo-sidecar.sh
```

The MVP looks up this resource at runtime and reports a readable error if the binary is missing.

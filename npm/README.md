# slg

**Semantic git intelligence for LLM agents.**

This is the npm installer and proxy for the [slg](https://github.com/venki0552/slg) CLI. It downloads the correct pre-built binary for your platform, verifies its SHA-256 checksum, and proxies all commands through to it.

## Usage

No install required — just use `npx`:

```bash
# Run any slg command directly
npx slg-cli init
npx slg-cli why "why was the retry limit set to 3"
npx slg-cli doctor

# Install slg permanently and add it to PATH
npx slg-cli install
```

The binary is cached at `~/.slg/bin/slg` after the first download, so subsequent `npx slg-cli` calls are instant.

## Global install

```bash
npm install -g slg-cli

# Now use slg directly (no npx needed)
slg init
slg why "your question"
```

## How it works

1. Detects your platform (`linux-x64`, `linux-arm64`, `darwin-arm64`, `darwin-x64`, `win32-x64`)
2. Downloads the binary from [GitHub Releases](https://github.com/venki0552/slg/releases)
3. Verifies the SHA-256 checksum against the `.sha256` file published alongside the binary
4. Caches it at `~/.slg/bin/slg` (or `slg.exe` on Windows)
5. Execs with your arguments — zero overhead on subsequent calls

Zero npm dependencies. Uses only Node.js built-ins.

## Links

- [Full documentation](https://github.com/venki0552/slg/tree/main/docs)
- [Getting started guide](https://github.com/venki0552/slg/blob/main/docs/getting-started.md)
- [GitHub repository](https://github.com/venki0552/slg)
- [Report an issue](https://github.com/venki0552/slg/issues)

## License

MIT OR Apache-2.0

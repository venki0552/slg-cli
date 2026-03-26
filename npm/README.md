# lore-cli

**Semantic git intelligence for LLM agents.**

This is the npm installer and proxy for the [lore](https://github.com/venki0552/lore-cli) CLI. It downloads the correct pre-built binary for your platform, verifies its SHA-256 checksum, and proxies all commands through to it.

## Usage

No install required — just use `npx`:

```bash
# Run any lore command directly
npx lore-cli init
npx lore-cli why "why was the retry limit set to 3"
npx lore-cli doctor

# Install lore permanently and add it to PATH
npx lore-cli install
```

The binary is cached at `~/.lore/bin/lore` after the first download, so subsequent `npx lore-cli` calls are instant.

## Global install

```bash
npm install -g lore-cli

# Now use lore directly (no npx needed)
lore init
lore why "your question"
```

## How it works

1. Detects your platform (`linux-x64`, `linux-arm64`, `darwin-arm64`, `darwin-x64`, `win32-x64`)
2. Downloads the binary from [GitHub Releases](https://github.com/venki0552/lore-cli/releases)
3. Verifies the SHA-256 checksum against the `.sha256` file published alongside the binary
4. Caches it at `~/.lore/bin/lore` (or `lore.exe` on Windows)
5. Execs with your arguments — zero overhead on subsequent calls

Zero npm dependencies. Uses only Node.js built-ins.

## Links

- [Full documentation](https://github.com/venki0552/lore-cli/tree/main/docs)
- [Getting started guide](https://github.com/venki0552/lore-cli/blob/main/docs/getting-started.md)
- [GitHub repository](https://github.com/venki0552/lore-cli)
- [Report an issue](https://github.com/venki0552/lore-cli/issues)

## License

MIT OR Apache-2.0

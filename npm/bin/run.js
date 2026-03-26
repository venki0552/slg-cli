#!/usr/bin/env node
// slg npm proxy
// Downloads the correct platform binary from GitHub Releases on first run,
// verifies its SHA-256 checksum, caches it at ~/.slg/bin/slg, then execs
// all CLI arguments through to it.  Zero npm dependencies.
"use strict";

const { spawnSync } = require("child_process");
const https = require("https");
const http = require("http");
const fs = require("fs");
const path = require("path");
const os = require("os");
const crypto = require("crypto");

// ─── Constants ────────────────────────────────────────────────────────────────

const VERSION = "0.1.0";
const BASE_URL = `https://github.com/venki0552/slg/releases/download/v${VERSION}`;

/** Maps Node.js `process.platform-process.arch` to a release binary name. */
const BINARIES = {
	"linux-x64": "slg-linux-x86_64",
	"linux-arm64": "slg-linux-aarch64",
	"darwin-arm64": "slg-darwin-arm64",
	"darwin-x64": "slg-darwin-x86_64",
	"win32-x64": "slg-windows-x86_64.exe",
};

// ─── Path helpers ─────────────────────────────────────────────────────────────

const BIN_DIR = path.join(os.homedir(), ".slg", "bin");
const BIN_NAME = process.platform === "win32" ? "slg.exe" : "slg";
const BIN_PATH = path.join(BIN_DIR, BIN_NAME);

function platformKey() {
	return `${process.platform}-${process.arch}`;
}

// ─── Version check ────────────────────────────────────────────────────────────

/**
 * Returns true when the cached binary exists and reports the expected version.
 * Silent — any error means "not installed".
 */
function isCached() {
	if (!fs.existsSync(BIN_PATH)) return false;
	try {
		const out = spawnSync(BIN_PATH, ["--version"], {
			encoding: "utf8",
			timeout: 4000,
		});
		return Boolean(out.stdout && out.stdout.includes(VERSION));
	} catch {
		return false;
	}
}

// ─── Download helper ──────────────────────────────────────────────────────────

/**
 * Downloads `url` to `dest`, following up to 5 redirects.
 * Uses only Node.js built-in https/http — zero dependencies.
 */
function download(url, dest) {
	return new Promise((resolve, reject) => {
		fs.mkdirSync(path.dirname(dest), { recursive: true });

		let redirects = 0;
		const MAX_REDIRECTS = 5;

		function get(u) {
			if (redirects > MAX_REDIRECTS) {
				return reject(new Error("Too many redirects"));
			}
			const mod = u.startsWith("https://") ? https : http;
			const req = mod.get(u, (res) => {
				// Follow redirects (GitHub releases always redirect)
				if (
					res.statusCode === 301 ||
					res.statusCode === 302 ||
					res.statusCode === 303
				) {
					redirects++;
					res.resume(); // discard body
					return get(res.headers.location);
				}
				if (res.statusCode !== 200) {
					return reject(new Error(`HTTP ${res.statusCode} fetching ${u}`));
				}
				const file = fs.createWriteStream(dest);
				res.pipe(file);
				file.on("finish", () => {
					file.close();
					resolve();
				});
				file.on("error", (err) => {
					fs.unlink(dest, () => {});
					reject(err);
				});
			});
			req.setTimeout(60_000, () => {
				req.destroy();
				reject(new Error("Download timed out after 60s"));
			});
			req.on("error", reject);
		}

		get(url);
	});
}

// ─── Checksum verification ────────────────────────────────────────────────────

function sha256File(filePath) {
	const buf = fs.readFileSync(filePath);
	return crypto.createHash("sha256").update(buf).digest("hex").toLowerCase();
}

function verifyChecksum(binaryPath, checksumPath) {
	const line = fs.readFileSync(checksumPath, "utf8").trim();
	const expected = line.split(/\s+/)[0].toLowerCase();
	const actual = sha256File(binaryPath);
	return { ok: expected === actual, expected, actual };
}

// ─── Install ──────────────────────────────────────────────────────────────────

async function downloadAndInstall() {
	const key = platformKey();
	const binaryName = BINARIES[key];

	if (!binaryName) {
		console.error(`\nslg: unsupported platform "${key}"`);
		console.error(
			"Supported platforms: linux-x64, linux-arm64, darwin-arm64, darwin-x64, win32-x64",
		);
		console.error("Open an issue at https://github.com/venki0552/slg/issues");
		process.exit(1);
	}

	const binaryUrl = `${BASE_URL}/${binaryName}`;
	const checksumUrl = `${BASE_URL}/${binaryName}.sha256`;
	const tmpBin = BIN_PATH + ".download";
	const tmpSum = BIN_PATH + ".sha256.tmp";

	process.stderr.write(`\nslg: downloading v${VERSION} for ${key}...\n`);

	// Download binary + checksum file in sequence (checksum is tiny, order doesn't matter much)
	try {
		await download(binaryUrl, tmpBin);
		process.stderr.write(`slg: download complete, verifying checksum...\n`);
		await download(checksumUrl, tmpSum);
	} catch (err) {
		// Clean up partial files
		for (const f of [tmpBin, tmpSum]) {
			try {
				fs.unlinkSync(f);
			} catch {}
		}
		console.error(`\nslg: download failed — ${err.message}`);
		console.error("Check your internet connection and try again.");
		process.exit(1);
	}

	// Verify
	const { ok, expected, actual } = verifyChecksum(tmpBin, tmpSum);
	fs.unlinkSync(tmpSum);

	if (!ok) {
		fs.unlinkSync(tmpBin);
		console.error("\nslg: CHECKSUM VERIFICATION FAILED");
		console.error(`  expected: ${expected}`);
		console.error(`  actual:   ${actual}`);
		console.error("The downloaded binary may be corrupted or tampered with.");
		console.error(
			"Please open an issue at https://github.com/venki0552/slg/issues",
		);
		process.exit(1);
	}

	// Move into place
	fs.mkdirSync(BIN_DIR, { recursive: true });
	// On Windows, renameSync across drives may fail — copy + delete as fallback
	try {
		fs.renameSync(tmpBin, BIN_PATH);
	} catch {
		fs.copyFileSync(tmpBin, BIN_PATH);
		fs.unlinkSync(tmpBin);
	}

	if (process.platform !== "win32") {
		fs.chmodSync(BIN_PATH, 0o755);
	}

	process.stderr.write(`slg: installed to ${BIN_PATH}\n`);
}

// ─── PATH install helper ──────────────────────────────────────────────────────

function printPathInstructions() {
	const dir = BIN_DIR.replace(os.homedir(), "~");
	console.log(`\n✓ slg v${VERSION} installed to ${BIN_PATH}\n`);
	console.log("To use slg without npx, add it to your PATH:\n");

	if (process.platform === "win32") {
		console.log("  PowerShell (current session):");
		console.log(`    $env:PATH += ";${BIN_DIR}"\n`);
		console.log(
			"  Permanent (System Properties → Environment Variables → PATH)",
		);
	} else {
		const shell = process.env.SHELL || "";
		const rcFile = shell.includes("zsh")
			? "~/.zshrc"
			: shell.includes("fish")
				? "~/.config/fish/config.fish"
				: "~/.bashrc";
		console.log(`  Add to ${rcFile}:`);
		console.log(`    export PATH="${dir}:$PATH"\n`);
		console.log("  Then reload your shell:");
		console.log(`    source ${rcFile}`);
	}
	console.log("\nOr run slg directly at any time with:");
	console.log("  npx slg-cli <command>\n");
}

// ─── Main ─────────────────────────────────────────────────────────────────────

async function main() {
	const args = process.argv.slice(2);

	// `npx slg install` — download + print PATH setup instructions
	if (args[0] === "install" && args.length === 1) {
		await downloadAndInstall();
		printPathInstructions();
		return;
	}

	// All other commands — ensure binary is cached, then proxy through
	if (!isCached()) {
		await downloadAndInstall();
		process.stderr.write("\n");
	}

	const result = spawnSync(BIN_PATH, args, {
		stdio: "inherit",
		windowsHide: false,
	});

	// Forward the exact exit code (or 1 if the process was killed)
	process.exit(result.status ?? 1);
}

main().catch((err) => {
	console.error(`\nslg: unexpected error — ${err.message}`);
	process.exit(1);
});

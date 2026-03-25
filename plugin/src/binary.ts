import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import * as crypto from 'crypto';
import { execSync } from 'child_process';

const EXPECTED_VERSION = '0.1.0';
const GITHUB_RELEASE_BASE = 'https://github.com/lore-sh/lore/releases/download';

const BINARY_MAP: Record<string, string> = {
  'linux-x64': 'lore-linux-x86_64',
  'linux-arm64': 'lore-linux-aarch64',
  'darwin-arm64': 'lore-darwin-arm64',
  'darwin-x64': 'lore-darwin-x86_64',
  'win32-x64': 'lore-windows-x86_64.exe',
};

async function downloadFile(url: string, dest: string): Promise<void> {
  const https = await import('https');
  const http = await import('http');

  return new Promise((resolve, reject) => {
    const dir = path.dirname(dest);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }

    const file = fs.createWriteStream(dest);
    const protocol = url.startsWith('https') ? https : http;

    const request = protocol.get(url, (response) => {
      // Handle redirects
      if (response.statusCode === 301 || response.statusCode === 302) {
        const redirectUrl = response.headers.location;
        if (!redirectUrl) {
          reject(new Error('Redirect without location header'));
          return;
        }
        file.close();
        fs.unlinkSync(dest);
        downloadFile(redirectUrl, dest).then(resolve, reject);
        return;
      }

      if (response.statusCode !== 200) {
        reject(new Error(`Download failed with status ${response.statusCode}`));
        return;
      }

      response.pipe(file);
      file.on('finish', () => {
        file.close();
        resolve();
      });
    });

    request.on('error', (err) => {
      fs.unlink(dest, () => {});
      reject(err);
    });

    request.setTimeout(30000, () => {
      request.destroy();
      reject(new Error('Download timed out'));
    });
  });
}

async function verifyChecksum(binaryPath: string, checksumPath: string): Promise<boolean> {
  if (!fs.existsSync(checksumPath)) {
    return false;
  }

  const expectedLine = fs.readFileSync(checksumPath, 'utf8').trim();
  // Format: "<hash>  <filename>" or just "<hash>"
  const expectedHash = expectedLine.split(/\s+/)[0].toLowerCase();

  const fileBuffer = fs.readFileSync(binaryPath);
  const actualHash = crypto.createHash('sha256').update(fileBuffer).digest('hex').toLowerCase();

  return actualHash === expectedHash;
}

/// Download and verify the lore binary for the current platform
export async function ensureLoreBinary(ctx: vscode.ExtensionContext): Promise<string | null> {
  const storagePath = ctx.globalStorageUri.fsPath;
  const platformKey = `${process.platform}-${process.arch}`;
  const binaryName = BINARY_MAP[platformKey];

  if (!binaryName) {
    vscode.window.showErrorMessage(
      `lore: unsupported platform ${process.platform}-${process.arch}`
    );
    return null;
  }

  const binaryPath = path.join(storagePath, 'bin', binaryName);
  const checksumPath = binaryPath + '.sha256';

  // Check if current version already installed and verified
  if (fs.existsSync(binaryPath)) {
    try {
      const out = execSync(`"${binaryPath}" --version`, { timeout: 3000 }).toString().trim();
      if (out.includes(EXPECTED_VERSION) && await verifyChecksum(binaryPath, checksumPath)) {
        return binaryPath;
      }
    } catch {
      // Fall through to download
    }
  }

  // Download with progress notification
  return await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: 'lore: setting up for first time...',
      cancellable: false,
    },
    async (progress) => {
      progress.report({ message: `Downloading lore binary for ${platformKey}...` });

      const url = `${GITHUB_RELEASE_BASE}/v${EXPECTED_VERSION}/${binaryName}`;
      await downloadFile(url, binaryPath);

      // chmod 755 on non-Windows
      if (process.platform !== 'win32') {
        fs.chmodSync(binaryPath, '755');
      }

      progress.report({ message: 'Verifying download...' });
      const checksumUrl = `${url}.sha256`;
      await downloadFile(checksumUrl, checksumPath);

      // SECURITY: Always verify checksum — never trust a download without verification
      if (!(await verifyChecksum(binaryPath, checksumPath))) {
        fs.unlinkSync(binaryPath);
        throw new Error('lore binary checksum verification failed');
      }

      progress.report({ message: 'Initializing lore...' });
      execSync(`"${binaryPath}" init --mcp-only --silent`, { timeout: 10000 });

      return binaryPath;
    }
  );
}

import * as vscode from 'vscode';
import { exec, execSync } from 'child_process';
import { SlgStatusBar } from './statusbar';

function getCurrentBranch(workspaceRoot: string): string | undefined {
  try {
    return execSync('git rev-parse --abbrev-ref HEAD', {
      cwd: workspaceRoot,
      timeout: 3000,
    }).toString().trim();
  } catch {
    return undefined;
  }
}

function execBackground(command: string, cwd: string): void {
  exec(command, { cwd }, (error) => {
    if (error) {
      console.error(`slg background exec failed: ${error.message}`);
    }
  });
}

function pollIndexStatus(
  binaryPath: string,
  statusBar: SlgStatusBar,
  workspaceRoot: string
): void {
  const interval = setInterval(() => {
    exec(
      `"${binaryPath}" _health`,
      { cwd: workspaceRoot, timeout: 5000 },
      (error, stdout) => {
        if (error) { return; }

        try {
          const health = JSON.parse(stdout);
          if (health.indexed && health.branch) {
            statusBar.setState({
              kind: 'ready',
              branch: health.branch,
              sizeKB: health.size_kb || 0,
            });
            clearInterval(interval);
          }
        } catch {
          // Not ready yet — keep polling
        }
      }
    );
  }, 2000);

  // Stop polling after 5 minutes regardless
  setTimeout(() => clearInterval(interval), 5 * 60 * 1000);
}

/// Install file system watchers for branch changes and workspace folder changes
export function installWatchers(
  binaryPath: string,
  workspaceRoot: string | undefined,
  statusBar: SlgStatusBar
): vscode.Disposable[] {
  const disposables: vscode.Disposable[] = [];

  if (!workspaceRoot) {
    return disposables;
  }

  const root = workspaceRoot;

  // Watch .git/HEAD for branch changes
  const headWatcher = vscode.workspace.createFileSystemWatcher(
    new vscode.RelativePattern(root, '.git/HEAD')
  );

  headWatcher.onDidChange(async () => {
    const branch = getCurrentBranch(root);
    if (branch) {
      statusBar.setState({ kind: 'reindexing', branch });
      execBackground(
        `"${binaryPath}" reindex --delta-only --background --silent`,
        root
      );
      pollIndexStatus(binaryPath, statusBar, root);
    }
  });

  disposables.push(headWatcher);

  // Watch for new workspace folders (multi-root workspaces)
  const folderWatcher = vscode.workspace.onDidChangeWorkspaceFolders(async (event) => {
    for (const folder of event.added) {
      const folderPath = folder.uri.fsPath;
      const fs = await import('fs');
      const path = await import('path');
      if (fs.existsSync(path.join(folderPath, '.git'))) {
        execBackground(
          `"${binaryPath}" index --background --silent`,
          folderPath
        );
      }
    }
  });

  disposables.push(folderWatcher);

  return disposables;
}

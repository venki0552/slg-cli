import * as vscode from 'vscode';
import { exec } from 'child_process';
import { ensureSlgBinary } from './binary';
import { SlgStatusBar } from './statusbar';
import { installWatchers } from './watcher';
import { registerMCPWithAllAgents } from './mcp';
import { runDoctor } from './doctor';

function getConfig<T>(key: string): T | undefined {
  return vscode.workspace.getConfiguration('slg').get<T>(key);
}

function getWorkspaceRoot(): string | undefined {
  const folders = vscode.workspace.workspaceFolders;
  return folders?.[0]?.uri.fsPath;
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  // 1. Ensure binary (download if needed)
  const binaryPath = await ensureSlgBinary(context);
  if (!binaryPath) {
    vscode.window.showErrorMessage(
      'slg: failed to set up binary. Check your internet connection.'
    );
    return;
  }

  // 2. Status bar
  const statusBar = new SlgStatusBar(binaryPath);
  context.subscriptions.push(statusBar);

  // 3. Index current workspace (background)
  const workspaceRoot = getWorkspaceRoot();
  if (workspaceRoot && getConfig<boolean>('indexOnActivation')) {
    statusBar.setState({ kind: 'indexing' });
    exec(
      `"${binaryPath}" init --background --silent`,
      { cwd: workspaceRoot },
      (error) => {
        if (error) {
          console.error(`slg: background indexing failed: ${error.message}`);
        }
      }
    );
    statusBar.startHealthPolling(workspaceRoot);
  }

  // 4. Watch for branch changes
  const watchers = installWatchers(binaryPath, workspaceRoot, statusBar);
  context.subscriptions.push(...watchers);

  // 5. Register MCP with all agents
  if (getConfig<boolean>('autoRegisterMCP')) {
    const registered = await registerMCPWithAllAgents(binaryPath);
    if (registered.length > 0) {
      console.log(`slg: registered MCP with ${registered.join(', ')}`);
    }
  }

  // 6. Register commands
  context.subscriptions.push(
    vscode.commands.registerCommand('slg.doctor', () => runDoctor(binaryPath)),

    vscode.commands.registerCommand('slg.status', () => {
      const outputChannel = vscode.window.createOutputChannel('slg status');
      outputChannel.clear();
      outputChannel.show();
      exec(
        `"${binaryPath}" doctor`,
        { cwd: workspaceRoot || process.cwd(), timeout: 10000 },
        (_error, stdout) => {
          outputChannel.appendLine(stdout || 'No output');
        }
      );
    }),

    vscode.commands.registerCommand('slg.reindex', () => {
      if (!workspaceRoot) {
        vscode.window.showWarningMessage('slg: no workspace folder open');
        return;
      }
      statusBar.setState({ kind: 'indexing' });
      exec(
        `"${binaryPath}" init --background --silent`,
        { cwd: workspaceRoot },
        (error) => {
          if (error) {
            statusBar.setState({ kind: 'error', message: 'reindex failed' });
          }
        }
      );
    })
  );
}

export function deactivate(): void {
  // Watchers disposed via context.subscriptions
  // Do NOT kill background indexing — let it finish
}

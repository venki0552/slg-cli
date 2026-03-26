import * as vscode from 'vscode';
import { exec } from 'child_process';

type SlgState =
  | { kind: 'indexing'; progress?: number }
  | { kind: 'reindexing'; branch: string }
  | { kind: 'ready'; branch: string; sizeKB: number }
  | { kind: 'error'; message: string }
  | { kind: 'mcp_down' }
  | { kind: 'no_index' };

export class SlgStatusBar implements vscode.Disposable {
  private item: vscode.StatusBarItem;
  private binaryPath: string;
  private pollTimer: NodeJS.Timeout | undefined;

  constructor(binaryPath: string) {
    this.binaryPath = binaryPath;
    this.item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 50);
    this.item.command = 'slg.status';
    this.setState({ kind: 'no_index' });
    this.item.show();
  }

  setState(state: SlgState): void {
    switch (state.kind) {
      case 'indexing':
        this.item.text = state.progress !== undefined
          ? `$(sync~spin) slg: indexing ${state.progress}%`
          : '$(sync~spin) slg: indexing...';
        this.item.tooltip = 'slg is indexing this repository';
        break;
      case 'reindexing':
        this.item.text = `$(sync~spin) slg: ↻ ${state.branch}`;
        this.item.tooltip = `slg is reindexing branch ${state.branch}`;
        break;
      case 'ready': {
        const sizeMB = (state.sizeKB / 1024).toFixed(1);
        this.item.text = `$(check) slg: ${state.branch} ✓ ${sizeMB}MB`;
        this.item.tooltip = `slg index ready — ${state.branch} (${sizeMB}MB)`;
        break;
      }
      case 'error':
        this.item.text = `$(warning) slg: ⚠ ${state.message}`;
        this.item.tooltip = `slg error: ${state.message}`;
        break;
      case 'mcp_down':
        this.item.text = '$(error) slg: MCP ✗';
        this.item.tooltip = 'slg MCP server is not responding';
        break;
      case 'no_index':
        this.item.text = '$(circle-slash) slg: not indexed';
        this.item.tooltip = 'No slg index found — run "slg init"';
        break;
    }
  }

  /// Poll slg _health every 30 seconds
  startHealthPolling(workspaceRoot: string | undefined): void {
    if (this.pollTimer) {
      clearInterval(this.pollTimer);
    }

    this.pollTimer = setInterval(() => {
      if (!workspaceRoot) { return; }

      exec(
        `"${this.binaryPath}" _health`,
        { cwd: workspaceRoot, timeout: 5000 },
        (error, stdout) => {
          if (error) {
            this.setState({ kind: 'error', message: 'health check failed' });
            return;
          }

          try {
            const health = JSON.parse(stdout);
            if (health.indexed && health.branch) {
              this.setState({
                kind: 'ready',
                branch: health.branch,
                sizeKB: health.size_kb || 0,
              });
            } else {
              this.setState({ kind: 'no_index' });
            }
          } catch {
            // Non-JSON output — binary might not support _health yet
          }
        }
      );
    }, 30000);
  }

  dispose(): void {
    if (this.pollTimer) {
      clearInterval(this.pollTimer);
    }
    this.item.dispose();
  }
}

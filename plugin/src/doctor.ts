import * as vscode from 'vscode';
import { execSync } from 'child_process';

/// Run lore doctor and show results in an output channel
export function runDoctor(binaryPath: string): void {
  const outputChannel = vscode.window.createOutputChannel('lore doctor');
  outputChannel.clear();
  outputChannel.show();

  const workspaceRoot = getWorkspaceRoot();
  const cwd = workspaceRoot || process.cwd();

  try {
    const output = execSync(`"${binaryPath}" doctor`, {
      cwd,
      timeout: 15000,
    }).toString();
    outputChannel.appendLine(output);
  } catch (error: unknown) {
    if (error && typeof error === 'object' && 'stdout' in error) {
      // execSync throws on non-zero exit, but stdout still has output
      outputChannel.appendLine((error as { stdout: Buffer }).stdout.toString());
    } else {
      outputChannel.appendLine(`Failed to run lore doctor: ${error}`);
    }
  }
}

function getWorkspaceRoot(): string | undefined {
  const folders = vscode.workspace.workspaceFolders;
  return folders?.[0]?.uri.fsPath;
}

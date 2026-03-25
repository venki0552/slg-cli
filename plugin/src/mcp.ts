import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

interface AgentConfig {
  name: string;
  configPath: string;
  key: string;
}

const AGENT_CONFIGS: AgentConfig[] = [
  { name: 'Claude Code', configPath: '~/.claude/claude_desktop_config.json', key: 'mcpServers' },
  { name: 'Cursor', configPath: '~/.cursor/mcp.json', key: 'mcpServers' },
  { name: 'Windsurf', configPath: '~/.windsurf/mcp.json', key: 'mcpServers' },
];

const LORE_MCP_ENTRY = {
  command: 'lore',
  args: ['mcp', 'start'],
};

function expandHome(filepath: string): string {
  if (filepath.startsWith('~')) {
    return path.join(os.homedir(), filepath.slice(1));
  }
  return filepath;
}

/// Register lore as an MCP server with all detected AI agents
export async function registerMCPWithAllAgents(binaryPath: string): Promise<string[]> {
  const registered: string[] = [];

  for (const agent of AGENT_CONFIGS) {
    const configPath = expandHome(agent.configPath);
    const configDir = path.dirname(configPath);

    // Only register if the agent's config directory exists
    if (!fs.existsSync(configDir)) {
      continue;
    }

    try {
      let existing: Record<string, unknown> = {};
      if (fs.existsSync(configPath)) {
        const content = fs.readFileSync(configPath, 'utf8');
        existing = JSON.parse(content);
      }

      // Add lore entry under mcpServers, preserving existing entries
      const servers = (existing[agent.key] as Record<string, unknown>) || {};
      servers['lore'] = {
        ...LORE_MCP_ENTRY,
        command: binaryPath,
      };
      existing[agent.key] = servers;

      fs.writeFileSync(configPath, JSON.stringify(existing, null, 2));
      registered.push(agent.name);
      console.log(`lore: registered MCP with ${agent.name}`);
    } catch (e) {
      console.error(`lore: failed to register MCP with ${agent.name}:`, e);
    }
  }

  return registered;
}

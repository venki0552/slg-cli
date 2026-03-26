import { defineConfig } from 'vitepress'
import mathjax3 from 'markdown-it-mathjax3'

export default defineConfig({
  title: 'lore',
  description: 'Semantic git intelligence for LLM agents — transform your git history into a queryable knowledge base.',

  // All .md files are read from the repo root
  srcExclude: ['lore-architecture.md', 'lore-claude-code-prompt.md', 'NUL'],

  head: [
    ['meta', { name: 'theme-color', content: '#646cff' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:title', content: 'lore — Semantic git intelligence' }],
    ['meta', { property: 'og:description', content: 'Transform your git history into a queryable knowledge base for LLM agents.' }],
  ],

  markdown: {
    config: (md) => {
      md.use(mathjax3)
    },
    lineNumbers: false,
  },

  themeConfig: {
    logo: { light: '/logo-light.svg', dark: '/logo-dark.svg', alt: 'lore' },

    nav: [
      { text: 'Get Started', link: '/docs/getting-started' },
      { text: 'Commands', link: '/docs/commands' },
      { text: 'MCP', link: '/docs/mcp' },
      { text: 'Architecture', link: '/docs/architecture' },
      {
        text: 'v0.1.0',
        items: [
          { text: 'Changelog', link: 'https://github.com/venki0552/lore-cli/releases' },
          { text: 'Contributing', link: '/CONTRIBUTING' },
        ],
      },
    ],

    sidebar: [
      {
        text: 'Getting Started',
        items: [
          { text: 'Introduction', link: '/' },
          { text: 'Installation & Setup', link: '/docs/getting-started' },
          { text: 'Commands', link: '/docs/commands' },
          { text: 'Configuration', link: '/docs/configuration' },
        ],
      },
      {
        text: 'Integrations',
        items: [
          { text: 'MCP Integration', link: '/docs/mcp' },
          { text: 'VS Code Extension', link: '/docs/vscode-extension' },
        ],
      },
      {
        text: 'Internals',
        items: [
          { text: 'Architecture', link: '/docs/architecture' },
          { text: 'Security Design', link: '/docs/security' },
        ],
      },
      {
        text: 'Project',
        items: [
          { text: 'Contributing', link: '/CONTRIBUTING' },
          { text: 'GitHub Releases', link: 'https://github.com/venki0552/lore-cli/releases' },
        ],
      },
    ],

    socialLinks: [
      { icon: 'github', link: 'https://github.com/venki0552/lore-cli' },
    ],

    editLink: {
      pattern: 'https://github.com/venki0552/lore-cli/edit/main/:path',
      text: 'Edit this page on GitHub',
    },

    footer: {
      message: 'Released under the MIT OR Apache-2.0 License.',
      copyright: 'Copyright © 2024–2026 lore contributors',
    },

    search: {
      provider: 'local',
    },
  },
})

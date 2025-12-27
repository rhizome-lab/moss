import { defineConfig } from 'vitepress'
import { withMermaid } from 'vitepress-plugin-mermaid'

export default withMermaid(
  defineConfig({
  title: 'Moss',
  description: 'Code intelligence CLI with structural awareness',

  base: '/moss/',

  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/moss/logo.svg' }],
  ],

  themeConfig: {
    logo: '/logo.svg',

    nav: [
      { text: 'Guide', link: '/getting-started/installation' },
      { text: 'CLI Reference', link: '/cli/commands' },
      { text: 'Architecture', link: '/architecture/overview' },
    ],

    sidebar: {
      '/': [
        {
          text: 'Getting Started',
          items: [
            { text: 'Installation', link: '/getting-started/installation' },
            { text: 'Quickstart', link: '/getting-started/quickstart' },
            { text: 'MCP Integration', link: '/getting-started/mcp-integration' },
          ]
        },
        {
          text: 'CLI Reference',
          items: [
            { text: 'Commands', link: '/cli/commands' },
            { text: 'Tools', link: '/tools' },
          ]
        },
        {
          text: 'Architecture',
          items: [
            { text: 'Overview', link: '/architecture/overview' },
            { text: 'Events', link: '/architecture/events' },
            { text: 'Plugins', link: '/architecture/plugins' },
            { text: 'CLI Architecture', link: '/cli-architecture' },
            { text: 'DWIM Architecture', link: '/dwim-architecture' },
            { text: 'Rust/Python Boundary', link: '/rust-python-boundary' },
            { text: 'API Boundaries', link: '/api-boundaries' },
          ]
        },
        {
          text: 'Synthesis',
          items: [
            { text: 'Overview', link: '/synthesis/overview' },
            { text: 'Strategies', link: '/synthesis/strategies' },
            { text: 'Generators', link: '/synthesis/generators' },
          ]
        },
        {
          text: 'Design',
          items: [
            { text: 'Philosophy', link: '/philosophy' },
            { text: 'Primitives Spec', link: '/primitives-spec' },
            { text: 'Documentation Strategy', link: '/documentation' },
            { text: 'Language Support', link: '/language-support' },
            { text: 'Architecture Decisions', link: '/architecture-decisions' },
            { text: 'Unification', link: '/unification' },
          ]
        },
        {
          text: 'Internals',
          collapsed: true,
          items: [
            { text: 'TUI Design', link: '/tui-design' },
            { text: 'TUI Notes', link: '/tui' },
            { text: 'Session Modes', link: '/session-modes' },
            { text: 'Dogfooding', link: '/dogfooding' },
            { text: 'Async Tasks', link: '/async-tasks' },
            { text: 'Nested Execution', link: '/nested-execution' },
            { text: 'Memory System', link: '/memory-system' },
            { text: 'Workflow Format', link: '/workflow-format' },
            { text: 'View Filtering', link: '/view-filtering' },
            { text: 'File Boundaries', link: '/file-boundaries' },
            { text: 'Codebase Tree', link: '/codebase-tree' },
          ]
        },
        {
          text: 'Research',
          collapsed: true,
          items: [
            { text: 'Spec', link: '/spec' },
            { text: 'LLM Evaluation', link: '/llm-evaluation' },
            { text: 'LLM Comparison', link: '/llm-comparison' },
            { text: 'LangGraph Evaluation', link: '/langgraph-evaluation' },
            { text: 'LLM Code Consistency', link: '/llm-code-consistency' },
            { text: 'Edit Paradigm Comparison', link: '/edit-paradigm-comparison' },
            { text: 'Prior Art', link: '/prior-art' },
            { text: 'Ampcode', link: '/research/ampcode' },
            { text: 'Log Analysis', link: '/log-analysis' },
            { text: 'Recursive Improvement', link: '/recursive-improvement' },
            { text: 'Restructuring Plan', link: '/restructuring-plan' },
          ]
        },
      ]
    },

    socialLinks: [
      { icon: 'github', link: 'https://github.com/pterror/moss' }
    ],

    search: {
      provider: 'local'
    },

    editLink: {
      pattern: 'https://github.com/pterror/moss/edit/master/docs/:path',
      text: 'Edit this page on GitHub'
    },
  },

}),
)

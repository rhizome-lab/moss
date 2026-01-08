import { defineConfig } from 'vitepress'
import { withMermaid } from 'vitepress-plugin-mermaid'

export default withMermaid(
  defineConfig({
  vite: {
    optimizeDeps: {
      include: ['mermaid'],
    },
  },

  markdown: {
    languageAlias: {
      scm: 'scheme',
    },
  },

  title: 'Moss',
  description: 'Code intelligence CLI with structural awareness',

  base: '/moss/',

  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/moss/logo.svg' }],
  ],

  themeConfig: {
    logo: '/logo.svg',

    nav: [
      { text: 'Guide', link: '/introduction' },
      { text: 'CLI Reference', link: '/cli/commands' },
      { text: 'Design', link: '/philosophy' },
    ],

    sidebar: {
      '/': [
        {
          text: 'Guide',
          items: [
            { text: 'Introduction', link: '/introduction' },
            { text: 'Primitives Spec', link: '/primitives-spec' },
            { text: 'Lua API', link: '/lua-api' },
            { text: 'Workflow Format', link: '/workflow-format' },
            { text: 'Language Support', link: '/language-support' },
          ]
        },
        {
          text: 'CLI Reference',
          items: [
            { text: 'Commands', link: '/cli/commands' },
            { text: 'view', link: '/cli/view' },
            { text: 'edit', link: '/cli/edit' },
            { text: 'analyze', link: '/cli/analyze' },
            { text: 'text-search', link: '/cli/text-search' },
            { text: 'sessions', link: '/cli/sessions' },
            { text: 'Tools', link: '/tools' },
          ]
        },
        {
          text: 'Design',
          items: [
            { text: 'Philosophy', link: '/philosophy' },
            { text: 'Architecture Decisions', link: '/architecture-decisions' },
            { text: 'Unification', link: '/unification' },
            { text: 'View Filtering', link: '/view-filtering' },
            { text: 'Agent', link: '/design/agent' },
            { text: 'Shadow Git', link: '/design/shadow-git' },
          ]
        },
        {
          text: 'Development',
          collapsed: true,
          items: [
            { text: 'Dogfooding', link: '/dogfooding' },
            { text: 'Agent Commands', link: '/agent-commands' },
            { text: 'Session Modes', link: '/session-modes' },
            { text: 'Documentation Strategy', link: '/documentation' },
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
            { text: 'Low-Priority Research', link: '/research-low-priority' },
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

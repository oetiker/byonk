import { defineConfig } from 'rspress/config';

export default defineConfig({
  root: 'content',
  base: '/byonk/',
  title: 'Byonk',
  description: 'Bring Your Own Ink - Self-hosted content server for TRMNL e-ink devices',
  icon: '/icon.svg',
  logo: '/logo.svg',
  logoText: 'Byonk',

  // Default language - removes /en/ from URLs
  lang: 'en',
  locales: [
    {
      lang: 'en',
      label: 'English',
    },
  ],

  themeConfig: {
    socialLinks: [
      {
        icon: 'github',
        mode: 'link',
        content: 'https://github.com/oetiker/byonk',
      },
    ],
    footer: {
      message: 'MIT License | Byonk - Bring Your Own Ink',
    },
    editLink: {
      docRepoBaseUrl: 'https://github.com/oetiker/byonk/tree/main/docs/content/en',
      text: 'Edit this page on GitHub',
    },
  },

  markdown: {
    showLineNumbers: true,
  },
});

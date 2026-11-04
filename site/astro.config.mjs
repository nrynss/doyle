import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const base = process.env.SITE_BASE ?? '/doyle';
const origin = process.env.SITE_ORIGIN ?? 'https://nrynss.github.io';

export default defineConfig({
  site: origin,
  base,
  integrations: [
    starlight({
      title: 'Doyle',
      description: 'Truth maintenance for agent memory.',
      favicon: '/favicon.svg',
      customCss: ['./src/styles/doyle.css'],
      logo: { src: './src/assets/doyle-mark.svg', alt: 'Doyle mark' },
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/nrynss/doyle' }],
      editLink: { baseUrl: 'https://github.com/nrynss/doyle/edit/main/site/' },
      head: [{ tag: 'meta', attrs: { name: 'theme-color', content: '#111111' } }],
      sidebar: [
        {
          label: 'Start here',
          items: [
            { label: 'Overview', slug: 'index' },
            { label: 'Judge walkthrough', slug: 'judge-walkthrough' },
            { label: 'The demo video', slug: 'demo-video' },
          ],
        },
        {
          label: 'How it works',
          items: [
            { label: 'Architecture', slug: 'architecture' },
            { label: 'Evidence sources', slug: 'evidence-sources' },
            { label: 'The guard', slug: 'guard' },
            { label: 'The review page', slug: 'review-page' },
            { label: 'Meta models, one job each', slug: 'models' },
          ],
        },
        {
          label: 'Run it yourself',
          items: [
            { label: 'Run on your own project', slug: 'run-on-your-project' },
            { label: 'Commands', slug: 'commands' },
            { label: 'Adapters', slug: 'adapters' },
          ],
        },
        {
          label: 'Trust and evidence',
          items: [
            { label: 'Evidence and measured numbers', slug: 'evidence' },
            { label: 'Safety and limits', slug: 'safety-and-limits' },
            { label: 'Built during the event', slug: 'built-during-the-event' },
            { label: "What's next", slug: 'whats-next' },
          ],
        },
      ],
    }),
  ],
});

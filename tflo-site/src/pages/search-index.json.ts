import { getCollection } from 'astro:content';

export async function GET() {
  const allPosts = await getCollection('blog', ({ data }) => !data.draft);
  allPosts.sort((a, b) => b.data.pubDate.valueOf() - a.data.pubDate.valueOf());

  const index = allPosts.map(post => ({
    title: post.data.title,
    description: post.data.description,
    tags: post.data.tags,
    slug: `/blog/${post.slug}`,
    pubDate: post.data.pubDate.toISOString().slice(0, 10),
  }));

  const docs = [
    { title: 'Quick Start', description: 'Get up and running with tflo in minutes.', slug: '/docs/quick-start', tags: ['docs'], pubDate: '' },
    { title: 'Core Concepts', description: 'Understanding computation graphs, windows, and pipelines.', slug: '/docs/concepts', tags: ['docs'], pubDate: '' },
    { title: 'Indicators', description: 'Full reference of available technical analysis indicators.', slug: '/docs/indicators', tags: ['docs'], pubDate: '' },
    { title: 'Signals', description: 'Signal detection and domain event generation.', slug: '/docs/signals', tags: ['docs'], pubDate: '' },
    { title: 'Crate Architecture', description: 'Architecture overview of the tflo crate family.', slug: '/docs/architecture', tags: ['docs'], pubDate: '' },
    { title: 'Use Cases', description: 'Real-world use cases for streaming technical analysis.', slug: '/use-cases', tags: ['docs'], pubDate: '' },
    { title: 'Docs Home', description: 'tflo documentation index.', slug: '/docs', tags: ['docs'], pubDate: '' },
  ];

  const body = JSON.stringify([...index, ...docs]);

  return new Response(body, {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });
}

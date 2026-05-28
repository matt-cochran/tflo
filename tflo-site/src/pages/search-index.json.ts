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
    { title: 'Core Concepts', description: 'Computation graphs, windows, timers, typed Absent, ShardRouter.', slug: '/docs/concepts', tags: ['docs'], pubDate: '' },
    { title: 'Indicators', description: 'Full reference of available technical analysis indicators.', slug: '/docs/indicators', tags: ['docs'], pubDate: '' },
    { title: 'Signals', description: 'Signal detection and domain event generation.', slug: '/docs/signals', tags: ['docs'], pubDate: '' },
    { title: 'Crate Architecture', description: 'Architecture overview of the tflo crate family.', slug: '/docs/architecture', tags: ['docs'], pubDate: '' },
    { title: 'Advanced', description: 'Checkpointing, async contracts, Deduplicator, Metrics, scripting.', slug: '/docs/advanced', tags: ['docs'], pubDate: '' },
    { title: 'WebAssembly', description: 'Run tflo computations in the browser via wasm-bindgen.', slug: '/docs/wasm', tags: ['docs'], pubDate: '' },
    { title: 'Deployment shapes', description: 'Six concrete shapes tflo covers, with production caveats.', slug: '/docs/deployment-shapes', tags: ['docs'], pubDate: '' },
    { title: 'Contracts', description: 'The four pluggable traits: AsyncStateStore, Cursor, ShardRouter, Operator.', slug: '/docs/contracts', tags: ['docs'], pubDate: '' },
    { title: 'Reference deployment', description: 'End-to-end iot-portal: MQTT → tflo → Kafka → tflo → Influx + Parquet.', slug: '/docs/reference-deployment', tags: ['docs'], pubDate: '' },
    { title: 'Non-goals', description: 'What tflo deliberately does not do, and why.', slug: '/docs/non-goals', tags: ['docs'], pubDate: '' },
    { title: 'Interop backlog', description: 'Designed integrations with Flink, Beam, Kafka Streams (deferred).', slug: '/docs/interop-backlog', tags: ['docs'], pubDate: '' },
    { title: 'Positioning', description: 'Where tflo fits next to Flink, Esper, and Kafka Streams.', slug: '/positioning', tags: ['positioning'], pubDate: '' },
    { title: 'Release notes', description: "What's new in tflo: hardening pass through Phase 6.", slug: '/release-notes', tags: ['release'], pubDate: '' },
    { title: 'Use Cases', description: 'Real-world use cases for streaming temporal analysis.', slug: '/use-cases', tags: ['docs'], pubDate: '' },
    { title: 'Docs Home', description: 'tflo documentation index.', slug: '/docs', tags: ['docs'], pubDate: '' },
  ];

  const body = JSON.stringify([...index, ...docs]);

  return new Response(body, {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });
}

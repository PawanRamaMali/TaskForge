// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
import { dev } from '$app/environment';
import type { LayoutLoad } from './$types';

export const ssr = false;

// Development-only demo mode (?demo): runs the UI in a normal browser with fake
// data, e.g. for README screenshots. ?view=mini shows the mini view.
export const load: LayoutLoad = async ({ url }) => {
  if (dev && url.searchParams.has('demo')) {
    const { installDemo } = await import('$lib/demo');
    installDemo(url.searchParams.get('view'));
  }
};

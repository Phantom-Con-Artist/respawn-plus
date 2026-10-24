// SPDX-License-Identifier: MIT
// The site lives at respawnplus.mavern.in. Requests to the production pages.dev host are sent
// there permanently, keeping the path and query; preview deployments (<hash>.respawnplus.pages.dev)
// are left alone so they can still be checked before release.
const CANONICAL = 'respawnplus.mavern.in';
const PAGES_HOST = 'respawnplus.pages.dev';

export async function onRequest({ request, next }) {
  const url = new URL(request.url);
  if (url.hostname === PAGES_HOST) {
    url.hostname = CANONICAL;
    return Response.redirect(url.toString(), 301);
  }
  return next();
}

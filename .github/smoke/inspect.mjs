// TEMPORARY: inspects DeTube's YouTube tab over WebView2's DevTools protocol.
const sleep = ms => new Promise(r => setTimeout(r, ms));
let targets = [];
for (let i = 0; i < 30 && !targets.some(t => t.url.includes('youtube.com')); i++) {
  try { targets = await (await fetch('http://127.0.0.1:9222/json')).json(); } catch (e) {}
  await sleep(1000);
}
console.log('targets:', targets.map(t => `${t.type} ${t.url}`).join(' | '));
const tab = targets.find(t => t.type === 'page' && t.url.includes('youtube.com'));
if (!tab) { console.log('no YouTube target'); process.exit(0); }
const ws = new WebSocket(tab.webSocketDebuggerUrl);
await new Promise(r => ws.addEventListener('open', r));
let id = 0; const pending = new Map();
ws.addEventListener('message', e => { const m = JSON.parse(e.data); if (pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } });
const send = (method, params = {}) => new Promise(r => { pending.set(++id, r); ws.send(JSON.stringify({ id, method, params })); });
const evaluate = async expr => (await send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true })).result?.result?.value;
const shot = async name => { const r = await send('Page.captureScreenshot', { format: 'png' }); (await import('node:fs')).writeFileSync(name, Buffer.from(r.result.data, 'base64')); };
const go = async url => { await evaluate(`location.href = ${JSON.stringify(url)}`); await sleep(9000); };

const PROBE = `(() => {
  const vis = el => !!(el.offsetWidth || el.offsetHeight || el.getClientRects().length);
  return {
    url: location.href, ua: navigator.userAgent,
    mt2: !!window.__mt2, ads: !!window.__mt2Ads, settings: window.__mt2Settings || null,
    hideShortsAttr: document.documentElement.hasAttribute('mt2-hide-shorts'),
    visibleShortsLinks: [...document.querySelectorAll('a[href^="/shorts/"]')].filter(vis).length,
    shortsGuide: [...document.querySelectorAll('ytd-guide-entry-renderer, ytd-mini-guide-entry-renderer')].filter(vis).some(e => /shorts/i.test(e.innerText)),
    blockButton: !!document.getElementById('mt2-block-channel'),
    actionsMenu: !!document.querySelector('ytd-watch-metadata #actions ytd-menu-renderer'),
    owner: document.querySelector('ytd-watch-metadata #owner a[href^="/@"], ytd-watch-metadata #owner a[href^="/channel/"]')?.getAttribute('href') || null,
    sponsored: [...document.querySelectorAll('*')].filter(e => e.children.length === 0 && /^\\s*Sponsored\\s*$/.test(e.textContent) && vis(e)).length,
    adSlots: [...document.querySelectorAll('ytd-ad-slot-renderer, ad-slot-renderer, ytd-in-feed-ad-layout-renderer, ytd-search-pyv-renderer, feed-ad-metadata-view-model')].filter(vis).map(e => e.tagName.toLowerCase()),
  };
})()`;

await send('Runtime.enable'); await send('Page.enable');
console.log('home', JSON.stringify(await evaluate(PROBE)));
await shot('cdp-home.png');
// Turn Shorts and AI off the way the app does, then look again.
await evaluate(`window.__mt2 && window.__mt2.update({ showShorts: false, showAI: false, rev: 1000 })`);
await go('https://www.youtube.com/results?search_query=funny+cats');
console.log('search (shorts off)', JSON.stringify(await evaluate(PROBE)));
await shot('cdp-search.png');
await go('https://www.youtube.com/results?search_query=vpn');
console.log('search vpn', JSON.stringify(await evaluate(PROBE)));
await go('https://www.youtube.com/watch?v=jNQXAC9IVRw');
console.log('watch', JSON.stringify(await evaluate(PROBE)));
await shot('cdp-watch.png');
ws.close();

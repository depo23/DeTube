// TEMPORARY: page state checked by the in-app probe (DETUBE_PROBE).
(() => {
  const vis = el => !!(el.offsetWidth || el.offsetHeight || el.getClientRects().length);
  const chat = document.querySelector('#chatframe');
  let chatText = null;
  try { chatText = chat && chat.contentDocument && chat.contentDocument.body ? chat.contentDocument.body.innerText.slice(0, 160) : null; } catch (e) { chatText = 'unreadable: ' + e.message; }
  return {
    url: location.href, ua: navigator.userAgent,
    mt2: !!window.__mt2, ads: !!window.__mt2Ads,
    hideShortsAttr: document.documentElement.hasAttribute('mt2-hide-shorts'),
    visibleShortsLinks: [...document.querySelectorAll('a[href^="/shorts/"]')].filter(vis).length,
    shortsGuide: [...document.querySelectorAll('ytd-guide-entry-renderer, ytd-mini-guide-entry-renderer')].filter(vis).some(e => /shorts/i.test(e.innerText)),
    blockButton: !!document.getElementById('mt2-block-channel'),
    owner: document.querySelector('ytd-watch-metadata #owner a[href^="/@"], ytd-watch-metadata #owner a[href^="/channel/"]')?.getAttribute('href') || null,
    sponsored: [...document.querySelectorAll('*')].filter(e => e.children.length === 0 && /^\s*Sponsored\s*$/.test(e.textContent) && vis(e)).length,
    adSlots: [...document.querySelectorAll('ytd-ad-slot-renderer, ad-slot-renderer, ytd-in-feed-ad-layout-renderer, ytd-search-pyv-renderer, feed-ad-metadata-view-model')].filter(vis).length,
    chatFrame: chat ? chat.src : null,
    chatText,
  };
})()

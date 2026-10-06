// Ad blocking inside the page (ported from MacTube 2). The app also blocks ad servers and
// YouTube's ad endpoints at the network level; see ad_rules in main.rs.
(() => {
  if (window.__mt2Ads) return;
  window.__mt2Ads = true;

  // 1. Strip ads from the player's data before the player reads it (as uBlock Origin's json-prune does),
  //    both for the first page (an inline ytInitialPlayerResponse) and for in-app navigation (fetched JSON).
  const AD_KEYS = ['adPlacements', 'adSlots', 'playerAds', 'adBreakHeartbeatParams'];
  function prune(data) {
    for (const o of [data, data && data.playerResponse]) {
      if (o && typeof o === 'object') for (const k of AD_KEYS) if (k in o) delete o[k];
    }
    return data;
  }
  let initial;
  Object.defineProperty(window, 'ytInitialPlayerResponse', {
    configurable: true,
    get() { return initial; },
    set(v) { initial = prune(v); },
  });
  const parse = JSON.parse;
  JSON.parse = function () { return prune(parse.apply(this, arguments)); };
  const json = Response.prototype.json;
  Response.prototype.json = function () { return json.apply(this, arguments).then(prune); };

  // 2. Hide ad slots in feeds, search and on watch pages. One rule per selector so an
  //    unsupported one can't invalidate the rest.
  const HIDE = [
    'ytd-ad-slot-renderer',
    'ad-slot-renderer',
    'ytd-rich-item-renderer:has(ytd-ad-slot-renderer)',
    'ytd-rich-item-renderer:has(ad-slot-renderer)',
    'ytd-rich-item-renderer:has(feed-ad-metadata-view-model)',
    'ytd-in-feed-ad-layout-renderer',
    'ytd-display-ad-renderer',
    'ytd-promoted-sparkles-web-renderer',
    'ytd-promoted-video-renderer',
    'ytd-search-pyv-renderer',
    'ytd-banner-promo-renderer',
    'ytd-statement-banner-renderer',
    'ytd-rich-section-renderer:has(ytd-statement-banner-renderer)',
    'ytd-brand-video-singleton-renderer',
    'ytd-brand-video-shelf-renderer',
    'ytd-companion-slot-renderer',
    'ytd-player-legacy-desktop-watch-ads-renderer',
    'ytd-engagement-panel-section-list-renderer[target-id="engagement-panel-ads"]',
    'ytd-merch-shelf-renderer',
    '#masthead-ad',
    '#player-ads',
    '.ytp-ad-overlay-container',
    '.ytp-featured-product',
    // YouTube's "Ad blockers are not allowed" wall.
    'tp-yt-paper-dialog:has(ytd-enforcement-message-view-model)',
    'ytd-enforcement-message-view-model',
    'html:has(ytd-enforcement-message-view-model) tp-yt-iron-overlay-backdrop',
  ];
  const style = document.createElement('style');
  style.textContent = HIDE.map(s => `${s} { display: none !important; }`).join('\n');
  // WebView2 (Windows) runs this before <html> exists.
  if (document.documentElement) document.documentElement.appendChild(style);
  else new MutationObserver((_, observer) => {
    if (!document.documentElement) return;
    observer.disconnect();
    document.documentElement.appendChild(style);
  }).observe(document, { childList: true });

  // 3. Any ad that still plays: mute it, press Skip as soon as it exists, and jump to its end.
  //    The wall above pauses the video; resume it once.
  let mutedByUs = false;
  let wallSeen = false;
  setInterval(() => {
    const player = document.getElementById('movie_player');
    const video = player && player.querySelector('video');
    if (!video) return;
    if (player.classList.contains('ad-showing')) {
      if (!video.muted) { video.muted = true; mutedByUs = true; }
      const skip = player.querySelector('.ytp-skip-ad-button, .ytp-ad-skip-button, .ytp-ad-skip-button-modern');
      if (skip) skip.click();
      if (isFinite(video.duration) && video.duration > 0) video.currentTime = video.duration;
    } else if (mutedByUs) {
      video.muted = false;
      mutedByUs = false;
    }
    const wall = !!document.querySelector('ytd-enforcement-message-view-model');
    if (wall && !wallSeen && video.paused) video.play();
    wallSeen = wall;
  }, 250);
})();

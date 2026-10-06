(() => {
  if (window.__mt2) return;

  // Viewer-facing text of YouTube's AI disclosure (pages are fetched in English via hl=en).
  const AI_MARKERS = [
    // Only the unique subtitles/headings are used: "Made with AI" alone also shows up in video titles,
    // and "How this (content) was made" also wraps non-AI disclosures such as C2PA "Captured with a camera".
    'Sounds or visuals were altered or fully generated',          // "Made with AI" label, May 2026+ (long-form + Shorts)
    'Sounds or visuals were significantly edited or digitally generated', // earlier Shorts subtitle
    'Altered or synthetic content',                               // earlier heading (2024-26)
  ];
  const SHORTS_SELECTORS = [
    'ytd-reel-shelf-renderer',
    'ytd-rich-shelf-renderer[is-shorts]',
    'ytd-rich-section-renderer:has(ytd-rich-shelf-renderer[is-shorts])',
    'grid-shelf-view-model:has(a[href^="/shorts/"])',
    'ytd-rich-item-renderer:has(a[href^="/shorts/"])',
    'ytd-video-renderer:has(a[href^="/shorts/"])',
    'ytd-compact-video-renderer:has(a[href^="/shorts/"])',
    'ytd-guide-entry-renderer:has(a[title="Shorts"])',
    'ytd-mini-guide-entry-renderer[aria-label="Shorts"]',
    // Collapsed sidebar: the label moved onto the link, so match its target (any language).
    'ytd-mini-guide-entry-renderer:has(a[href="/shorts/"])',
    'yt-tab-shape[tab-title="Shorts"]',
  ];
  const ITEMS = 'ytd-rich-item-renderer, ytd-video-renderer, ytd-compact-video-renderer, ytd-grid-video-renderer, yt-lockup-view-model';
  const KEY = 'mt2.aiChannels';

  // WebView2 (Windows) runs this before <html> exists; whatever needs it waits until it does.
  function withRoot(fn) {
    if (document.documentElement) return fn(document.documentElement);
    new MutationObserver((_, observer) => {
      if (!document.documentElement) return;
      observer.disconnect();
      fn(document.documentElement);
    }).observe(document, { childList: true });
  }
  // Settings come from the app: injected when the tab opened, then pushed with update(). The last
  // pushed values are also kept in this site's storage so a reloaded tab doesn't fall back to the
  // ones injected at tab creation; "rev" says which copy is newer.
  const SETTINGS_KEY = 'detube.settings';
  function readSettings() { try { return JSON.parse(localStorage.getItem(SETTINGS_KEY)) || {}; } catch (e) { return {}; } }
  const injected = window.__mt2Settings || {}, stored = readSettings();
  let settings = Object.assign({ showShorts: true, showAI: true, blocked: [], rev: 0 },
                               (stored.rev || 0) > (injected.rev || 0) ? stored : injected);
  let blocked = new Set(settings.blocked);
  let aiChannels = new Set(load());
  const results = new Map(); // videoId -> Promise<{ ai, channel }>
  const allowed = new Set(); // videoIds the user chose to watch anyway
  let lastUrl = '';
  let blockedId = null;

  function load() { try { return JSON.parse(localStorage.getItem(KEY)) || []; } catch (e) { return []; } }
  function save() { try { localStorage.setItem(KEY, JSON.stringify([...aiChannels])); } catch (e) {} }
  function norm(handle) { try { return decodeURIComponent(handle).toLowerCase(); } catch (e) { return handle.toLowerCase(); } }
  // "/@Name/videos" → "@name", "/channel/UC…" → "channel/uc…" (same form as the app's blocklist).
  function channelKey(href) {
    const m = (href || '').match(/^(?:https?:\/\/(?:www\.)?youtube\.com)?\/(@[^\/?#]+|channel\/[^\/?#]+)/);
    return m ? norm(m[1]) : null;
  }
  const CHANNEL_LINK = 'a[href^="/@"], a[href^="/channel/"]';

  // One rule per selector so an unsupported selector can't invalidate the rest.
  const style = document.createElement('style');
  style.textContent = SHORTS_SELECTORS.map(s => `html[mt2-hide-shorts] ${s} { display: none !important; }`).join('\n') + `
    html[mt2-hide-ai] .mt2-ai { display: none !important; }
    .mt2-blocked { display: none !important; }
    #mt2-block-channel { display: inline-flex; align-items: center; gap: 6px; flex: none; height: 36px;
      margin-left: 8px; padding: 0 16px 0 12px; border: 0; border-radius: 18px; cursor: pointer;
      font: 500 14px Roboto, Arial, sans-serif; color: var(--yt-spec-text-primary, #0f0f0f);
      background: var(--yt-spec-badge-chip-background, rgba(0,0,0,.05)); }
    #mt2-block-channel:hover { background: var(--yt-spec-button-chip-background-hover, rgba(0,0,0,.1)); }
    #mt2-block-channel svg { width: 22px; height: 22px; fill: currentColor; }
    #mt2-block { position: fixed; inset: 0; z-index: 2147483647; display: flex; flex-direction: column;
      align-items: center; justify-content: center; gap: 20px; background: rgba(15,15,15,.97);
      color: #fff; font: 16px "Segoe UI", system-ui, -apple-system, sans-serif; }
    #mt2-block p { margin: 0; }
    #mt2-block div { display: flex; gap: 12px; }
    #mt2-block button { font: inherit; padding: 8px 18px; border: 0; border-radius: 18px; cursor: pointer;
      background: #fff; color: #0f0f0f; }
    #mt2-block button + button { background: #3f3f3f; color: #fff; }`;
  withRoot(root => root.appendChild(style));

  function applyAttrs() {
    withRoot(root => {
      root.toggleAttribute('mt2-hide-shorts', !settings.showShorts);
      root.toggleAttribute('mt2-hide-ai', !settings.showAI);
    });
  }

  function videoId() {
    if (location.pathname === '/watch') return new URLSearchParams(location.search).get('v');
    const m = location.pathname.match(/^\/shorts\/([\w-]+)/);
    return m ? m[1] : null;
  }

  function channelOf(html) {
    const m = html.match(/"ownerProfileUrl":"https?:\/\/www\.youtube\.com\/(@[^"\/?]+)"/)
           || html.match(/"canonicalBaseUrl":"\/(@[^"\/?]+)"/);
    return m ? norm(m[1]) : null;
  }

  // Fetch the video's page logged-out, in English, and look for the disclosure text.
  function check(id) {
    if (!results.has(id)) {
      results.set(id, fetch(`/watch?v=${encodeURIComponent(id)}&hl=en`, { credentials: 'omit' })
        .then(r => r.text())
        .then(html => ({ ai: AI_MARKERS.some(m => html.includes(m)), channel: channelOf(html) }))
        .catch(() => { results.delete(id); return { ai: false, channel: null }; }));
    }
    return results.get(id);
  }

  function button(label, onClick) {
    const b = document.createElement('button');
    b.textContent = label;
    b.addEventListener('click', onClick);
    return b;
  }

  function block(id, message) {
    unblock();
    blockedId = id;
    const isShort = location.pathname.startsWith('/shorts/');
    const box = document.createElement('div');
    box.id = 'mt2-block';
    const msg = document.createElement('p');
    msg.textContent = message;
    const actions = document.createElement('div');
    actions.append(
      button(isShort ? 'Next Short' : 'Go back', () => {
        const next = document.querySelector('#navigation-button-down button');
        if (isShort && next) next.click(); else history.back();
      }),
      button('Watch anyway', () => {
        allowed.add(id);
        unblock();
        const v = document.querySelector('video');
        if (v) v.play();
      })
    );
    box.append(msg, actions);
    document.documentElement.appendChild(box);
    document.querySelectorAll('video').forEach(v => v.pause());
  }

  function unblock() {
    blockedId = null;
    const box = document.getElementById('mt2-block');
    if (box) box.remove();
  }

  // Keep playback paused while a video is blocked (YouTube autoplays).
  document.addEventListener('play', e => { if (blockedId) e.target.pause(); }, true);

  async function onUrlChange() {
    const id = videoId();
    if (blockedId && blockedId !== id) unblock();
    if (!settings.showShorts && location.pathname.startsWith('/shorts')) { location.replace('/'); return; }
    if (settings.showAI || !id || allowed.has(id) || blockedId === id) return;
    const r = await check(id);
    if (!r.ai) return;
    if (r.channel && !aiChannels.has(r.channel)) { aiChannels.add(r.channel); save(); }
    if (videoId() === id && !settings.showAI && !allowed.has(id)) block(id, 'Hidden by DeTube — YouTube labels this video as made with AI.');
  }

  // Hide feed items from blocked channels, and mark those from channels caught posting AI-labeled videos.
  // YouTube reuses item elements for new videos, so the blocked mark is re-evaluated every pass.
  function scan() {
    document.querySelectorAll(ITEMS).forEach(el => {
      const a = el.querySelector(CHANNEL_LINK);
      const channel = a && channelKey(a.getAttribute('href'));
      el.classList.toggle('mt2-blocked', !!channel && blocked.has(channel));
      if (channel && !settings.showAI && aiChannels.has(channel)) el.classList.add('mt2-ai');
    });
  }

  function watchedChannel() {
    const a = document.querySelector('ytd-watch-metadata #owner ' + CHANNEL_LINK.replace(', ', ', ytd-watch-metadata #owner '));
    return a && channelKey(a.getAttribute('href'));
  }

  // Stop videos from blocked channels, once the page shows the current video's channel.
  function checkChannel() {
    const id = videoId();
    if (location.pathname !== '/watch' || !id || allowed.has(id) || blockedId === id) return;
    const page = document.querySelector('ytd-watch-flexy');
    if (!page || page.getAttribute('video-id') !== id) return;
    const channel = watchedChannel();
    if (channel && blocked.has(channel)) block(id, 'Hidden by DeTube — you blocked this channel.');
  }

  // A "Block" button next to Like / Share / Save. YouTube re-renders that row, so it's re-added as needed.
  function addBlockButton() {
    if (location.pathname !== '/watch' || document.getElementById('mt2-block-channel')) return;
    const menu = document.querySelector('ytd-watch-metadata #actions ytd-menu-renderer');
    if (!menu) return;
    const b = document.createElement('button');
    b.id = 'mt2-block-channel';
    b.title = 'Block this channel: hide its videos everywhere (DeTube)';
    const NS = 'http://www.w3.org/2000/svg';
    const icon = document.createElementNS(NS, 'svg');
    icon.setAttribute('viewBox', '0 0 24 24');
    const path = document.createElementNS(NS, 'path');
    path.setAttribute('d', 'M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm0 2a8 8 0 0 1 6.32 12.9L7.1 5.68A7.96 7.96 0 0 1 12 4zM4 12c0-1.85.63-3.55 1.68-4.9L16.9 18.32A8 8 0 0 1 4 12z');
    icon.appendChild(path);
    const label = document.createElement('span');
    label.textContent = 'Block';
    b.append(icon, label);
    b.addEventListener('click', () => {
      const channel = watchedChannel();
      if (!channel) return;
      blocked.add(channel);
      allowed.delete(videoId());
      // The app saves it and pushes the new list to every tab (detube.invalid is never loaded).
      window.open('https://detube.invalid/block?c=' + encodeURIComponent(channel));
      checkChannel();
    });
    (menu.querySelector('#top-level-buttons-computed') || menu).appendChild(b);
  }

  // YouTube is a single-page app: poll for URL changes instead of relying on page loads.
  setInterval(() => {
    if (location.href !== lastUrl) { lastUrl = location.href; onUrlChange(); }
    scan();
    checkChannel();
    addBlockButton();
  }, 400);

  window.__mt2 = {
    update(next) {
      settings = Object.assign(settings, next);
      try { localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings)); } catch (e) {}
      blocked = new Set(settings.blocked);
      applyAttrs();
      unblock();
      lastUrl = ''; // re-evaluate the current page (blocks again if it still should be)
    },
    forgetChannels() {
      aiChannels.clear();
      save();
      document.querySelectorAll('.mt2-ai').forEach(el => el.classList.remove('mt2-ai'));
    },
  };
  applyAttrs();

  // Tab shortcuts while YouTube has focus. The app intercepts these window.open calls
  // (detube.invalid is never loaded) and acts on them.
  document.addEventListener('keydown', e => {
    if (!e.ctrlKey || e.altKey || e.metaKey) return;
    const key = e.key.toLowerCase();
    const cmd = key === 't' && !e.shiftKey ? 'newtab'
              : key === 'w' && !e.shiftKey ? 'close'
              : key === 'tab' ? (e.shiftKey ? 'prev' : 'next')
              : null;
    if (!cmd) return;
    e.preventDefault();
    e.stopPropagation();
    window.open('https://detube.invalid/' + cmd);
  }, true);
})();

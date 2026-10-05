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
    'yt-tab-shape[tab-title="Shorts"]',
  ];
  const ITEMS = 'ytd-rich-item-renderer, ytd-video-renderer, ytd-compact-video-renderer, ytd-grid-video-renderer, yt-lockup-view-model';
  const KEY = 'mt2.aiChannels';

  const root = document.documentElement;
  // Settings: the app injects its current values; the last values pushed with update()
  // are also kept in this site's storage so reloaded tabs pick them up.
  const SETTINGS_KEY = 'wt.settings';
  let settings = Object.assign({ showShorts: true, showAI: true }, window.__mt2Settings, readSettings());
  function readSettings() { try { return JSON.parse(localStorage.getItem(SETTINGS_KEY)) || {}; } catch (e) { return {}; } }
  let aiChannels = new Set(load());
  const results = new Map(); // videoId -> Promise<{ ai, channel }>
  const allowed = new Set(); // videoIds the user chose to watch anyway
  let lastUrl = '';
  let blockedId = null;

  function load() { try { return JSON.parse(localStorage.getItem(KEY)) || []; } catch (e) { return []; } }
  function save() { try { localStorage.setItem(KEY, JSON.stringify([...aiChannels])); } catch (e) {} }
  function norm(handle) { try { return decodeURIComponent(handle).toLowerCase(); } catch (e) { return handle.toLowerCase(); } }

  // One rule per selector so an unsupported selector can't invalidate the rest.
  const style = document.createElement('style');
  style.textContent = SHORTS_SELECTORS.map(s => `html[mt2-hide-shorts] ${s} { display: none !important; }`).join('\n') + `
    html[mt2-hide-ai] .mt2-ai { display: none !important; }
    #mt2-block { position: fixed; inset: 0; z-index: 2147483647; display: flex; flex-direction: column;
      align-items: center; justify-content: center; gap: 20px; background: rgba(15,15,15,.97);
      color: #fff; font: 16px -apple-system, BlinkMacSystemFont, sans-serif; }
    #mt2-block p { margin: 0; }
    #mt2-block div { display: flex; gap: 12px; }
    #mt2-block button { font: inherit; padding: 8px 18px; border: 0; border-radius: 18px; cursor: pointer;
      background: #fff; color: #0f0f0f; }
    #mt2-block button + button { background: #3f3f3f; color: #fff; }`;
  root.appendChild(style);

  function applyAttrs() {
    root.toggleAttribute('mt2-hide-shorts', !settings.showShorts);
    root.toggleAttribute('mt2-hide-ai', !settings.showAI);
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

  function block(id) {
    unblock();
    blockedId = id;
    const isShort = location.pathname.startsWith('/shorts/');
    const box = document.createElement('div');
    box.id = 'mt2-block';
    const msg = document.createElement('p');
    msg.textContent = 'Hidden by WinTube — YouTube labels this video as made with AI.';
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
    root.appendChild(box);
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
    if (videoId() === id && !settings.showAI && !allowed.has(id)) block(id);
  }

  // Mark feed items from channels already caught posting labeled videos.
  function scan() {
    if (settings.showAI || !aiChannels.size) return;
    document.querySelectorAll(ITEMS).forEach(el => {
      if (el.classList.contains('mt2-ai')) return;
      const a = el.querySelector('a[href^="/@"]');
      if (a && aiChannels.has(norm(a.getAttribute('href').slice(1).split(/[\/?]/)[0]))) el.classList.add('mt2-ai');
    });
  }

  // YouTube is a single-page app: poll for URL changes instead of relying on page loads.
  setInterval(() => {
    if (location.href !== lastUrl) { lastUrl = location.href; onUrlChange(); }
    scan();
  }, 400);

  window.__mt2 = {
    update(next) {
      settings = Object.assign(settings, next);
      try { localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings)); } catch (e) {}
      applyAttrs();
      if (settings.showAI) unblock();
      lastUrl = ''; // re-evaluate the current page
    },
    forgetChannels() {
      aiChannels.clear();
      save();
      document.querySelectorAll('.mt2-ai').forEach(el => el.classList.remove('mt2-ai'));
    },
  };
  applyAttrs();

  // Tab shortcuts while YouTube has focus. The app intercepts these window.open calls
  // (wintube.invalid is never loaded) and acts on them.
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
    window.open('https://wintube.invalid/' + cmd);
  }, true);
})();

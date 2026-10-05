<p align="center">
  <img src="icon.png" width="160" alt="WinTube icon">
</p>

<h1 align="center">WinTube</h1>

<p align="center">
  YouTube as a real Windows app — without Shorts, without AI slop, without a browser full of tabs.
</p>

<p align="center">
  <a href="https://github.com/depo23/WinTube/releases/latest/download/WinTube-Setup.exe"><img src="https://img.shields.io/badge/Download-WinTube%20for%20Windows-black?style=for-the-badge&logo=windows" alt="Download WinTube"></a>
</p>

<p align="center"><sub>Windows 10 or 11 · always the latest build</sub></p>

---

## Why WinTube

**Watch what you chose, not what the algorithm pushes.**

- **Shorts off, for good.** One switch removes Shorts from your home feed, search, sidebar and recommendations. Shorts links take you back Home.
- **Skip AI-generated videos.** Videos YouTube labels *Made with AI* are stopped before they play. Channels caught posting them disappear from your feed too, so it gets cleaner the more you watch. One click to watch anyway.
- **Tabs.** Ctrl+T for a new tab, Ctrl+click a video to open it in one. Closing a tab stops its sound.
- **Links go where they should.** Links in descriptions and comments open in your default browser.
- **Always current.** A banner tells you when a new version is out.

## Controls

| | Where | Shortcut |
| --- | --- | --- |
| Show / hide Shorts | ⚙ menu | — |
| Show / hide AI videos | ⚙ menu | — |
| New tab | **+** button | Ctrl+T |
| Close tab | **×** on the tab, or middle-click it | Ctrl+W |
| Next / previous tab | | Ctrl+Tab / Ctrl+Shift+Tab |
| Check for updates | ⚙ menu | |

## Install

1. [Download WinTube-Setup.exe](https://github.com/depo23/WinTube/releases/latest/download/WinTube-Setup.exe) and run it.
2. If Windows shows *"Windows protected your PC"*, click **More info → Run anyway**. (WinTube isn't signed with a paid certificate, so Windows asks once.)

## Good to know

- AI detection relies on YouTube's own *Made with AI* label. Videos their creators don't disclose — and YouTube doesn't catch — will still show up.
- Changed your mind about a channel? **⚙ → Forget learned AI channels** resets the list.

## Build it yourself

Every push to `main` builds a new release automatically. To build locally you need [Rust](https://rustup.rs) and Node.js: `npx @tauri-apps/cli@2 build`.

---

<sub>Windows sibling of [MacTube 2](https://github.com/depo23/MacTube2). Not affiliated with YouTube or Google.</sub>

<p align="center">
  <img src="icon.png" width="160" alt="DeTube icon">
</p>

<h1 align="center">DeTube</h1>

<p align="center">
  YouTube as a real desktop app — without Shorts, without AI slop, without a browser full of tabs.
</p>

<p align="center">
  <a href="https://github.com/depo23/DeTube/releases/latest/download/DeTube-Setup.exe"><img src="https://img.shields.io/badge/Windows-Download-black?style=for-the-badge&logo=windows" alt="Download for Windows"></a>
  &nbsp;
  <a href="https://github.com/depo23/DeTube/releases/latest/download/DeTube-x86_64.AppImage"><img src="https://img.shields.io/badge/Linux-AppImage-black?style=for-the-badge&logo=linux" alt="Download AppImage for Linux"></a>
</p>

<p align="center"><sub>
  Windows 10 / 11 · Linux x86_64 (also as <a href="https://github.com/depo23/DeTube/releases/latest/download/DeTube-amd64.deb">.deb</a> and <a href="https://github.com/depo23/DeTube/releases/latest/download/DeTube-x86_64.rpm">.rpm</a>) · always the latest build · on a Mac? See <a href="https://github.com/depo23/MacTube2">MacTube 2</a>
</sub></p>

---

## Why DeTube

**Watch what you chose, not what the algorithm pushes.**

- **Shorts off, for good.** One switch removes Shorts from your home feed, search, sidebar and recommendations. Shorts links take you back Home.
- **Skip AI-generated videos.** Videos YouTube labels *Made with AI* are stopped before they play. Channels caught posting them disappear from your feed too, so it gets cleaner the more you watch. One click to watch anyway.
- **No ads.** Video ads are removed before the player sees them, any that slip through are muted and skipped, and promoted videos and ad banners are hidden from your feed, search and watch pages.
- **Live chat replay: fully working 😉** Live streams and past streams load their chat alongside the video, with no "your browser is out of date" message.
- **Block channels.** Press **Block** under any video and that channel is gone: its videos vanish from your feed and search, and won't play unless you choose to. Edit the list anytime in **⚙ → Blocked channels…**.
- **Tabs.** Ctrl+T for a new tab, Ctrl+click a video to open it in one. Closing a tab stops its sound.
- **Links go where they should.** Links in descriptions and comments open in your default browser.
- **Always current.** A banner tells you when a new version is out.

## Controls

| | Where | Shortcut |
| --- | --- | --- |
| Show / hide Shorts | ⚙ menu | |
| Show / hide AI videos | ⚙ menu | |
| Block a channel | **Block** button under any video | |
| Edit blocked channels | ⚙ → **Blocked channels…** | |
| New tab | **+** button | Ctrl+T |
| Close tab | **×** on the tab, or middle-click it | Ctrl+W |
| Next / previous tab | | Ctrl+Tab / Ctrl+Shift+Tab |
| Check for updates | ⚙ menu | |

## Install

**Windows** — [download DeTube-Setup.exe](https://github.com/depo23/DeTube/releases/latest/download/DeTube-Setup.exe) and run it. If Windows shows *"Windows protected your PC"*, click **More info → Run anyway** (DeTube isn't signed with a paid certificate, so Windows asks once).

**Linux** — pick one:

- **AppImage** (any distribution): download [DeTube-x86_64.AppImage](https://github.com/depo23/DeTube/releases/latest/download/DeTube-x86_64.AppImage), then `chmod +x DeTube-x86_64.AppImage` and run it. On NixOS: `appimage-run DeTube-x86_64.AppImage`.
- **Ubuntu / Debian / Mint:** `sudo apt install ./DeTube-amd64.deb` ([download](https://github.com/depo23/DeTube/releases/latest/download/DeTube-amd64.deb))
- **Fedora / openSUSE:** `sudo dnf install ./DeTube-x86_64.rpm` ([download](https://github.com/depo23/DeTube/releases/latest/download/DeTube-x86_64.rpm))

If videos don't play on Linux, install your distribution's GStreamer codecs (on Ubuntu: `sudo apt install gstreamer1.0-plugins-bad gstreamer1.0-libav`). The AppImage already includes them.

## Good to know

- AI detection relies on YouTube's own *Made with AI* label. Videos their creators don't disclose — and YouTube doesn't catch — will still show up.
- Changed your mind about a channel? **⚙ → Forget learned AI channels** resets the list.

## Build it yourself

Every push to `main` builds Windows and Linux releases automatically. To build locally you need [Rust](https://rustup.rs) and Node.js (plus the [Tauri Linux packages](https://v2.tauri.app/start/prerequisites/#linux) on Linux): `npx @tauri-apps/cli@2 build`.

---

<sub>Sibling of [MacTube 2](https://github.com/depo23/MacTube2). Not affiliated with YouTube or Google.</sub>

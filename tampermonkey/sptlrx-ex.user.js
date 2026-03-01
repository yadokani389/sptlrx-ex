// ==UserScript==
// @name         sptlrx-ex relay bridge
// @namespace    https://github.com/yadokani389/sptlrx-ex
// @version      0.1.0
// @description  Relay Spotify Web Player lyric state to local sptlrx-ex HTTP relay.
// @license      MIT
// @author       yadokani389
// @match        https://open.spotify.com/*
// @run-at       document-idle
// @grant        GM_getValue
// @grant        GM_setValue
// @grant        GM_registerMenuCommand
// @grant        GM_xmlhttpRequest
// @connect      127.0.0.1
// @connect      localhost
// ==/UserScript==

(() => {
  "use strict";

  const STORAGE_KEY = "relayConfig";
  const DEFAULT_RELAY_URL = "http://127.0.0.1:17373/lyrics";
  const POLL_MS = 1000;
  const DEBOUNCE_MS = 120;
  const HEARTBEAT_MS = 5000;
  const REQUEST_TIMEOUT_MS = 1500;

  let relayUrl = DEFAULT_RELAY_URL;
  let timerId = null;
  let observer = null;
  let lastSignature = "";
  let lastSentAt = 0;

  function normalizeText(value) {
    if (typeof value !== "string") {
      return "";
    }
    return value.replace(/\s+/g, " ").trim();
  }

  function normalizeRelayUrl(value) {
    if (typeof value !== "string") {
      return DEFAULT_RELAY_URL;
    }

    const trimmed = value.trim();
    if (!trimmed) {
      return DEFAULT_RELAY_URL;
    }

    try {
      const parsed = new URL(trimmed);
      if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
        return DEFAULT_RELAY_URL;
      }
      if (parsed.hostname !== "127.0.0.1" && parsed.hostname !== "localhost") {
        return DEFAULT_RELAY_URL;
      }
      parsed.hash = "";
      return parsed.toString().replace(/\/$/, "");
    } catch {
      return DEFAULT_RELAY_URL;
    }
  }

  function titleFromDocumentTitle() {
    const pageTitle = normalizeText(document.title || "");
    if (!pageTitle || pageTitle.toLowerCase() === "spotify") {
      return "";
    }

    for (const separator of [" • ", " · ", " | ", " - "]) {
      if (!pageTitle.includes(separator)) {
        continue;
      }
      const [head] = pageTitle.split(separator);
      const title = normalizeText(head);
      if (title && title.toLowerCase() !== "spotify") {
        return title;
      }
    }

    return pageTitle;
  }

  function getTitle() {
    const selectors = [
      '[data-testid="now-playing-widget"] [data-testid="context-item-info-title"]',
      '[data-testid="now-playing-bar"] [data-testid="context-item-info-title"]',
      '[data-testid="context-item-info-title"]',
      '[data-testid="now-playing-widget"] [data-testid="context-item-link"]',
      '[data-testid="now-playing-bar"] [data-testid="context-item-link"]',
    ];

    for (const selector of selectors) {
      const text = normalizeText(
        document.querySelector(selector)?.textContent || "",
      );
      if (text) {
        return text;
      }
    }

    const nowPlayingLabel = normalizeText(
      document
        .querySelector('[data-testid="now-playing-widget"]')
        ?.getAttribute("aria-label") || "",
    );
    if (nowPlayingLabel) {
      const match = nowPlayingLabel.match(/^Now playing:\s*(.+?)\s+by\s+.+$/i);
      if (match) {
        const title = normalizeText(match[1]);
        if (title) {
          return title;
        }
      }
    }

    return titleFromDocumentTitle();
  }

  function getArtists() {
    const selectors = [
      'a[data-testid="context-item-info-artist"][href*="/artist/"]',
      '[data-testid="context-item-info-artist"] a[href*="/artist/"]',
      '[data-testid="now-playing-widget"] a[href*="/artist/"]',
      '[data-testid="now-playing-bar"] a[href*="/artist/"]',
    ];

    for (const selector of selectors) {
      const values = Array.from(document.querySelectorAll(selector))
        .map((node) => normalizeText(node.textContent || ""))
        .filter(Boolean);
      if (values.length > 0) {
        return Array.from(new Set(values));
      }
    }

    return [];
  }

  function getLyricNodes() {
    return Array.from(
      document.querySelectorAll(
        '[data-testid="lyrics-line"], [data-testid="fullscreen-lyric"]',
      ),
    ).filter((node) => normalizeText(node.textContent || "").length > 0);
  }

  function getCurrentNode(lyricNodes) {
    if (lyricNodes.length === 0) {
      return null;
    }

    const classFrequency = new Map();
    for (const node of lyricNodes) {
      for (const className of node.classList) {
        classFrequency.set(className, (classFrequency.get(className) || 0) + 1);
      }
    }

    let bestNode = null;
    let bestScore = Number.NEGATIVE_INFINITY;
    for (const node of lyricNodes) {
      let score = 0;
      for (const className of node.classList) {
        const count = classFrequency.get(className) || 0;
        if (count === 1) {
          score += 10;
        } else if (count === 2) {
          score += 3;
        } else if (count === 3) {
          score += 1;
        }
      }

      if (score > bestScore) {
        bestScore = score;
        bestNode = node;
      }
    }

    if (bestScore <= 0) {
      return null;
    }
    return bestNode;
  }

  function getLyricsPanelOpen(lyricNodes) {
    const button = document.querySelector(
      'button[data-testid="lyrics-button"]',
    );
    if (button) {
      if (button.getAttribute("aria-pressed") === "true") {
        return true;
      }
      if (button.getAttribute("data-active") === "true") {
        return true;
      }
    }
    return lyricNodes.length > 0;
  }

  function buildPayload() {
    const lyricNodes = getLyricNodes();
    const lines = lyricNodes.map((node) =>
      normalizeText(node.textContent || ""),
    );
    const currentNode = getCurrentNode(lyricNodes);
    const currentIndex = lyricNodes.findIndex((node) => node === currentNode);
    const currentText = currentIndex >= 0 ? lines[currentIndex] : "";
    const lyricsPanelOpen = getLyricsPanelOpen(lyricNodes);

    let status = "ok";
    if (!lyricsPanelOpen) {
      status = "lyrics_panel_closed";
    } else if (lines.length === 0) {
      status = "lyrics_not_available";
    } else if (!currentText) {
      status = "current_line_not_detected";
    }

    return {
      schemaVersion: 1,
      source: "spotify-web-player",
      title: getTitle(),
      artists: getArtists(),
      status,
      lines,
      linesCount: lines.length,
      lyricsPanelOpen,
      currentLine: currentText
        ? { text: currentText, index: currentIndex }
        : null,
      timestamp: new Date().toISOString(),
    };
  }

  function requestApi() {
    if (typeof GM_xmlhttpRequest === "function") {
      return GM_xmlhttpRequest;
    }
    if (
      typeof GM === "object" &&
      GM !== null &&
      typeof GM.xmlHttpRequest === "function"
    ) {
      return GM.xmlHttpRequest.bind(GM);
    }
    return null;
  }

  function postPayload(payload) {
    const api = requestApi();
    if (!api) {
      return;
    }

    api({
      method: "POST",
      url: relayUrl,
      headers: { "content-type": "application/json" },
      data: JSON.stringify(payload),
      timeout: REQUEST_TIMEOUT_MS,
    });
  }

  function run() {
    timerId = null;

    const payload = buildPayload();
    const signature = JSON.stringify({
      title: payload.title,
      artists: payload.artists,
      status: payload.status,
      linesCount: payload.linesCount,
      current: payload.currentLine?.text || "",
      currentIndex: payload.currentLine?.index ?? -1,
    });

    const now = Date.now();
    const shouldHeartbeat = now - lastSentAt >= HEARTBEAT_MS;
    if (signature === lastSignature && !shouldHeartbeat) {
      return;
    }

    lastSignature = signature;
    lastSentAt = now;
    postPayload(payload);
  }

  function schedule() {
    if (timerId !== null) {
      return;
    }
    timerId = setTimeout(run, DEBOUNCE_MS);
  }

  async function loadRelayUrl() {
    if (typeof GM_getValue === "function") {
      const value = GM_getValue(STORAGE_KEY, null);
      relayUrl = normalizeRelayUrl(value?.relayUrl);
      return;
    }

    if (
      typeof GM === "object" &&
      GM !== null &&
      typeof GM.getValue === "function"
    ) {
      try {
        const value = await GM.getValue(STORAGE_KEY, null);
        relayUrl = normalizeRelayUrl(value?.relayUrl);
      } catch {
        relayUrl = DEFAULT_RELAY_URL;
      }
      return;
    }

    relayUrl = DEFAULT_RELAY_URL;
  }

  function saveRelayUrl(nextUrl) {
    const value = { relayUrl: normalizeRelayUrl(nextUrl) };
    relayUrl = value.relayUrl;

    if (typeof GM_setValue === "function") {
      GM_setValue(STORAGE_KEY, value);
      return;
    }
    if (
      typeof GM === "object" &&
      GM !== null &&
      typeof GM.setValue === "function"
    ) {
      const maybePromise = GM.setValue(STORAGE_KEY, value);
      if (
        maybePromise !== null &&
        typeof maybePromise === "object" &&
        typeof maybePromise.catch === "function"
      ) {
        maybePromise.catch(() => {});
      }
    }
  }

  function registerMenu() {
    const register =
      typeof GM_registerMenuCommand === "function"
        ? GM_registerMenuCommand
        : typeof GM === "object" &&
            GM !== null &&
            typeof GM.registerMenuCommand === "function"
          ? GM.registerMenuCommand.bind(GM)
          : null;

    if (!register) {
      return;
    }

    register("sptlrx-ex: Set relay URL", () => {
      const input = window.prompt(
        "sptlrx-ex relay URL (localhost only)",
        relayUrl,
      );
      if (input === null) {
        return;
      }
      saveRelayUrl(input);
      window.alert(`sptlrx-ex relay URL: ${relayUrl}`);
    });

    register("sptlrx-ex: Reset relay URL", () => {
      saveRelayUrl(DEFAULT_RELAY_URL);
      window.alert(`sptlrx-ex relay URL: ${relayUrl}`);
    });

    register("sptlrx-ex: Show relay URL", () => {
      window.alert(`sptlrx-ex relay URL: ${relayUrl}`);
    });
  }

  async function start() {
    if (window.top !== window.self) {
      return;
    }

    await loadRelayUrl();
    registerMenu();

    observer = new MutationObserver(schedule);
    observer.observe(document.documentElement, {
      subtree: true,
      childList: true,
      characterData: true,
      attributes: true,
    });

    setInterval(schedule, POLL_MS);
    window.addEventListener("hashchange", schedule);
    window.addEventListener("popstate", schedule);
    window.addEventListener("visibilitychange", schedule);
    schedule();
  }

  void start();
})();

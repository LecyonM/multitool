// Gestion du thème (clair / sombre / automatique) et du mode contraste élevé.
// Chargé dans <head> : le thème est donc appliqué avant le premier affichage.
(() => {
  'use strict';

  const KEY = 'multitool.prefs';
  const THEMES = ['auto', 'light', 'dark'];
  const root = document.documentElement;

  const media = (query) => (window.matchMedia ? window.matchMedia(query) : { matches: false });
  const systemDark = media('(prefers-color-scheme: dark)');
  const systemContrast = media('(prefers-contrast: more)');

  function read() {
    try {
      return JSON.parse(localStorage.getItem(KEY)) || {};
    } catch {
      return {};
    }
  }

  function write() {
    try {
      localStorage.setItem(KEY, JSON.stringify(prefs));
    } catch {
      /* stockage indisponible : les préférences ne seront pas conservées */
    }
  }

  const saved = read();
  const prefs = {
    theme: THEMES.includes(saved.theme) ? saved.theme : 'auto',
    // null = suivre le réglage du système.
    contrast: typeof saved.contrast === 'boolean' ? saved.contrast : null,
  };

  const api = {
    get theme() {
      return prefs.theme;
    },
    /** Vrai si le contraste élevé est actif (choix de l'utilisateur ou du système). */
    get contrast() {
      return prefs.contrast === null ? Boolean(systemContrast.matches) : prefs.contrast;
    },
    setTheme(value) {
      if (!THEMES.includes(value)) return;
      prefs.theme = value;
      write();
      apply();
    },
    setContrast(value) {
      prefs.contrast = Boolean(value);
      write();
      apply();
    },
  };

  function apply() {
    const dark = prefs.theme === 'auto' ? Boolean(systemDark.matches) : prefs.theme === 'dark';
    root.dataset.theme = dark ? 'dark' : 'light';
    root.dataset.contrast = api.contrast ? 'more' : 'normal';
    document.dispatchEvent(new CustomEvent('themechange'));
  }

  for (const query of [systemDark, systemContrast]) {
    if (query.addEventListener) query.addEventListener('change', apply);
  }

  window.MultitoolTheme = api;
  apply();
})();

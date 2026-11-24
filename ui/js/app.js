// Interface de Multitool.
// Rien n'est codé en dur ici : la liste, les formulaires et les résultats sont
// construits à partir de ce que déclarent les utilitaires côté Rust.
(() => {
  'use strict';

  const tauri = window.__TAURI__;
  const invoke = tauri && tauri.core ? tauri.core.invoke : null;
  const theme = window.MultitoolTheme;

  const $ = (selector) => document.querySelector(selector);
  const els = {
    nav: $('#utility-nav'),
    search: $('#search'),
    banner: $('#banner'),
    empty: $('#empty-state'),
    panel: $('#utility-panel'),
    category: $('#utility-category'),
    title: $('#utility-title'),
    description: $('#utility-description'),
    form: $('#utility-form'),
    fields: $('#utility-fields'),
    run: $('#run-button'),
    readout: $('#readout'),
    readoutBody: $('#readout-body'),
    readoutTitle: $('#readout-title'),
    copy: $('#copy-result'),
    themeSelect: $('#theme-select'),
    contrast: $('#contrast-toggle'),
    announce: $('#announce'),
  };

  const LAST_KEY = 'multitool.last';
  const COPY_LABEL = 'Copier le résultat';
  const FOLDERS_KEY = 'multitool.folders';
  const RUN_LABEL = 'Exécuter';
  const state = {
    utilities: [],
    selectedId: null,
    lastText: '',
    copyTimer: null,
    runId: 0, // identifie l'exécution en cours : ignore les messages d'une exécution terminée
    running: false,
    runningId: null, // utilitaire en cours d'exécution
    lastProgress: null,
  };

  const store = {
    get(key) {
      try {
        return localStorage.getItem(key);
      } catch {
        return null;
      }
    },
    set(key, value) {
      try {
        localStorage.setItem(key, value);
      } catch {
        /* stockage indisponible */
      }
    },
  };

  /** Crée un élément DOM. Les textes passent par textContent : aucun HTML injecté. */
  function h(tag, props, ...children) {
    const node = document.createElement(tag);
    for (const [key, value] of Object.entries(props || {})) {
      if (value === null || value === undefined || value === false) continue;
      if (key === 'class') node.className = value;
      else if (key === 'text') node.textContent = value;
      else if (key.startsWith('on')) node.addEventListener(key.slice(2), value);
      else node.setAttribute(key, value === true ? '' : value);
    }
    for (const child of children) if (child) node.append(child);
    return node;
  }

  /**
   * Empêche la coupure de ligne au milieu d'un mot à trait d'union (« yt-dlp »
   * ne doit pas finir en « yt- / dlp »). Les caractères restent les mêmes : un
   * copier-coller d'une commande fonctionne toujours.
   */
  const HYPHENATED = /\b[\p{L}\d]+(?:-[\p{L}\d]+)+\b/gu;

  function softText(text) {
    const nodes = [];
    let last = 0;
    for (const match of text.matchAll(HYPHENATED)) {
      if (match[0].length > 20) continue;
      if (match.index > last) nodes.push(text.slice(last, match.index));
      nodes.push(h('span', { class: 'nowrap', text: match[0] }));
      last = match.index + match[0].length;
    }
    if (last < text.length) nodes.push(text.slice(last));
    return nodes;
  }

  const normalize = (text) =>
    text.normalize('NFD').replace(/\p{Diacritic}/gu, '').toLowerCase();

  const current = () => state.utilities.find((u) => u.id === state.selectedId);

  function announce(message) {
    els.announce.textContent = '';
    // Léger délai pour que les lecteurs d'écran relisent un message identique.
    setTimeout(() => (els.announce.textContent = message), 50);
  }

  /* ---------- Liste des utilitaires ---------- */

  function renderNav() {
    const raw = els.search.value.trim();
    const query = normalize(raw);
    els.nav.replaceChildren();

    if (!state.utilities.length) {
      els.nav.append(h('p', { class: 'nav-empty', text: 'Aucun utilitaire disponible.' }));
      return;
    }

    const matches = state.utilities.filter(
      (u) => !query || normalize(`${u.name} ${u.description} ${u.category}`).includes(query),
    );
    if (!matches.length) {
      els.nav.append(h('p', { class: 'nav-empty', text: `Aucun utilitaire ne correspond à « ${raw} ».` }));
      return;
    }

    const groups = new Map();
    for (const utility of matches) {
      if (!groups.has(utility.category)) groups.set(utility.category, []);
      groups.get(utility.category).push(utility);
    }

    for (const [category, items] of groups) {
      const list = h('ul', { class: 'nav-list', role: 'list' });
      for (const utility of items) {
        const button = h('button', {
          type: 'button',
          class: 'nav-item',
          title: utility.description,
          'aria-current': utility.id === state.selectedId ? 'true' : null,
          onclick: () => selectUtility(utility.id),
          text: utility.name,
        });
        list.append(h('li', {}, button));
      }
      els.nav.append(
        h('section', { class: 'nav-group' }, h('h2', { class: 'nav-group-title', text: category }), list),
      );
    }
  }

  /* ---------- Formulaire ---------- */

  /** Derniers dossiers choisis, par utilitaire et par champ. */
  function readFolders() {
    try {
      return JSON.parse(store.get(FOLDERS_KEY)) || {};
    } catch {
      return {};
    }
  }

  const folderKey = (utility, field) => `${utility.id}:${field.id}`;

  function rememberFolders(utility, values) {
    const folders = readFolders();
    for (const field of utility.fields) {
      if (field.kind.type === 'folder' && values[field.id]) folders[folderKey(utility, field)] = values[field.id];
    }
    store.set(FOLDERS_KEY, JSON.stringify(folders));
  }

  /** Ouvre la boîte de dialogue du système pour choisir un dossier. */
  async function browseFolder(input) {
    const dialog = tauri && tauri.dialog;
    if (!dialog) {
      announce('Le choix d’un dossier est disponible dans l’application. Saisissez le chemin à la main.');
      input.focus();
      return;
    }
    try {
      const picked = await dialog.open({
        directory: true,
        multiple: false,
        defaultPath: input.value.trim() || undefined,
        title: 'Choisir le dossier de destination',
      });
      if (typeof picked === 'string' && picked) input.value = picked;
    } catch (error) {
      showError(String(error));
    }
    input.focus();
  }

  function buildField(utility, field) {
    const id = `field-${field.id}`;
    const common = { id, name: field.id };
    const kind = field.kind.type;
    const hasDefault = field.default !== null && field.default !== undefined;
    let control;
    let wrapper = null;

    switch (kind) {
      case 'folder': {
        control = h('input', {
          ...common,
          type: 'text',
          placeholder: field.placeholder || 'Chemin du dossier',
          required: field.required,
          spellcheck: 'false',
        });
        control.value = readFolders()[folderKey(utility, field)] || (hasDefault ? field.default : '');
        const browse = h('button', {
          type: 'button',
          class: 'btn',
          'aria-describedby': id,
          onclick: () => browseFolder(control),
          text: 'Parcourir…',
        });
        wrapper = h('div', { class: 'field-row' }, control, browse);
        break;
      }
      case 'textarea':
        control = h('textarea', {
          ...common,
          rows: 8,
          placeholder: field.placeholder,
          required: field.required,
        });
        if (hasDefault) control.value = field.default;
        break;
      case 'number':
        control = h('input', {
          ...common,
          type: 'number',
          step: 'any',
          min: field.kind.min,
          max: field.kind.max,
          placeholder: field.placeholder,
          required: field.required,
        });
        if (hasDefault) control.value = field.default;
        break;
      case 'select':
        control = h(
          'select',
          common,
          ...field.kind.options.map((option) => h('option', { value: option.value, text: option.label })),
        );
        if (hasDefault) control.value = field.default;
        break;
      case 'checkbox':
        control = h('input', { ...common, type: 'checkbox' });
        control.checked = Boolean(field.default);
        break;
      default:
        control = h('input', {
          ...common,
          type: 'text',
          placeholder: field.placeholder,
          required: field.required,
        });
        if (hasDefault) control.value = field.default;
    }

    if (kind === 'checkbox') {
      return h('div', { class: 'field field-check' }, control, h('label', { for: id, text: field.label }));
    }

    const label = h('label', { for: id, text: field.label });
    if (!field.required) label.append(' ', h('span', { class: 'field-hint', text: 'facultatif' }));
    return h('div', { class: 'field' }, label, wrapper || control);
  }

  function readValues(utility) {
    const values = {};
    for (const field of utility.fields) {
      const control = els.form.elements[field.id];
      switch (field.kind.type) {
        case 'checkbox':
          values[field.id] = control.checked;
          break;
        case 'number':
          values[field.id] = control.value === '' ? null : Number(control.value);
          break;
        default:
          values[field.id] = control.value;
      }
    }
    return values;
  }

  function selectUtility(id, { focus = true } = {}) {
    const utility = state.utilities.find((u) => u.id === id);
    if (!utility) return;
    state.selectedId = id;
    store.set(LAST_KEY, id);
    renderNav();

    els.empty.hidden = true;
    els.panel.hidden = false;
    els.category.textContent = utility.category;
    els.title.textContent = utility.name;
    els.description.textContent = utility.description;
    els.fields.replaceChildren(...utility.fields.map((field) => buildField(utility, field)));
    // De retour sur l'utilitaire en cours d'exécution : on reprend l'affichage de sa barre.
    if (state.running && state.runningId === id && state.lastProgress) showProgress(state.lastProgress);
    else resetReadout();
    syncRunButton();

    if (focus) {
      const first = els.form.querySelector('input, textarea, select');
      (first || els.run).focus();
    }
  }

  /* ---------- Afficheur de résultat ---------- */

  function setCopyVisible(visible) {
    window.clearTimeout(state.copyTimer);
    els.copy.hidden = !visible;
    els.copy.textContent = COPY_LABEL;
  }

  function resetReadout() {
    state.lastText = '';
    els.readoutTitle.textContent = 'Résultat';
    els.readout.classList.remove('is-error');
    els.readout.closest('.workspace')?.classList.remove('has-grid');
    els.readoutBody.replaceChildren(h('p', { class: 'readout-idle', text: 'Le résultat s’affichera ici.' }));
    setCopyVisible(false);
  }

  /** Barre d'avancement : créée au premier message de progression, mise à jour ensuite. */
  function showProgress(update) {
    let shell = els.readoutBody.querySelector('.progress');
    if (!shell) {
      shell = h(
        'div',
        { class: 'progress' },
        h('p', { class: 'progress-percent', 'aria-hidden': 'true' }),
        h(
          'div',
          { class: 'progress-track', role: 'progressbar', 'aria-label': 'Avancement', 'aria-valuemin': '0', 'aria-valuemax': '100' },
          h('div', { class: 'progress-fill' }),
        ),
        h('p', { class: 'progress-text' }),
      );
      state.lastText = '';
      els.readoutTitle.textContent = 'Progression';
      els.readout.classList.remove('is-error');
      els.readoutBody.replaceChildren(shell);
      setCopyVisible(false);
    }

    const track = shell.querySelector('.progress-track');
    const fill = shell.querySelector('.progress-fill');
    const known = typeof update.fraction === 'number';

    if (known) {
      const percent = Math.round(Math.min(1, Math.max(0, update.fraction)) * 100);
      fill.style.width = `${percent}%`;
      track.classList.remove('is-indeterminate');
      track.setAttribute('aria-valuenow', String(percent));
      shell.querySelector('.progress-percent').replaceChildren(String(percent), h('span', { class: 'progress-unit', text: ' %' }));
      track.setAttribute('aria-valuetext', update.message ? `${percent} %, ${update.message}` : `${percent} %`);
    } else {
      fill.style.width = '';
      track.classList.add('is-indeterminate');
      track.removeAttribute('aria-valuenow');
      shell.querySelector('.progress-percent').replaceChildren(h('span', { class: 'progress-unit', text: 'En cours' }));
      track.setAttribute('aria-valuetext', update.message || 'En cours');
    }
    shell.querySelector('.progress-text').textContent = update.message || '';
  }

  function showOutput(output) {
    const parts = [];
    const nodes = [];

    if (output.table && output.table.length) {
      nodes.push(
        h(
          'dl',
          { class: 'readout-table' },
          ...output.table.map((entry) =>
            h(
              'div',
              // Une valeur longue (chemin, titre) passe sous son libellé au lieu de le recouvrir.
              { class: entry.value.length > 14 ? 'row is-long' : 'row' },
              h('dt', { text: entry.label }),
              h('dd', { text: entry.value }),
            ),
          ),
        ),
      );
      parts.push(output.table.map((entry) => `${entry.label} : ${entry.value}`).join('\n'));
    }
    const grid = output.grid;
    if (grid && grid.columns.length && grid.rows.length) {
      nodes.push(
        h(
          'div',
          // Zone focusable : le défilement horizontal doit rester utilisable au clavier.
          { class: 'readout-grid', tabindex: '0', role: 'region', 'aria-label': 'Tableau des résultats' },
          h(
            'table',
            {},
            h('thead', {}, h('tr', {}, ...grid.columns.map((name) => h('th', { scope: 'col', text: name })))),
            h(
              'tbody',
              {},
              ...grid.rows.map((row) => h('tr', {}, ...row.map((cell) => h('td', { text: cell })))),
            ),
          ),
        ),
      );
      // Copie séparée par des tabulations : se colle proprement dans un tableur.
      parts.push([grid.columns, ...grid.rows].map((row) => row.join('\t')).join('\n'));
    }
    if (output.text) {
      nodes.push(h('pre', { class: 'readout-text', text: output.text }));
      parts.push(output.text);
    }
    if (!nodes.length) nodes.push(h('p', { class: 'readout-idle', text: 'Aucun résultat à afficher.' }));

    state.lastText = parts.join('\n\n');
    els.readoutTitle.textContent = 'Résultat';
    els.readout.classList.remove('is-error');
    // Un tableau à colonnes passe sous le formulaire pour disposer de toute la largeur.
    const workspace = els.readout.closest('.workspace');
    if (workspace) workspace.classList.toggle('has-grid', nodes.some((n) => n.classList.contains('readout-grid')));
    els.readoutBody.replaceChildren(...nodes);
    setCopyVisible(Boolean(state.lastText));
    announce('Résultat disponible.');
  }

  function showError(message) {
    state.lastText = '';
    els.readoutTitle.textContent = 'Résultat';
    els.readout.classList.add('is-error');
    els.readout.closest('.workspace')?.classList.remove('has-grid');
    els.readoutBody.replaceChildren(
      h(
        'div',
        { class: 'readout-error', role: 'alert' },
        h('p', { class: 'readout-error-title', text: 'Impossible d’exécuter l’utilitaire' }),
        h('p', {}, ...softText(message)),
      ),
    );
    setCopyVisible(false);
  }

  async function onSubmit(event) {
    event.preventDefault();
    const utility = current();
    if (!utility) return;
    if (!invoke) {
      showError('Ouvrez Multitool depuis l’application pour exécuter les utilitaires.');
      return;
    }

    const runId = ++state.runId;
    const values = readValues(utility);
    state.running = true;
    state.runningId = utility.id;
    state.lastProgress = null;
    syncRunButton();
    els.readout.setAttribute('aria-busy', 'true');
    announce('Exécution en cours.');

    // Le canal reçoit l'avancement envoyé par Rust pendant que la commande tourne.
    const channel = new tauri.core.Channel();
    channel.onmessage = (update) => {
      // Un message tardif d'une exécution terminée ne doit pas rouvrir la barre.
      if (!state.running || state.runId !== runId) return;
      state.lastProgress = update;
      if (state.selectedId === utility.id) showProgress(update);
    };

    let outcome;
    try {
      outcome = { output: await invoke('run_utility', { id: utility.id, input: values, onProgress: channel }) };
    } catch (error) {
      outcome = { error: String(error) };
    }

    state.running = false;
    state.runningId = null;
    state.lastProgress = null;
    els.readout.removeAttribute('aria-busy');
    syncRunButton();

    if (outcome.output) rememberFolders(utility, values);
    if (state.selectedId !== utility.id) {
      // L'utilisateur a changé d'utilitaire pendant l'exécution : on le prévient sans écraser son écran.
      announce(`${utility.name} : ${outcome.error ? 'échec' : 'terminé'}.`);
      return;
    }
    if (outcome.error) showError(outcome.error);
    else showOutput(outcome.output);
  }

  /** Le bouton reflète l'exécution en cours (une seule à la fois). */
  function syncRunButton() {
    els.run.disabled = state.running;
    if (!state.running) els.run.textContent = RUN_LABEL;
    else if (state.runningId === state.selectedId) els.run.textContent = 'En cours…';
    else els.run.textContent = 'Un autre utilitaire est en cours…';
  }

  async function writeClipboard(text) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      /* repli ci-dessous */
    }
    const area = h('textarea', { class: 'sr-only', 'aria-hidden': 'true' });
    area.value = text;
    document.body.append(area);
    area.select();
    let ok = false;
    try {
      ok = document.execCommand('copy');
    } catch {
      ok = false;
    }
    area.remove();
    return ok;
  }

  async function onCopy() {
    const ok = await writeClipboard(state.lastText);
    els.copy.textContent = ok ? 'Copié' : 'Copie impossible';
    announce(ok ? 'Résultat copié dans le presse-papiers.' : 'La copie dans le presse-papiers a échoué.');
    window.clearTimeout(state.copyTimer);
    state.copyTimer = window.setTimeout(() => (els.copy.textContent = COPY_LABEL), 2000);
  }

  /* ---------- Thème ---------- */

  function syncThemeControls() {
    els.themeSelect.value = theme.theme;
    els.contrast.setAttribute('aria-pressed', String(theme.contrast));
  }

  /* ---------- Démarrage ---------- */

  function showBanner(message) {
    els.banner.textContent = message;
    els.banner.hidden = false;
  }

  async function init() {
    els.themeSelect.addEventListener('change', (event) => theme.setTheme(event.target.value));
    els.contrast.addEventListener('click', () => theme.setContrast(!theme.contrast));
    document.addEventListener('themechange', syncThemeControls);
    syncThemeControls();

    els.search.addEventListener('input', renderNav);
    els.search.addEventListener('keydown', (event) => {
      if (event.key === 'Escape' && els.search.value) {
        els.search.value = '';
        renderNav();
      }
    });
    els.form.addEventListener('submit', onSubmit);
    els.copy.addEventListener('click', onCopy);

    document.addEventListener('keydown', (event) => {
      const mod = event.ctrlKey || event.metaKey;
      if (mod && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        els.search.focus();
        els.search.select();
      } else if (mod && event.key === 'Enter' && els.form.contains(document.activeElement)) {
        event.preventDefault();
        els.form.requestSubmit();
      }
    });

    if (!invoke) {
      showBanner('Aperçu hors application : lancez « npm run dev » pour utiliser les utilitaires.');
      renderNav();
      return;
    }

    try {
      state.utilities = await invoke('list_utilities');
    } catch (error) {
      showBanner(`Impossible de charger les utilitaires : ${error}`);
    }
    renderNav();

    const last = store.get(LAST_KEY);
    if (last && state.utilities.some((u) => u.id === last)) selectUtility(last, { focus: false });
  }

  init();
})();

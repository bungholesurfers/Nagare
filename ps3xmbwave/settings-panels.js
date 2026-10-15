'use strict';
// The settings panel: one panel whose tabs and sections each settings file's `*_SETTINGS_LAYOUT` lays out, built by
// introspecting the settings objects into sliders, selects - a radio pair for the two-valued ones - and Reset buttons,
// each label carrying its meta's `help` as a tooltip, and a meta's `hidden`, the reason why, keeping a setting off it -
// which marks what was changed from the panel, lets it be locked, and marks what something else set off its default.
// Fed by `spline-settings.js` and `particles-settings.js`, created from `index.html`, which calls the returned
// `refresh()` when a theme or a sequence rewrites the settings and hands them the `locked` ones.

(function () {
  // The padlock beside a value changed by hand: open, the theme and the sequences may still move it; closed, they
  // leave it alone.
  const LOCK_OPEN = [
    '<svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">',
    '<rect x="3" y="7" width="10" height="7" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.4"/>',
    '<path d="M5 7V5a3 3 0 0 1 5.8-1.1" fill="none" stroke="currentColor" stroke-width="1.6"/></svg>',
  ].join('');
  const LOCK_CLOSED = [
    '<svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">',
    '<rect x="3" y="7" width="10" height="7" rx="1.5" fill="currentColor"/>',
    '<path d="M5 7V5a3 3 0 0 1 6 0v2" fill="none" stroke="currentColor" stroke-width="1.6"/></svg>',
  ].join('');
  // Where the tab last chosen is kept, in the viewer's own browser; without it the panel opens on its first tab.
  const TAB_STORE_KEY = 'ps3xmb.settingsTab';

  function decimalsFromStep(step) {
    const s = String(step);
    const idx = s.indexOf('.');
    return idx === -1 ? 0 : Math.min(6, s.length - idx - 1);
  }

  function humanizeKey(key) {
    return key.replace(/([A-Z])/g, ' $1').replace(/^./, function (c) { return c.toUpperCase(); });
  }

  // A select's options as { value, label }, each value of the type its meta gives it.
  function choices(meta) {
    return (Array.isArray(meta.options) ? meta.options : []).map(function (o) {
      return o && typeof o === 'object'
        ? { value: o.value, label: String(o.label != null ? o.label : o.value) }
        : { value: o, label: String(o) };
    });
  }

  function inferMeta(value) {
    const abs = Math.max(Math.abs(value), 1);
    return { min: value - abs * 2, max: value + abs * 2, step: abs / 200 };
  }

  function setPosition(el, pos) {
    if (!pos) return;
    if (pos.top != null) el.style.top = String(pos.top);
    if (pos.right != null) el.style.right = String(pos.right);
    if (pos.bottom != null) el.style.bottom = String(pos.bottom);
    if (pos.left != null) el.style.left = String(pos.left);
  }

  function getUiLayerRoot() {
    let root = document.getElementById('ui-layer-root');
    if (root) return root;

    root = document.createElement('div');
    root.id = 'ui-layer-root';
    root.className = 'ui-layer-root';
    document.body.appendChild(root);
    return root;
  }

  function readStoredTab() {
    try { return window.localStorage.getItem(TAB_STORE_KEY); } catch (e) { return null; }
  }

  function storeTab(name) {
    try { window.localStorage.setItem(TAB_STORE_KEY, name); } catch (e) { /* the choice is not kept, nothing more */ }
  }

  // options: id, title, position and showPosition, hideTitle (the Hide button's tooltip), tabs (their names, in
  // order), and sources, by name: { settings, meta, layout, lockable, defaultTab } for each settings object. A layout
  // lists { tab, section, keys }; a key it leaves out lands in an Other section of its source's defaultTab.
  window.createSettingsPanel = function createSettingsPanel(options) {
    const id = options && options.id;
    const title = options && options.title;
    const sources = (options && options.sources) || {};
    const panelPos = options && options.position;
    const showPos = options && options.showPosition;

    if (!id || !title || !Object.keys(sources).length) return;
    if (document.getElementById(id)) return;

    const panel = document.createElement('div');
    panel.id = id;
    panel.className = 'settings-panel';
    setPosition(panel, panelPos);

    const header = document.createElement('div');
    header.className = 'settings-panel-header';

    const titleEl = document.createElement('div');
    titleEl.className = 'settings-panel-title';
    titleEl.textContent = title;

    const countEl = document.createElement('span');
    countEl.className = 'settings-panel-count';
    titleEl.appendChild(countEl);

    const hideBtn = document.createElement('button');
    hideBtn.className = 'settings-btn';
    hideBtn.type = 'button';
    hideBtn.textContent = 'Hide';
    if (options.hideTitle) hideBtn.title = options.hideTitle;

    const tabBar = document.createElement('div');
    tabBar.className = 'settings-tabs';
    tabBar.setAttribute('role', 'tablist');

    const list = document.createElement('div');
    list.className = 'settings-panel-list';

    const showBtn = document.createElement('button');
    showBtn.id = id + '-show';
    showBtn.className = 'settings-btn settings-show-btn';
    showBtn.type = 'button';
    setPosition(showBtn, showPos || panelPos);

    // The tabs, by name: the button, its count of the rows changed here, the page, and the page's sections by name,
    // each made when its first row arrives.
    const tabs = {};
    const tabOrder = [];

    function tab(name) {
      if (tabs[name]) return tabs[name];
      const button = document.createElement('button');
      button.className = 'settings-tab';
      button.type = 'button';
      button.setAttribute('role', 'tab');
      button.textContent = name;
      const badge = document.createElement('span');
      badge.className = 'settings-tab-count';
      badge.hidden = true;
      button.appendChild(badge);
      const page = document.createElement('div');
      page.className = 'settings-tab-page';
      page.setAttribute('role', 'tabpanel');
      page.hidden = true;
      button.addEventListener('click', function () {
        select(name);
        storeTab(name);
      });
      tabBar.appendChild(button);
      list.appendChild(page);
      tabs[name] = { button: button, badge: badge, page: page, sections: {} };
      tabOrder.push(name);
      return tabs[name];
    }

    function section(tabName, name) {
      const t = tab(tabName);
      if (t.sections[name]) return t.sections[name];
      const heading = document.createElement('div');
      heading.className = 'settings-section-title';
      heading.textContent = name;
      const body = document.createElement('div');
      body.className = 'settings-section';
      t.page.appendChild(heading);
      t.page.appendChild(body);
      t.sections[name] = body;
      return body;
    }

    function select(name) {
      if (!tabs[name]) return;
      tabOrder.forEach(function (n) {
        const on = n === name;
        tabs[n].page.hidden = !on;
        tabs[n].button.setAttribute('aria-selected', String(on));
      });
      list.scrollTop = 0;
    }

    ((options && options.tabs) || []).forEach(tab);

    // The header counts the marked rows of every source, with a pill for each colour; each tab counts the rows changed
    // here among its own, and the button that brings the hidden panel back counts them all.
    const bindings = {};
    const externalPills = {};
    function showCounts() {
      let n = 0;
      const external = {};
      const perTab = {};
      Object.keys(bindings).forEach(function (name) {
        const b = bindings[name];
        n += b.modified.size;
        b.modified.forEach(function (key) { perTab[b.tabOf[key]] = (perTab[b.tabOf[key]] || 0) + 1; });
        Object.keys(b.external).forEach(function (writer) {
          external[writer] = (external[writer] || 0) + b.external[writer].size;
        });
      });
      countEl.textContent = n + ' modified';
      countEl.hidden = n === 0;
      showBtn.textContent = 'Show ' + title + (n ? ' (' + n + ' modified)' : '');
      Object.keys(external).forEach(function (writer) {
        let pill = externalPills[writer];
        if (!pill) {
          pill = document.createElement('span');
          pill.className = 'settings-panel-count settings-panel-count-external';
          pill.dataset.source = writer;
          titleEl.appendChild(pill);
          externalPills[writer] = pill;
        }
        pill.textContent = external[writer] + ' by ' + writer;
        pill.hidden = external[writer] === 0;
      });
      tabOrder.forEach(function (name) {
        const k = perTab[name] || 0;
        tabs[name].badge.textContent = String(k);
        tabs[name].badge.hidden = k === 0;
      });
    }

    hideBtn.addEventListener('click', function () {
      panel.classList.add('hidden');
      showBtn.style.display = 'block';
    });

    showBtn.addEventListener('click', function () {
      panel.classList.remove('hidden');
      showBtn.style.display = 'none';
    });

    // One settings object: its rows, wherever its layout puts them, and its own record of what was changed here and
    // what something else wrote.
    function bindSource(sourceOptions, sourceName) {
      const settings = sourceOptions.settings;
      const metaMap = sourceOptions.meta || {};
      const layout = sourceOptions.layout || [];
      const defaultTab = sourceOptions.defaultTab || tabOrder[0] || 'Settings';

      // Each setting's default: its value when the panel was made, which `index.html` does before any theme is
      // applied.
      const base = Object.assign({}, settings);
      // What Reset returns to, setting by setting: the value the panel was made with, or the last one something other
      // than the panel wrote (see refresh). A row whose setting differs from it has been changed here, and is marked.
      const initial = Object.assign({}, settings);
      // Each setting as the panel last left it, which tells refresh() what something else has written since.
      const known = Object.assign({}, settings);
      // Who writes the settings from outside the panel now, as refresh() was last told: the rows it has set off their
      // defaults take its colour.
      let writer = null;
      const rows = [];
      const modified = new Set();
      const external = {}; // for each writer, the settings it has set off their defaults
      const tabOf = {}; // each shown setting's tab
      // The settings something else can write, whose value, once changed here, can be locked against it; and the ones
      // locked, which the caller hands to that writer.
      const lockable = new Set(sourceOptions.lockable || []);
      const locked = new Set();

      // Where each setting goes: the layout's order first, then whatever it leaves out, in the settings' own order.
      const placed = [];
      const laidOut = new Set();
      layout.forEach(function (entry) {
        (entry.keys || []).forEach(function (key) {
          if (laidOut.has(key)) return;
          laidOut.add(key);
          placed.push({ key: key, tab: entry.tab || defaultTab, section: entry.section || 'Other' });
        });
      });
      Object.keys(settings).forEach(function (key) {
        if (!laidOut.has(key)) placed.push({ key: key, tab: defaultTab, section: 'Other' });
      });

      placed.forEach(function (place) {
        const key = place.key;
        if (!(key in settings)) return;
        const value = settings[key];
        const meta = metaMap[key] || {};
        if (meta.hidden) return; // kept in the settings, off the panel, for the reason it gives
        const isSelect = meta.type === 'select';
        const isNumber = typeof value === 'number' && Number.isFinite(value);
        if (!isSelect && !isNumber) return;
        const opts = isSelect ? choices(meta) : [];
        const isPair = opts.length === 2; // two values: a pair of radio buttons, one click to change

        const row = document.createElement('div');
        row.className = 'settings-row';

        const left = document.createElement('div');
        const label = document.createElement('div');
        label.className = 'settings-label';
        label.textContent = humanizeKey(key);
        if (meta.help) label.title = meta.help;

        const controls = document.createElement('div');
        controls.className = 'settings-controls';

        const resetBtn = document.createElement('button');
        resetBtn.className = 'settings-btn settings-reset';
        resetBtn.type = 'button';
        resetBtn.textContent = 'Reset';

        let lockBtn = null;
        if (lockable.has(key)) {
          lockBtn = document.createElement('button');
          lockBtn.className = 'settings-btn settings-lock';
          lockBtn.type = 'button';
          lockBtn.setAttribute('aria-label', 'Lock ' + humanizeKey(key));
          lockBtn.addEventListener('click', function () {
            if (locked.has(key)) locked.delete(key);
            else locked.add(key);
            mark();
          });
        }

        let sync; // brings the control up to the setting
        let describe; // a value as the control would show it, for the tooltips
        let valueView; // what shows the value, and carries its tooltip: the number beside a slider, the select, the pair

        // A select writes strings, and a radio pair its option's own value, whatever type the setting started as, so
        // both compare as text. Numbers are the same within a billionth: the end of a theme's blend, a + (b - a) x 1,
        // can land a hair off b.
        function same(a, b) {
          if (isSelect) return String(a) === String(b);
          return Math.abs(a - b) <= 1e-9 * Math.max(Math.abs(a), Math.abs(b));
        }

        // Amber if the value was changed here; if not, in the writer's colour where something else set it off its
        // default.
        function mark() {
          const mine = !same(settings[key], initial[key]);
          const from = !mine && !same(initial[key], base[key]) ? writer || 'outside' : null;
          row.classList.toggle('settings-row-modified', mine);
          row.classList.toggle('settings-row-external', from !== null);
          if (from) row.dataset.source = from;
          else delete row.dataset.source;
          resetBtn.disabled = !mine;
          resetBtn.title = mine ? 'Back to ' + describe(initial[key]) : '';
          if (lockBtn) {
            if (!mine) locked.delete(key); // the lock goes with the mark
            const on = locked.has(key);
            lockBtn.hidden = !mine;
            lockBtn.setAttribute('aria-pressed', String(on));
            if (lockBtn.dataset.on !== String(on)) {
              lockBtn.dataset.on = String(on);
              lockBtn.innerHTML = on ? LOCK_CLOSED : LOCK_OPEN;
            }
            lockBtn.title = on
              ? 'Locked: the theme and the sequences leave it alone'
              : 'Lock it, and the theme and the sequences will leave it alone';
          }
          if (mine) valueView.title = 'Changed from ' + describe(initial[key]);
          else valueView.title = from ? 'Default ' + describe(base[key]) : '';
          if (mine) modified.add(key);
          else modified.delete(key);
          Object.keys(external).forEach(function (name) { external[name].delete(key); });
          if (from) {
            if (!external[from]) external[from] = new Set();
            external[from].add(key);
          }
        }

        // After the panel itself writes the setting.
        function written() {
          known[key] = settings[key];
          mark();
          showCounts();
        }

        if (isPair) {
          const group = document.createElement('div');
          group.className = 'settings-radios';
          group.setAttribute('role', 'radiogroup');
          group.setAttribute('aria-label', humanizeKey(key));
          const radios = opts.map(function (choice) {
            const item = document.createElement('label');
            item.className = 'settings-radio';
            const input = document.createElement('input');
            input.type = 'radio';
            input.name = id + '-' + sourceName + '-' + key;
            input.value = String(choice.value);
            input.addEventListener('change', function () {
              if (!input.checked) return;
              settings[key] = choice.value;
              written();
            });
            item.appendChild(input);
            item.appendChild(document.createTextNode(choice.label));
            group.appendChild(item);
            return input;
          });

          sync = function () {
            radios.forEach(function (input) { input.checked = input.value === String(settings[key]); });
          };
          sync();

          resetBtn.addEventListener('click', function () {
            settings[key] = initial[key];
            sync();
            written();
          });

          valueView = group;
          describe = function (v) {
            const choice = opts.find(function (c) { return String(c.value) === String(v); });
            return choice ? choice.label : String(v);
          };
          controls.appendChild(group);
        } else if (isSelect) {
          const select = document.createElement('select');
          select.className = 'settings-select';
          opts.forEach(function (choice) {
            const opt = document.createElement('option');
            opt.value = String(choice.value);
            opt.textContent = choice.label;
            select.appendChild(opt);
          });
          select.value = String(value);

          select.addEventListener('change', function () {
            settings[key] = select.value;
            written();
          });

          resetBtn.addEventListener('click', function () {
            settings[key] = initial[key];
            select.value = String(initial[key]);
            written();
          });

          sync = function () { select.value = String(settings[key]); };
          valueView = select;
          describe = function (v) {
            const opt = Array.prototype.find.call(select.options, function (o) { return o.value === String(v); });
            return opt ? opt.textContent : String(v);
          };
          controls.appendChild(select);
        } else {
          const numMeta = metaMap[key] || inferMeta(value);
          const decimals = Number.isFinite(numMeta.decimals) ? numMeta.decimals : decimalsFromStep(numMeta.step);

          const slider = document.createElement('input');
          slider.className = 'settings-slider';
          slider.type = 'range';
          slider.min = String(numMeta.min);
          slider.max = String(numMeta.max);
          slider.step = String(numMeta.step);
          slider.value = String(value);

          const valueEl = document.createElement('span');
          valueEl.className = 'settings-value';
          function formatValue() {
            return Number(settings[key]).toFixed(decimals);
          }
          valueEl.textContent = formatValue();

          slider.addEventListener('input', function () {
            settings[key] = parseFloat(slider.value);
            valueEl.textContent = formatValue();
            written();
          });

          resetBtn.addEventListener('click', function () {
            settings[key] = initial[key];
            slider.value = String(initial[key]);
            valueEl.textContent = formatValue();
            written();
          });

          sync = function () {
            slider.value = String(settings[key]);
            valueEl.textContent = formatValue();
          };
          valueView = valueEl;
          // In full, since the slider's step can round away what tells the two apart.
          describe = function (v) { return String(Number(Number(v).toPrecision(6))); };
          controls.appendChild(slider);
          controls.appendChild(valueEl);
        }

        left.appendChild(label);
        left.appendChild(controls);

        const actions = document.createElement('div');
        actions.className = 'settings-actions';
        if (lockBtn) actions.appendChild(lockBtn);
        actions.appendChild(resetBtn);

        row.appendChild(left);
        row.appendChild(actions);
        section(place.tab, place.section).appendChild(row);
        tabOf[key] = place.tab;
        rows.push({ key: key, sync: sync, mark: mark });
        mark();
      });

      return {
        modified: modified,
        external: external,
        tabOf: tabOf,
        locked: locked,
        // After something other than the panel writes into the settings, naming it (`source`, say 'theme'): the
        // controls of the settings it wrote catch up, and the values they land on become what Reset returns to, in
        // its colour where they differ from the defaults. A setting changed from the panel and not written since keeps
        // its mark, and Reset still returns it to what it was. When the writer changes, every row set off its default
        // takes the new one's colour: here the theme and the sequences write the same settings, often to the same
        // values.
        refresh: function refresh(source) {
          const handover = (source || null) !== writer;
          writer = source || null;
          rows.forEach(function (r) {
            const changed = settings[r.key] !== known[r.key];
            if (changed) {
              known[r.key] = settings[r.key];
              initial[r.key] = settings[r.key];
              r.sync();
            }
            if (changed || handover) r.mark();
          });
        },
      };
    }

    const locked = {};
    Object.keys(sources).forEach(function (name) {
      const src = sources[name];
      if (!src || !src.settings || typeof src.settings !== 'object') return;
      bindings[name] = bindSource(src, name);
      locked[name] = bindings[name].locked;
    });

    // Tabs left empty - every setting in them hidden - go.
    tabOrder.slice().forEach(function (name) {
      if (Object.keys(tabs[name].sections).length) return;
      tabs[name].button.remove();
      tabs[name].page.remove();
      delete tabs[name];
      tabOrder.splice(tabOrder.indexOf(name), 1);
    });

    const stored = readStoredTab();
    select(stored && tabs[stored] ? stored : tabOrder[0]);

    header.appendChild(titleEl);
    header.appendChild(hideBtn);
    panel.appendChild(header);
    panel.appendChild(tabBar);
    panel.appendChild(list);
    showCounts();

    const root = getUiLayerRoot();
    const host = document.createElement('div');
    host.id = id + '-layer';
    host.className = 'settings-layer-host';
    host.appendChild(panel);
    host.appendChild(showBtn);
    root.appendChild(host);

    return {
      // Call after something other than the panel writes into one source's settings, naming the source and the writer.
      refresh: function refresh(sourceName, writerName) {
        const b = bindings[sourceName];
        if (!b) return;
        b.refresh(writerName);
        showCounts();
      },
      // The settings, by source, whose value was changed here and locked, for whatever else writes them to leave alone.
      // Live Sets: a lock goes when its value is Reset or brought back by hand.
      locked: locked,
    };
  };
})();

document.documentElement.classList.remove('no-js');
const REPO = 'cpstest-tech/Venturi';
const RELEASES = `https://github.com/${REPO}/releases/latest/download/`;
const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;

// Screenshots: each placeholder swaps to the real image as soon as the file exists.
for (const fig of document.querySelectorAll('.shot[data-shot]')) {
  if (!fig.querySelector('.faux')) {
    fig.insertAdjacentHTML('beforeend', `<div class="ph"><div class="ph-inner">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="10" r="2"/><path d="m21 16-5-5-9 9"/></svg>
      <b>${fig.dataset.label}</b><code>${fig.dataset.shot}</code></div></div>`);
  }
  const img = new Image();
  img.alt = `Tandem: ${fig.dataset.label}`;
  img.decoding = 'async';
  img.onload = () => { fig.prepend(img); requestAnimationFrame(() => fig.classList.add('has-img')); };
  img.src = fig.dataset.shot;
}

// Reveal on scroll, staggered within each batch.
const io = new IntersectionObserver((entries) => {
  let i = 0;
  for (const e of entries) {
    if (!e.isIntersecting) continue;
    e.target.style.setProperty('--d', `${Math.min(i++ * 0.08, 0.4)}s`);
    e.target.classList.add('in');
    io.unobserve(e.target);
  }
}, { threshold: 0.12, rootMargin: '0px 0px -40px 0px' });
document.querySelectorAll('.reveal').forEach((el) => io.observe(el));

// Nav background once scrolled.
const nav = document.getElementById('nav');
const onScroll = () => nav.classList.toggle('scrolled', scrollY > 20);
addEventListener('scroll', onScroll, { passive: true });
onScroll();

// Hero window flattens as you scroll and follows the pointer.
const win = document.querySelector('.window');
const stage = document.getElementById('stage');
if (win && !reduceMotion && matchMedia('(min-width: 1081px)').matches) {
  let px = 0, py = 0;
  const apply = () => {
    const k = Math.max(0, 1 - scrollY / 600);
    win.style.transform = `rotateY(${(-14 + px * 8) * k}deg) rotateX(${(6 - py * 6) * k}deg) rotateZ(${k}deg)`;
  };
  stage.addEventListener('pointermove', (e) => {
    const r = stage.getBoundingClientRect();
    px = (e.clientX - r.left) / r.width - 0.5;
    py = (e.clientY - r.top) / r.height - 0.5;
    apply();
  });
  stage.addEventListener('pointerleave', () => { px = py = 0; apply(); });
  addEventListener('scroll', apply, { passive: true });
}

// Spotlight on cards.
for (const card of document.querySelectorAll('.card')) {
  card.addEventListener('pointermove', (e) => {
    const r = card.getBoundingClientRect();
    card.style.setProperty('--mx', `${e.clientX - r.left}px`);
    card.style.setProperty('--my', `${e.clientY - r.top}px`);
  });
}

document.querySelectorAll('.eq i').forEach((bar, i) => {
  const base = 45 + 40 * Math.sin(i * 0.55) * Math.cos(i * 0.21);
  bar.style.setProperty('--h', `${Math.max(25, Math.min(98, base + Math.random() * 20))}%`);
  bar.style.animationDelay = `${-Math.random() * 1.5}s`;
});

// Count-up stats.
const fmt = new Intl.NumberFormat('en-US');
const countIo = new IntersectionObserver((entries) => {
  for (const e of entries) {
    if (!e.isIntersecting) continue;
    countIo.unobserve(e.target);
    const el = e.target, target = +el.dataset.count, suffix = el.dataset.suffix || '';
    if (reduceMotion) continue;
    const t0 = performance.now(), dur = 1600;
    const tick = (t) => {
      const p = Math.min(1, (t - t0) / dur), eased = 1 - Math.pow(1 - p, 4);
      el.textContent = fmt.format(Math.round(target * eased)) + suffix;
      if (p < 1) requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  }
}, { threshold: 0.6 });
document.querySelectorAll('[data-count]').forEach((el) => countIo.observe(el));

// Gallery tabs, auto-advancing until the user picks one.
const tabs = [...document.querySelectorAll('.tabs button')];
const slides = [...document.querySelectorAll('.shot-big')];
let current = 0, autoplay = !reduceMotion;
const show = (i) => {
  current = i;
  tabs.forEach((t, j) => t.setAttribute('aria-selected', String(j === i)));
  slides.forEach((s, j) => s.classList.toggle('active', j === i));
};
tabs.forEach((t, i) => t.addEventListener('click', () => { autoplay = false; show(i); }));
setInterval(() => { if (autoplay && !document.hidden) show((current + 1) % slides.length); }, 4500);

// Copy install command.
for (const btn of document.querySelectorAll('.copy')) {
  btn.addEventListener('click', async () => {
    const text = btn.parentElement.querySelector('code').textContent;
    try { await navigator.clipboard.writeText(text); } catch { return; }
    btn.classList.add('done');
    setTimeout(() => btn.classList.remove('done'), 1800);
  });
}

// Platform detection.
const ua = navigator.userAgent;
const os = /Windows/i.test(ua) ? 'windows'
  : /Mac OS X|Macintosh/i.test(ua) && !/iPhone|iPad/i.test(ua) ? 'mac'
  : /Linux|X11/i.test(ua) && !/Android/i.test(ua) ? 'linux' : null;
const heroBtn = document.getElementById('hero-download');
const OS_UI = {
  linux: { label: 'Download for Linux', icon: '#i-linux', href: '#download' },
  mac: { label: 'Download for macOS', icon: '#i-apple', href: `${RELEASES}Tandem-arm64.dmg` },
  windows: { label: 'Windows · coming soon', icon: '#i-windows', href: '#download' },
};
const setHero = (key) => {
  const ui = OS_UI[key];
  if (!ui) return;
  heroBtn.querySelector('span').textContent = ui.label;
  heroBtn.querySelector('use').setAttribute('href', ui.icon);
  heroBtn.href = ui.href;
};
if (os) {
  setHero(os);
  document.querySelector(`.dl[data-os="${os}"]`)?.classList.add('mine');
}

// Latest release: show the version and switch on Windows the moment its build is published.
fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers: { Accept: 'application/vnd.github+json' } })
  .then((r) => (r.ok ? r.json() : Promise.reject()))
  .then((rel) => {
    const tag = rel.tag_name;
    document.querySelectorAll('[data-version]').forEach((el) => { el.textContent = tag; });
    document.querySelectorAll('[data-version-text]').forEach((el) => { el.textContent = `Tandem ${tag} is out`; });
    const names = new Set((rel.assets || []).map((a) => a.name));
    const card = document.getElementById('dl-windows');
    let any = false;
    for (const [sel, file] of [['.win-setup', 'Tandem-windows-x64-setup.exe'], ['.win-zip', 'Tandem-windows-x64.zip']]) {
      if (names.has(file)) { card.querySelector(sel).removeAttribute('aria-disabled'); any = true; }
    }
    if (any) {
      card.classList.remove('dl-soon');
      card.classList.add('dl-ready');
      card.querySelector('.dl-note').textContent = 'Native Windows build with NVENC export. Installer or portable zip.';
      document.querySelector('.os-row .soon i')?.remove();
      OS_UI.windows = { label: 'Download for Windows', icon: '#i-windows', href: `${RELEASES}Tandem-windows-x64-setup.exe` };
      if (os === 'windows') setHero('windows');
    }
  })
  .catch(() => {});

// Agent terminal: replays the pause-removal flow from docs/MCP.md.
const SCRIPT = [
  ['u', '› remove every pause longer than half a second from talk.mp4\n\n'],
  ['c', '# importing the recording\n'],
  ['t', 'import_media'], ['', '({ '], ['k', 'paths'], ['', ': ['], ['s', '"talk.mp4"'], ['', '] })\n'],
  ['o', '  ✓ media m_01 · 25 fps · 02:31\n'],
  ['t', 'create_timeline'], ['', '({ '], ['k', 'from_media'], ['', ': '], ['s', '"m_01"'], ['', ' })\n'],
  ['t', 'insert_clip'], ['', '({ '], ['k', 'at'], ['', ': 0 })\n\n'],
  ['c', '# measuring the audio, one RMS value per frame\n'],
  ['t', 'get_audio_levels'], ['', '({ '], ['k', 'media_id'], ['', ': '], ['s', '"m_01"'], ['', ' })\n'],
  ['o', '  rms_db: [-21.3, -21.4, … -120.0, -120.0, … -21.2]\n\n'],
  ['c', '# pause = below -50 dB for 12+ frames, keep the edges\n'],
  ['t', 'delete_ranges'], ['', '({\n  '], ['k', 'ranges'], ['', ': [[27, 60], [102, 123], …],\n  '], ['k', 'ripple'], ['', ': true\n})\n'],
  ['o', '  ✓ 14 pauses removed · one undo step · A/V in sync\n\n'],
  ['t', 'render_frame'], ['', '({ '], ['k', 'at'], ['', ': 27 })   '], ['c', '# check the cut\n'],
  ['t', 'export'], ['', '({ '], ['k', 'path'], ['', ': '], ['s', '"talk_cut.mp4"'], ['', ' })\n'],
  ['o', '  ✓ done\n'],
];
const code = document.getElementById('term-code');
const esc = (s) => s.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' })[c]);
const renderAll = () => { code.innerHTML = SCRIPT.map(([c, t]) => (c ? `<span class="${c}">${esc(t)}</span>` : esc(t))).join(''); };

async function typeTerminal() {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  for (;;) {
    code.innerHTML = '';
    for (const [cls, text] of SCRIPT) {
      const span = document.createElement('span');
      if (cls) span.className = cls;
      code.append(span);
      const instant = cls === 'o';
      if (instant) { await sleep(380); span.textContent = text; continue; }
      for (const ch of text) {
        span.textContent += ch;
        await sleep(cls === 'u' ? 28 : 11);
      }
      if (text.endsWith('\n')) await sleep(140);
    }
    await sleep(5000);
  }
}
if (reduceMotion) renderAll();
else {
  const termIo = new IntersectionObserver(([e]) => {
    if (e.isIntersecting) { termIo.disconnect(); typeTerminal(); }
  }, { threshold: 0.3 });
  termIo.observe(document.getElementById('term'));
}

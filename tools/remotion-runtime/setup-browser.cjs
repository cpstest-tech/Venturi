const fs = require('node:fs');
const path = require('node:path');
const {ensureBrowser} = require('@remotion/renderer');
(async () => {
  const packages = path.join(__dirname, 'node_modules', '@remotion');
  for (const name of fs.readdirSync(packages).filter(name => name.startsWith('compositor-'))) {
    for (const executable of ['remotion', 'ffmpeg', 'ffprobe']) {
      const file = path.join(packages, name, executable);
      if (fs.existsSync(file)) fs.chmodSync(file, 0o755);
    }
  }
  const browser = await ensureBrowser({chromeMode: 'headless-shell'});
  require('node:child_process').execFileSync(browser.path, ['--version']);
  console.log('Runtime directory:', process.cwd());
  console.log('Node executable:', process.execPath);
  console.log('Chromium executable:', browser.path);
})().catch(error => { console.error(error); process.exit(1); });

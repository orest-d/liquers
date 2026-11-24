const { chromium } = require('playwright');
const fs = require('fs'), path = require('path');
(async () => {
  const b = await chromium.launch();
  const dir = path.join(__dirname, 'html'), out = path.join(__dirname, '..');
  for (const f of fs.readdirSync(dir).filter(f => f.endsWith('.html'))) {
    const p = await b.newPage({ viewport: { width: 1300, height: 700 }, deviceScaleFactor: Number(process.argv[2] || 1) });
    await p.goto('file://' + path.join(dir, f));
    await p.screenshot({ path: path.join(out, f.replace('.html', '.png')), fullPage: true });
    await p.close();
  }
  await b.close();
})();

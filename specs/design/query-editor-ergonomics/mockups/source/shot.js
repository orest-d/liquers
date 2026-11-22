const { chromium } = require('playwright');
const fs = require('fs'), path = require('path');
(async () => {
  const b = await chromium.launch();
  const dir = path.join(__dirname, 'html'), out = path.join(__dirname, 'png');
  fs.mkdirSync(out, { recursive: true });
  const scale = Number(process.argv[2] || 1);
  for (const f of fs.readdirSync(dir).filter(f => f.endsWith('.html'))) {
    const p = await b.newPage({ viewport: { width: 1300, height: 800 }, deviceScaleFactor: scale });
    await p.goto('file://' + path.join(dir, f));
    const w = await p.evaluate(() => document.body.getBoundingClientRect().width + 56);
    await p.setViewportSize({ width: Math.ceil(w), height: 800 });
    await p.screenshot({ path: path.join(out, f.replace('.html', '.png')), fullPage: true });
    await p.close();
  }
  await b.close();
})();

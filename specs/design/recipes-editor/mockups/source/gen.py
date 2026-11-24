"""Generates the recipes-editor mockups as HTML pages (rendered to PNG by shot.js)."""
import os

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "html")
os.makedirs(OUT, exist_ok=True)

CSS = r"""
:root{--bg:#121316;--panel:#16171b;--card:#1b1c21;--head:#202127;--ctl:#26272d;--ctl2:#2e3037;
--ink:#f2f1ee;--ink2:#a9abb3;--ink3:#6c6f79;--line:rgba(255,255,255,.07);--line2:rgba(255,255,255,.12);
--accent:#2d81ff;--wash:rgba(45,129,255,.16);--danger:#e2475a;--ok:#3fb27f;--warn:#e0a23a;
--q-key:#8ec5ff;--q-act:#f2f1ee;--q-par:#f0b37e;--q-sep:#5d616b;--q-ns:#c49bff;--q-link:#7fd6a1;
--y-key:#8ec5ff;--y-str:#cfd3da;--y-punct:#5d616b;--y-com:#6c6f79;--y-bool:#f0b37e}
*{box-sizing:border-box}
html,body{margin:0;background:var(--bg);color:var(--ink);font:13px/1.45 Inter,system-ui,sans-serif}
body{padding:28px;width:1240px}
.mono{font-family:"DejaVu Sans Mono",ui-monospace,monospace;font-size:12.5px}
h1{font:600 20px/1.2 Inter,sans-serif;margin:0 0 4px}
.sub{color:var(--ink2);margin:0 0 18px}
.row{display:flex;gap:22px;align-items:flex-start}
.win{background:var(--panel);border:1px solid var(--line2);border-radius:10px;overflow:hidden;box-shadow:0 20px 50px rgba(0,0,0,.35)}
.tb{display:flex;align-items:center;gap:10px;padding:9px 12px;border-bottom:1px solid var(--line);background:var(--bg);white-space:nowrap}
.tb b{font-weight:600}.sp{flex:1}.dim{color:var(--ink3)}.ink2{color:var(--ink2)}
.seg{display:inline-flex;background:var(--ctl);border:1px solid var(--line);border-radius:7px;padding:2px}
.seg span{padding:3px 10px;border-radius:5px;color:var(--ink2);font-size:12px}
.seg span.on{background:var(--card);color:var(--ink);border:1px solid var(--line2)}
.btn{display:inline-flex;align-items:center;gap:6px;height:26px;padding:0 10px;border-radius:6px;background:var(--ctl);border:1px solid var(--line);font-size:12.5px;white-space:nowrap}
.btn.pri{background:var(--accent);border-color:var(--accent);color:#fff}
.btn.ghost{background:transparent;border-color:transparent;color:var(--ink2)}
.out{padding:6px}
.r{display:flex;align-items:center;gap:9px;padding:5px 8px;border-radius:6px;white-space:nowrap}
.r.sel{background:var(--wash);outline:1px solid rgba(45,129,255,.45)}
.ic{width:20px;text-align:center;font-size:15px}
.fn{font-family:"DejaVu Sans Mono",monospace;font-size:12.5px;min-width:130px}
.ti{color:var(--ink2)}
.badge{font-size:10.5px;padding:1px 6px;border-radius:9px;border:1px solid var(--line2);color:var(--ink2)}
.badge.res{color:#f5c76a;border-color:rgba(245,199,106,.4)}
.badge.src{color:var(--ink3)}
.badge.warn{color:var(--warn);border-color:rgba(224,162,58,.45)}
.badge.err{color:var(--danger);border-color:rgba(226,71,90,.45)}
.st{width:8px;height:8px;border-radius:50%;display:inline-block}
.foot{border-top:1px solid var(--line);padding:7px 12px;color:var(--ink3);font-size:12px;display:flex;gap:14px}
.ren{background:var(--card);border:1px solid var(--accent);border-radius:5px;padding:1px 6px;font-family:"DejaVu Sans Mono",monospace;font-size:12.5px;min-width:110px}
.menu{position:absolute;background:var(--head);border:1px solid var(--line2);border-radius:8px;padding:4px;box-shadow:0 10px 30px rgba(0,0,0,.4);font-size:12.5px;width:190px}
.menu div{padding:5px 10px;border-radius:5px;display:flex;justify-content:space-between}
.menu div.h{background:var(--wash)}
.menu hr{border:0;border-top:1px solid var(--line);margin:4px 0}
.kbd{font-size:10.5px;color:var(--ink3)}
.form{padding:12px 14px;display:grid;grid-template-columns:110px 1fr;gap:8px 12px;align-items:center}
.lab{color:var(--ink2);font-size:12px;text-align:right}
.inp{background:var(--card);border:1px solid var(--line2);border-radius:6px;padding:5px 8px;min-height:28px}
.q .k{color:var(--q-key)}.q .a{color:var(--q-act)}.q .p{color:var(--q-par)}.q .s{color:var(--q-sep)}.q .n{color:var(--q-ns)}.q .l{color:var(--q-link)}
.card{background:var(--card);border:1px solid var(--line2);border-radius:8px;padding:8px 10px;margin-top:4px}
.card .h{display:flex;gap:8px;align-items:center;font-weight:600}
.fld{display:grid;grid-template-columns:90px 1fr;gap:6px 10px;margin-top:8px;align-items:center;font-size:12.5px}
.sec{grid-column:1/3;color:var(--ink3);font-size:11px;letter-spacing:.06em;text-transform:uppercase;font-weight:600;margin-top:6px;border-top:1px solid var(--line);padding-top:8px}
.tree .r{padding-left:calc(8px + var(--d,0)*20px)}
.tw{width:12px;color:var(--ink3);font-size:10px}
.yaml{padding:10px 0;font-family:"DejaVu Sans Mono",monospace;font-size:12.5px;line-height:1.6}
.yaml .ln{display:flex;white-space:pre}.yaml .no{width:40px;text-align:right;padding-right:12px;color:var(--ink3)}
.yaml .k{color:var(--y-key)}.yaml .pu{color:var(--y-punct)}.yaml .c{color:var(--y-com)}.yaml .b{color:var(--y-bool)}.yaml .st2{color:var(--y-str)}
.yaml .errl{background:rgba(226,71,90,.12)}
.squig{text-decoration:underline wavy var(--danger)}
.panel{width:250px;flex:none}
.ptb{display:flex;align-items:center;gap:4px;padding:7px 8px;border-bottom:1px solid var(--line);background:var(--bg);white-space:nowrap}
.ib{width:24px;height:24px;border-radius:5px;display:inline-flex;align-items:center;justify-content:center;color:var(--ink2);font-size:13px}
.ib.on{background:var(--ctl2);color:var(--ink)}.ib.pri{background:var(--accent);color:#fff}
.pc{font-size:10.5px;color:var(--accent);font-variant-numeric:tabular-nums}
.tip{position:absolute;background:#2a2c33;border:1px solid var(--line2);border-radius:6px;padding:6px 9px;font-size:12px;max-width:260px;box-shadow:0 8px 24px rgba(0,0,0,.45);z-index:5}
.tip b{display:block}.tip span{color:var(--ink2)}
.note{color:var(--ink3);font-size:12px;margin-top:10px;max-width:1180px}
"""

def page(name, title, sub, body):
    with open(os.path.join(OUT, name + ".html"), "w") as f:
        f.write(f"<!doctype html><meta charset=utf-8><style>{CSS}</style><h1>{title}</h1><p class=sub>{sub}</p>{body}")

def q(text):
    """Very small query highlighter for the mockups."""
    out = []
    for seg in text.split("/"):
        if seg.startswith("-R") or seg == "-":
            out.append(f'<span class=s>{seg}</span>')
        elif "." in seg and "-" not in seg:
            out.append(f'<span class=k>{seg}</span>')
        elif seg.startswith("ns-"):
            out.append(f'<span class=n>{seg}</span>')
        else:
            parts = seg.split("-")
            out.append(f'<span class=a>{parts[0]}</span>' + "".join(f'<span class=s>-</span><span class=p>{p}</span>' for p in parts[1:]))
    return '<span class=q>' + '<span class=s>/</span>'.join(out) + '</span>'


def ptb(crumb, view="Outline", extra=""):
    o = "on" if view == "Outline" else ""
    d = "on" if view == "Dependencies" else ""
    return (f'<div class=ptb><span class=dim>📁</span><b>{crumb}</b>{extra}<span class=sp></span>'
            f'<span class="ib {o}" title="Outline">≡</span><span class="ib {d}" title="Dependencies">⑂</span>'
            f'<span class=ib title="YAML">{{ }}</span><span class=ib>⚙</span><span class="ib pri">＋</span></div>')

def row(icon, label, status="", sel=False, d=None, tw="", title=None, plain=False):
    dd = f' style="--d:{d}"' if d is not None else ""
    t = f'<span class=ti style="overflow:hidden;text-overflow:ellipsis">{title}</span>' if title else ""
    return (f'<div class="r{" sel" if sel else ""}"{dd}>' + (f'<span class=tw>{tw}</span>' if d is not None else "") +
            f'<span class=ic>{icon}</span><span class=fn style="min-width:0{';font-family:Inter,sans-serif;font-size:13px' if plain else ''}">{label}</span>{t}<span class=sp></span>{status}</div>')

def dot(c): return f'<span class=st style="background:{c}"></span>'
ok, none, err = "var(--ok)", "var(--ctl2)", "var(--danger)"
pct = lambda n: f'<span class=pc>{n}%</span>'
warn = '<span style="color:var(--warn);font-size:12px">⚠</span>'
res = '<span style="color:#f5c76a;font-size:12px">★</span>'

def recipe_window(w=600):
    return f"""
<div class=win style="width:{w}px">
 <div class=tb><span class=ic>🎨</span><b>Monthly chart</b><span class="dim mono">chart.png</span><span class=sp></span>
 <span class="seg"><span class=on>On demand</span><span>Apply immediately</span></span><span class="btn pri">✓</span><span class="btn ghost">↶</span><span class="btn ghost">✕</span></div>
 <div class=form>
  <span class=lab>Title</span><span class=inp>Monthly chart</span>
  <span class=lab>Description</span><span class="inp ink2">Bar chart of the monthly totals.</span>
  <span class=lab>Filename</span><span class="inp mono">chart.png <span class=dim>· rename rewrites references</span></span>
  <span class=lab>Query</span><span class="inp mono">{q("-R/./monthly.parquet/-/ns-pl/bar_chart-month-total/chart.png")}</span>
  <span></span><div>
   <div class=card><div class=h>📄 Resource <span class="dim mono" style="font-weight:400">./monthly.parquet</span></div></div>
   <div class=card><div class=h>📊 Bar chart <span class="dim mono" style="font-weight:400">pl/bar_chart</span></div>
    <div class=fld><span class=lab>x</span><span class="inp mono">month</span><span class=lab>y</span><span class="inp mono">total</span></div></div>
  </div>
  <span class=sec>Overrides</span>
  <span class=lab>arguments</span><span class="inp mono dim">＋ add argument override</span>
  <span class=sec>Options ▸</span>
  <span class=lab></span><span class="dim">cwd · volatile · expires · stored  (collapsed)</span>
 </div>
</div>"""

# 01 — side panel (label: name) + recipe window, tooltips
rows1 = "".join([
    row("🧮", "sales.csv", dot(ok)),
    row("🧮", "monthly.parquet", pct(42)),
    row("🎨", "chart.png", dot(ok), sel=True),
    row("📰", "report.html", warn + dot(none)),
    row("📄", "notes.txt", dot(err)),
])
rows1b = "".join([
    row("🧮", "Raw sales", dot(ok), plain=True),
    row("🧮", "Monthly totals", pct(42), plain=True),
    row("🎨", "Monthly chart", dot(ok), sel=True, plain=True),
    row("📰", "Sales report", warn + dot(none), plain=True),
    row("📄", "Notes", dot(err), plain=True),
])
rows1c = "".join([
    row("🧮", "sales.csv", dot(ok), title="Raw sales"),
    row("🧮", "monthly.parquet", pct(42), title="Monthly…"),
    row("🎨", "chart.png", dot(ok), sel=True, title="Monthly chart"),
    row("📰", "report.html", warn + dot(none), title="Sales report"),
    row("📄", "notes.txt", dot(err), title="Notes"),
])
tips = """<div class=tip style="left:70px;top:106px;width:200px"><b>Monthly totals</b><span>Sales summed per month, from sales.csv.</span></div>
<div class=tip style="left:236px;top:62px;width:190px"><b>Processing · 42%</b><span>Aggregating: 42 of 100 chunks</span></div>"""
page("01-recipes-list", "Recipes panel: a narrow navigator beside the recipe window",
     "Label setting: name (default), title or both. Status is a coloured circle, a percentage while progress is known, ⚠/✕ for findings; every one has a tooltip.",
     f"""<div class=row>
 <div class=col style="display:flex;flex-direction:column;gap:14px">
 <div class="win panel" style="position:relative;overflow:visible">{ptb("reports/")}<div class=out>{rows1}</div>{tips}
  <div class=foot><span>label: name</span></div></div>
 <div class="win panel">{ptb("reports/")}<div class=out>{rows1b}</div><div class=foot><span>label: title</span></div></div>
 <div class="win panel" style="width:300px">{ptb("reports/")}<div class=out>{rows1c}</div><div class=foot><span>label: both</span></div></div>
 </div><div style="margin-left:170px">{recipe_window()}</div></div>
 <p class=note>Tooltips: on the name — title and description; on the title — description; on the status — status name, or the progress message, or the finding / error. A click opens the recipe window; the status dot comes from the asset manager.</p>""")

# 02 — tree panel with context menu; dependencies panel
nodes = "".join([
    row("🫖", "body", dot(ok)),
    row("🫖", "tyre", dot(ok)),
    row("🫖", "wheel", pct(7), sel=True),
    row("🫖", "spare", dot(none)),
    row("🫖", "car", res + dot(none)),
    row("🎨", "preview", warn + dot(none)),
])
menu = """<div class=menu style="left:150px;top:118px;width:200px">
<div class=h>View<span class=kbd>dbl-click</span></div><div>Metadata</div><div>Edit<span class=kbd>click</span></div><div>Evaluate<span class=kbd>Mod+E</span></div><hr>
<div>Rename<span class=kbd>F2</span></div><div>Duplicate<span class=kbd>Mod+D</span></div><div>Add child…</div><div>Set as result</div><hr>
<div>Save as…</div><div>Export ▸</div><hr><div style="color:var(--danger)">Delete<span class=kbd>Del</span></div></div>
<div class=menu style="left:352px;top:398px;width:150px"><div>glTF binary (glb)</div><div>glTF (gltf)</div><div class=dim>no other formats</div></div>"""
deps = "".join([
    row("🫖", "car", res, d=0, tw="▾"),
    row("🫖", "body", "", d=1),
    row("🫖", "wheel", pct(7), d=1, tw="▾"),
    row("🫖", "tyre", "", d=2),
    row("🫖", "spare", "", d=1, tw="▾"),
    row("🫖", "tyre", '<span class="badge">×2</span>', d=2),
    row("🎨", "preview", "", d=0, tw="▸"),
])
page("02-recipe-tree", "Recipe tree panel: node names, right-click menu, dependencies view",
     "A folder with a <span class=mono>tree:</span> section — rows show node names (no extension), the icon carries the file type. Right click opens the menu.",
     f"""<div class=row style="gap:280px">
 <div class="win panel" style="position:relative;overflow:visible;min-height:520px">{ptb("car/", extra='<span class="badge">strict</span>')}<div class=out>{nodes}</div>{menu}</div>
 <div class="win panel tree" style="min-height:520px">{ptb("car/", "Dependencies", '<span class="badge">strict</span>')}<div class=out>{deps}</div>
  <div class=foot><span>top: nodes nothing depends on</span></div></div>
 </div>
 <p class=note>Menu items that do not apply are disabled with the reason as a tooltip (View before evaluation, Export when the type declares no other format). Export lists the formats of the value's type; Save as writes the value to a file on this computer, not to the store.</p>""")

# 03 — YAML window beside the panel
lines = [
    ('<span class=k>tree</span><span class=pu>:</span>', ""),
    ('  <span class=k>result</span><span class=pu>:</span> <span class=st2>car.glb</span>', ""),
    ('  <span class=k>dependencies</span><span class=pu>:</span> <span class=b>strict</span>', ""),
    ('<span class=k>recipes</span><span class=pu>:</span>', ""),
    ('  <span class=pu>-</span> <span class=k>query</span><span class=pu>:</span> ' + q("body.glb"), ""),
    ('    <span class=k>title</span><span class=pu>:</span> <span class=st2>Body</span>', ""),
    ('  <span class=pu>-</span> <span class=k>query</span><span class=pu>:</span> ' + q("tyre.glb"), ""),
    ('    <span class=k>title</span><span class=pu>:</span> <span class=st2>Tyre</span>', ""),
    ('  <span class=pu>-</span> <span class=k>query</span><span class=pu>:</span> <span class=pu>"</span>' + q("-R/./tyre.glb/-/ns-mesh/rim-17/wheel.glb") + '<span class=pu>"</span>', ""),
    ('    <span class=k>title</span><span class=pu>:</span> <span class=st2>Wheel</span>', ""),
    ('  <span class=pu>-</span> <span class=k>query</span><span class=pu>:</span> <span class=pu>"</span>' + q("ns-mesh/assemble-body-wheel/car.glb") + '<span class=pu>"</span>', ""),
    ('    <span class=k>title</span> <span class=squig>Car</span>', "errl"),
]
yaml = "".join(f'<div class="ln {c}"><span class=no>{i+1}</span><span>{t}</span></div>' for i, (t, c) in enumerate(lines))
page("03-yaml-view", "YAML window: the whole file, highlighted",
     "Opened from the panel's { } button as its own window. Queries inside <span class=mono>query:</span> use the query highlighter; typed text applies only when it parses as a recipe list.",
     f"""<div class=row>
 <div class="win panel">{ptb("car/", extra='<span class="badge">strict</span>')}<div class=out>{nodes}</div></div>
 <div class=win style="width:760px"><div class=tb><span class=dim>{{ }}</span><b>recipes.yaml</b><span class="dim mono">scenes/car/</span><span class=sp></span><span class="btn pri">✓</span><span class="btn ghost">↶</span><span class="btn ghost">✕</span></div>
  <div class=yaml>{yaml}</div>
  <div class=foot style="color:var(--danger)"><span>✕ line 12: expected ':' after a mapping key — nothing applied</span><span class=sp></span><span class=dim>Enter or ✓ applies · Esc reverts</span></div>
 </div></div>""")

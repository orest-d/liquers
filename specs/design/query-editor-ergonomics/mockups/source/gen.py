"""Generates the GUI editor mockups as HTML pages (rendered to PNG by shot.js)."""
import os, re, html

HERE = os.path.dirname(os.path.abspath(__file__))
ICONS = os.path.join(HERE, "..", "icons", "ph", "package", "assets", "regular")
OUT = os.path.join(HERE, "html")
os.makedirs(OUT, exist_ok=True)


def ic(name, size=16, cls=""):
    svg = open(os.path.join(ICONS, f"{name}.svg")).read()
    svg = re.sub(r'<svg ', f'<svg class="ic {cls}" width="{size}" height="{size}" ', svg, count=1)
    return svg


CSS = r"""
:root{
 --bg:#121316; --panel:#16171b; --card:#1b1c21; --head:#202127; --ctl:#26272d; --ctl2:#2e3037;
 --ink:#f2f1ee; --ink2:#a9abb3; --ink3:#6c6f79; --line:rgba(255,255,255,.07); --line2:rgba(255,255,255,.12);
 --accent:#2d81ff; --wash:rgba(45,129,255,.14); --danger:#e2475a; --ok:#3fb27f; --warn:#e0a23a;
 --ns-pl:#2fb3a3; --ns-img:#9b6cff; --ns-res:#4c9dff; --ns-core:#8a8f9c;
 --q-key:#8ec5ff; --q-act:#f2f1ee; --q-par:#f0b37e; --q-sep:#5d616b; --q-ns:#c49bff; --q-link:#7fd6a1;
 --shadow:0 1px 0 rgba(255,255,255,.03) inset, 0 6px 18px rgba(0,0,0,.28);
}
.light{
 --bg:#f4f4f2; --panel:#ecebe8; --card:#ffffff; --head:#fafaf8; --ctl:#f0efec; --ctl2:#e6e5e1;
 --ink:#1d1e22; --ink2:#55585f; --ink3:#8b8e96; --line:rgba(0,0,0,.08); --line2:rgba(0,0,0,.14);
 --accent:#1f6fe5; --wash:rgba(31,111,229,.10);
 --q-key:#1f63c6; --q-act:#1d1e22; --q-par:#b4570f; --q-sep:#a3a6ad; --q-ns:#7b45d6; --q-link:#1d8a4f;
 --shadow:0 1px 2px rgba(0,0,0,.05), 0 4px 14px rgba(0,0,0,.05);
}
*{box-sizing:border-box}
html,body{margin:0;background:var(--bg);color:var(--ink);font:13px/1.45 Inter, system-ui, sans-serif}
body{padding:28px}
.mono{font-family:"DejaVu Sans Mono", ui-monospace, monospace;font-size:12.5px}
.ic{vertical-align:-3px;fill:currentColor;flex:none}
h1{font:600 20px/1.2 Inter;margin:0 0 4px;letter-spacing:-.01em}
.sub{color:var(--ink2);margin:0 0 20px;font-size:13px}
.cap{color:var(--ink3);font-size:11px;letter-spacing:.06em;text-transform:uppercase;font-weight:600;margin:0 0 8px}
.row{display:flex;gap:24px;align-items:flex-start}
.col{display:flex;flex-direction:column;gap:10px}

/* window */
.win{background:var(--panel);border:1px solid var(--line2);border-radius:10px;overflow:hidden;box-shadow:0 20px 50px rgba(0,0,0,.35)}
.light .win{box-shadow:0 12px 40px rgba(0,0,0,.10)}
.titlebar{white-space:nowrap;display:flex;align-items:center;gap:10px;padding:9px 12px;border-bottom:1px solid var(--line);background:var(--bg)}
.dots{display:flex;gap:6px}.dots i{width:10px;height:10px;border-radius:50%;background:var(--ctl2);display:block}
.crumb{color:var(--ink2);font-size:12.5px;display:flex;align-items:center;gap:6px}
.crumb b{color:var(--ink);font-weight:600}
.sp{flex:1}
.seg{display:inline-flex;background:var(--ctl);border:1px solid var(--line);border-radius:7px;padding:2px}
.seg span{padding:3px 10px;border-radius:5px;color:var(--ink2);font-size:12px;font-weight:500;display:flex;gap:5px;align-items:center}
.seg span.on{background:var(--card);color:var(--ink);box-shadow:0 1px 2px rgba(0,0,0,.25);border:1px solid var(--line2)}
.kbd{font:500 10.5px/1 Inter;color:var(--ink3);border:1px solid var(--line2);border-bottom-width:2px;border-radius:4px;padding:2px 5px;background:var(--ctl)}
.btn{display:inline-flex;align-items:center;gap:6px;height:28px;padding:0 10px;border-radius:6px;background:var(--ctl);border:1px solid var(--line);color:var(--ink);font-size:12.5px;font-weight:500;white-space:nowrap}
.btn.ghost{background:transparent;border-color:transparent;color:var(--ink2)}
.btn.pri{background:var(--accent);border-color:var(--accent);color:#fff}
.btn.sm{height:24px;padding:0 8px;font-size:12px}
.btn.icon{width:28px;padding:0;justify-content:center}
.btn.icon.sm{width:24px}
.btn.danger{color:var(--danger)}
.btn.dis{opacity:.4}
.toggle{display:inline-flex;align-items:center;gap:7px;color:var(--ink2);font-size:12.5px}
.toggle i{width:26px;height:15px;border-radius:8px;background:var(--ctl2);position:relative;display:block}
.toggle i:after{content:"";position:absolute;left:2px;top:2px;width:11px;height:11px;border-radius:50%;background:var(--ink3)}
.toggle.on i{background:var(--accent)}.toggle.on i:after{left:13px;background:#fff}
.vsep{width:1px;height:18px;background:var(--line2)}

/* query bar */
.qbar{margin:12px 14px 4px;display:flex;align-items:center;gap:8px;background:var(--bg);border:1px solid var(--line2);border-radius:8px;padding:8px 10px}
.qbar.focus{border-color:var(--accent);box-shadow:0 0 0 3px var(--wash)}
.q{white-space:pre;flex:1;overflow:hidden}
.q .h{color:var(--q-sep)}.q .k{color:var(--q-key)}.q .s{color:var(--q-sep)}.q .n{color:var(--q-ns)}
.q .a{color:var(--q-act);font-weight:700}.q .p{color:var(--q-par)}.q .l{color:var(--q-link)}
.q .hl{background:var(--wash);outline:1px solid rgba(45,129,255,.5);border-radius:3px}
.q .dim{opacity:.45}
.caret{display:inline-block;width:1.5px;height:15px;background:var(--accent);vertical-align:-3px;margin:0 -0.75px;box-shadow:0 0 0 .5px var(--accent)}
.qstatus{margin:0 16px 8px;color:var(--ink3);font-size:11.5px;display:flex;gap:14px;align-items:center}
.qstatus .okc{color:var(--ok);display:flex;gap:4px;align-items:center}

/* pipeline breadcrumb (simple mode) */
.steps{display:flex;align-items:center;gap:4px;margin:6px 14px 12px;flex-wrap:wrap}
.chip{white-space:nowrap;display:inline-flex;align-items:center;gap:5px;height:24px;padding:0 9px;border-radius:12px;background:var(--ctl);border:1px solid var(--line);color:var(--ink2);font-size:12px}
.chip.mono{font-size:11.5px}
.chip.cur{background:var(--wash);border-color:rgba(45,129,255,.45);color:var(--ink)}
.chip.on{background:var(--accent);border-color:var(--accent);color:#fff}
.chip.add{border-style:dashed;background:transparent}
.arrow{color:var(--ink3)}

/* cards */
.stack{padding:4px 14px 16px;display:flex;flex-direction:column;gap:8px}
.card{background:var(--card);border:1px solid var(--line2);border-radius:9px;box-shadow:var(--shadow);overflow:hidden}
.card.focus{border-color:rgba(45,129,255,.7);box-shadow:0 0 0 3px var(--wash), var(--shadow)}
.card.err{border-color:rgba(226,71,90,.55)}
.chead{display:flex;align-items:center;gap:9px;height:40px;padding:0 10px 0 8px;background:var(--head)}
.card.open .chead{border-bottom:1px solid var(--line)}
.chev{color:var(--ink3)}
.tile{width:24px;height:24px;border-radius:6px;display:flex;align-items:center;justify-content:center;color:#fff;flex:none}
.tile.mono{font:700 12px/1 Inter}
.cname{font-weight:600;font-size:13.5px}
.clabel{color:var(--ink3);font-size:12.5px}
.ns{font:600 10.5px/1 Inter;color:var(--ink2);border:1px solid var(--line2);border-radius:4px;padding:2px 5px;letter-spacing:.02em}
.idx{color:var(--ink3);font-size:11.5px;font-variant-numeric:tabular-nums}
.dot{width:7px;height:7px;border-radius:50%;background:var(--warn)}
.summary{color:var(--ink3);font-size:12px}
.ctool{display:flex;align-items:center;gap:4px;padding:6px 8px;border-bottom:1px solid var(--line);background:var(--card)}
.grid{display:grid;grid-template-columns:132px 1fr 56px;column-gap:14px;row-gap:9px;padding:12px 14px 14px;align-items:center}
.lab{color:var(--ink2);font-size:12.5px;text-align:right;white-space:nowrap}
.lab .req{color:var(--danger)}
.fld{display:flex;align-items:center;gap:6px;min-width:0}
.inp{height:28px;flex:1;min-width:0;display:flex;align-items:center;gap:6px;background:var(--bg);border:1px solid var(--line2);border-radius:6px;padding:0 9px;color:var(--ink)}
.light .inp{background:#fff}
.inp.ph{color:var(--ink3)}
.inp.focus{border-color:var(--accent);box-shadow:0 0 0 3px var(--wash)}
.inp.w{flex:none;width:110px}
.sel{justify-content:space-between}
.hint{grid-column:2/3;color:var(--ink3);font-size:11.5px;margin-top:-5px}
.fa{display:flex;gap:2px;justify-content:flex-end;color:var(--ink3)}
.fa .b{width:24px;height:24px;display:flex;align-items:center;justify-content:center;border-radius:5px}
.fa .b.on{color:var(--accent);background:var(--wash)}
.fa.faint{opacity:.35}
.check{width:16px;height:16px;border-radius:4px;border:1px solid var(--line2);background:var(--bg);display:inline-flex;align-items:center;justify-content:center}
.check.on{background:var(--accent);border-color:var(--accent);color:#fff}
.seg2{display:inline-flex;border:1px solid var(--line2);border-radius:6px;overflow:hidden}
.seg2 span{padding:4px 10px;font-size:12px;color:var(--ink2);border-right:1px solid var(--line2)}
.seg2 span:last-child{border-right:0}.seg2 span.on{background:var(--wash);color:var(--ink)}
.addrow{display:flex;gap:8px;align-items:center;padding:2px 2px}
.next{color:var(--ink3);font-size:12px;display:flex;align-items:center;gap:6px}

/* advanced */
.adv{display:grid;grid-template-columns:236px 1fr;min-height:0}
.outline{border-right:1px solid var(--line);padding:12px 8px;background:var(--bg)}
.tr{display:flex;align-items:center;gap:6px;height:26px;padding:0 6px;border-radius:5px;color:var(--ink2);font-size:12.5px;white-space:nowrap}
.tr.cur{background:var(--wash);color:var(--ink)}
.tr .g{color:var(--ink3)}
.tr .clabel{margin-left:5px}
.group{margin:2px 0 6px}
.glabel{display:flex;align-items:center;gap:8px;color:var(--ink3);font:600 10.5px/1 Inter;letter-spacing:.07em;text-transform:uppercase;margin:6px 2px 8px}
.glabel:after{content:"";flex:1;height:1px;background:var(--line)}
.treeline{border-left:1px dashed var(--line2);margin-left:22px;padding-left:12px;display:flex;flex-direction:column;gap:8px}
.sub-card{margin-top:2px}

/* annotation */
.note{position:absolute;display:flex;gap:8px;align-items:flex-start;max-width:270px;font-size:12px;color:var(--ink2)}
.badge{flex:none;width:20px;height:20px;border-radius:50%;background:var(--accent);color:#fff;font:700 11px/20px Inter;text-align:center}
.badge.r{background:var(--danger)}
.rel{position:relative}
.pin{position:absolute;width:20px;height:20px;border-radius:50%;background:var(--accent);color:#fff;font:700 11px/20px Inter;text-align:center;box-shadow:0 0 0 3px var(--bg)}
.pin.r{background:var(--danger)}
.legend{display:flex;flex-direction:column;gap:10px;width:300px}
.legend .it{display:flex;gap:10px;font-size:12.5px;color:var(--ink2)}
.legend .it b{color:var(--ink);font-weight:600}
.panel{background:var(--panel);border:1px solid var(--line2);border-radius:10px;padding:16px}
.tag{display:inline-block;font:600 10.5px/1 Inter;padding:4px 7px;border-radius:4px;background:var(--wash);color:var(--accent);letter-spacing:.04em;text-transform:uppercase}
.tag.g{background:rgba(63,178,127,.15);color:var(--ok)}
.tag.gray{background:var(--ctl);color:var(--ink3)}
.pop{position:absolute;background:var(--card);border:1px solid var(--line2);border-radius:8px;box-shadow:0 12px 30px rgba(0,0,0,.45);padding:4px;min-width:220px;z-index:5}
.mi{display:flex;align-items:center;gap:8px;padding:6px 9px;border-radius:5px;font-size:12.5px}
.mi.hov{background:var(--wash)}
.mi .d{color:var(--ink3);font-size:11.5px;margin-left:auto}
.mi .kbd{margin-left:auto}
.msep{height:1px;background:var(--line);margin:4px 2px}
"""

NS_COLOR = {"pl": "var(--ns-pl)", "img": "var(--ns-img)", "res": "var(--ns-res)", "": "var(--ns-core)"}


def page(name, body, width=1280, light=False):
    doc = f"""<!doctype html><html class="{'light' if light else ''}"><head><meta charset="utf-8"><title>{name}</title>
<style>{CSS}</style></head><body class="{'light' if light else ''}" style="width:{width}px">{body}</body></html>"""
    open(os.path.join(OUT, f"{name}.html"), "w").write(doc)


# ---------------------------------------------------------------- query text
def qspan(parts):
    """parts: list of (cls, text) -> highlighted query html."""
    return "".join(f'<span class="{c}">{html.escape(t)}</span>' if c else html.escape(t) for c, t in parts)


Q_PARTS = [("h", "-R/"), ("k", "data/sales.csv"), ("s", "/-/"), ("n", "ns-pl"), ("s", "/"),
           ("a", "gt"), ("s", "-"), ("p", "amount"), ("s", "-"), ("p", "1000"), ("s", "/"),
           ("a", "sort"), ("s", "-"), ("p", "amount"), ("s", "-"), ("p", "f"), ("s", "/"),
           ("a", "head"), ("s", "-"), ("p", "20")]


def qbar(parts=Q_PARTS, focus=False, extra=""):
    return f'''<div class="qbar {'focus' if focus else ''}">{ic("magnifying-glass",15,"").replace('class="ic ','class="ic ')}
<div class="q mono">{qspan(parts) if isinstance(parts,list) else parts}</div>{extra}
<span class="btn ghost sm icon" title="copy">{ic("copy",15)}</span></div>'''


def titlebar(mode="simple", pending=True, light=False):
    s_on, a_on = ("on", "") if mode == "simple" else ("", "on")
    return f'''<div class="titlebar"><div class="dots"><i></i><i></i><i></i></div>
<div class="crumb">{ic("file-text",15)} recipes.yaml <span class="arrow">›</span> <b>top_sales</b></div>
<div class="sp"></div>
<div class="seg"><span class="{s_on}">{ic("square",13)} Simple</span><span class="{a_on}">{ic("tree-structure",13)} Advanced</span></div>
<span class="kbd">Ctrl B</span>
<div class="vsep"></div>
<span class="toggle"><i></i>Update immediately</span>
<span class="btn ghost sm">{ic("arrow-counter-clockwise",14)} Revert</span>
<span class="btn pri sm">{ic("check",14)} Update {'<span style="opacity:.75;font-weight:400">• 2 edits</span>' if pending else ''}</span>
</div>'''


# ---------------------------------------------------------------- cards
def tile(ns, icon=None, letter=None, size=24):
    col = NS_COLOR.get(ns, "var(--ns-core)")
    inner = ic(icon, 15) if icon else letter
    cls = "tile" if icon else "tile mono"
    return f'<div class="{cls}" style="background:{col};width:{size}px;height:{size}px">{inner}</div>'


HV = {"v": "c"}


def chead(name, label, ns, icon=None, letter=None, open_=True, idx=None, modified=False, summary=None, err=False, realm=None, hv=None):
    hv = hv or HV["v"]
    chev = ic("caret-down" if open_ else "caret-right", 14, "chev")
    right = ""
    if modified:
        right += '<span class="dot" title="modified"></span>'
    if err:
        right += f'<span style="color:var(--danger)">{ic("warning-circle",16)}</span>'
    if idx is not None:
        right += f'<span class="idx">#{idx}</span>'
    summ = f'<span class="summary mono">{html.escape(summary)}</span>' if summary else ""
    ident = f"{ns}/{name}" if ns else name
    rchip = f'<span class="ns" style="color:var(--accent);border-color:rgba(45,129,255,.4)">{realm}</span>' if realm else ""
    more = f'<span class="btn ghost sm icon">{ic("dots-three",16)}</span>'
    if hv == "a":
        mid = f'<span class="cname mono">{html.escape(name)}</span><span class="clabel">{html.escape(label)}</span>{summ}<div class="sp"></div>{right}' + (f'<span class="ns">{ns}</span>' if ns else "")
        return f'<div class="chead"><span class="chev">{chev}</span>{tile(ns, icon, letter)}{mid}{more}</div>'
    if hv == "b":
        return f'<div class="chead"><span class="chev">{chev}</span>{tile(ns, icon, letter)}<span class="cname">{html.escape(label)}</span>{summ}<div class="sp"></div>{right}{more}</div>'
    if open_:
        return f'''<div class="chead" style="height:52px"><span class="chev">{chev}</span>{tile(ns, icon, letter, size=30)}
<div style="display:flex;flex-direction:column;gap:2px;min-width:0"><div style="display:flex;gap:6px;align-items:center">{rchip}<span class="mono" style="font-size:11.5px;color:var(--ink3)">{html.escape(ident)}</span></div>
<span class="cname" style="font-size:14px">{html.escape(label)}</span></div>{summ}<div class="sp"></div>{right}{more}</div>'''
    return f'''<div class="chead"><span class="chev">{chev}</span>{tile(ns, icon, letter)}<span class="cname">{html.escape(label)}</span>
{rchip}<span class="mono" style="font-size:11.5px;color:var(--ink3)">{html.escape(ident)}</span>{summ}<div class="sp"></div>{right}{more}</div>'''


def toolbar(presets="hybrid", nexts=True, simple=False):
    if presets == "hybrid":
        pre = (f'<span class="clabel" style="margin:0 2px 0 6px">Presets</span>'
               '<span class="chip">Top 10</span><span class="chip on">Top 20</span><span class="chip">Top 100</span>'
               f'<span class="btn ghost sm" style="background:var(--ctl)">+3 {ic("caret-down",12)}</span>')
    elif presets == "menu":
        pre = f'<span class="btn sm">{ic("lightning",14)} Presets {ic("caret-down",12)}</span>'
    else:
        pre = ""
    nx = f'<span class="btn ghost sm">{ic("arrow-right",14)} Next {ic("caret-down",12)}</span>' if nexts else ""
    move = "" if simple else (f'<span class="btn ghost sm icon">{ic("arrow-up",15)}</span>'
                              f'<span class="btn ghost sm icon">{ic("arrow-down",15)}</span>')
    return f'''<div class="ctool"><span class="btn ghost sm">{ic("arrow-counter-clockwise",14)} Reset</span>
<div class="vsep"></div>{pre}<div class="vsep"></div>{nx}<div class="sp"></div>{move}
<span class="btn ghost sm icon danger">{ic("trash",15)}</span></div>'''


def field(label, editor, actions="std", hint=None, req=False):
    if actions == "std":
        fa = f'<div class="fa faint"><span class="b">{ic("link",14)}</span><span class="b">{ic("arrow-counter-clockwise",14)}</span></div>'
    elif actions == "hover":
        fa = f'<div class="fa"><span class="b">{ic("link",14)}</span><span class="b">{ic("arrow-counter-clockwise",14)}</span></div>'
    elif actions == "link":
        fa = f'<div class="fa"><span class="b on">{ic("link",14)}</span><span class="b">{ic("arrow-counter-clockwise",14)}</span></div>'
    else:
        fa = '<div class="fa"></div>'
    r = '<span class="req"> *</span>' if req else ""
    h = f'<div></div><div class="hint">{hint}</div><div></div>' if hint else ""
    return f'<div class="lab">{label}{r}</div><div class="fld">{editor}</div>{fa}{h}'


def inp(text, ph=False, focus=False, w=False, mono=True, icon=None, sel=False):
    cls = "inp" + (" ph" if ph else "") + (" focus" if focus else "") + (" w" if w else "") + (" sel" if sel else "")
    t = f'<span class="{"mono" if mono else ""}">{html.escape(text)}</span>'
    if focus:
        t += '<span class="caret" style="margin-left:-4px"></span>'
    after = ic("caret-up-down", 13) if sel else ""
    pre = ic(icon, 14) if icon else ""
    return f'<span class="{cls}">{pre}{t}{after}</span>'


def check(on, label=""):
    return f'<span class="check {"on" if on else ""}">{ic("check",11) if on else ""}</span><span style="color:var(--ink2)">{label}</span>'


def card(head, body="", cls="open"):
    return f'<div class="card {cls}">{head}{body}</div>'


def grid(*fields):
    return f'<div class="grid">{"".join(fields)}</div>'


HEAD_FIELDS = grid(
    field("Number of rows", inp("20", w=True) + '<span class="clabel">default 5</span>', hint="Return the first N rows"),
)
SORT_FIELDS = grid(
    field("Column", inp("amount", sel=True), req=True),
    field("Ascending", check(False)),
    field("Nulls last", check(False)),
)
GT_FIELDS = grid(
    field("Column", inp("amount", sel=True), req=True),
    field("Value", inp("1000"), req=True, hint="Rows where column &gt; value"),
)


# ================================================================ 01 current state
def current():
    E = """<style>
.eg{background:#1b1b1b;color:#8c8c8c;font:13.5px/1.35 "DejaVu Sans",sans-serif;padding:8px 10px;width:660px;border:1px solid #3c3c3c;border-radius:6px}
.eg .r{display:flex;align-items:center;gap:6px;margin:4px 0;flex-wrap:wrap}
.eg .b{background:#3c3c3c;color:#d2d2d2;padding:1px 6px;border-radius:2px}
.eg .b.d{color:#666}
.eg .cb{width:14px;height:14px;background:#3c3c3c;display:inline-block;border-radius:2px}
.eg .te{background:#0a0a0a;border:1px solid #3c3c3c;border-radius:2px;padding:2px 4px;color:#d2d2d2}
.eg .hd{color:#dcdcdc;font-weight:bold;margin:6px 0 2px}
.eg .gr{display:grid;grid-template-columns:auto auto auto;gap:3px 8px;margin-left:18px;align-items:center;justify-content:start}
.eg .sb{background:#3c3c3c;color:#d2d2d2;padding:0 4px;border-radius:2px;font-size:12px;margin-right:3px}
.eg .it{font-style:italic}
</style>"""
    q = qspan(Q_PARTS)
    def act(name, rows):
        g = "".join(f'<span>{l}</span><span>{e}</span><span><span class="sb">link</span><span class="sb">↺</span></span>' for l, e in rows)
        return f'<div class="hd">⏷ {name}</div><div class="gr">{g}</div><div class="r"><span class="b">⋯</span></div>'
    box = f"""{E}<div class="eg">
<div class="r"><span class="cb"></span> Update immediately <span class="b d">Update</span><span class="b">Revert</span><span class="it">pending</span></div>
<div class="r"><div class="te mono q" style="width:100%">{q}</div></div>
<div class="r">resource <span class="b">-R ⏷</span><span class="te" style="width:200px">data/sales.csv</span><span class="b">📁</span></div>
<div class="r">ns <span class="mono" style="color:#d2d2d2">pl</span><span class="sb">✕</span><span class="b">+</span></div>
{act("gt", [("column", '<span class="te" style="width:150px">amount</span>'), ("value", '<span class="te" style="width:150px">1000</span>')])}
{act("sort", [("column", '<span class="te" style="width:150px">amount</span>'), ("ascending", '<span class="cb"></span>'), ("nulls last", '<span class="cb"></span>')])}
{act("head", [("n", '<span class="te" style="width:90px;color:#666">5</span>')])}
<div class="r"><span class="b">+ action</span><span class="b">+ directive ⏷</span></div>
<div class="r">filename <span class="te" style="width:150px">&nbsp;</span><span class="b">Default</span></div>
</div>"""
    pins = [(12, 2, "r"), (52, 2, "r"), (150, 2, "r"), (178, 210, "r"), (240, 100, "r"), (300, 2, "r")]
    issues = [
        ("No frames", "Actions, resource and <span class='mono'>ns</span> rows share one flat column; nothing shows where an action begins or ends."),
        ("Detached action menu", "The <span class='mono'>⋯</span> menu (and <i>presets</i>, <i>next</i>) sits <i>under</i> the fields, so it reads as belonging to the next action."),
        ("Everything, always", "Every segment of the query is editable at once. The common case, tuning the last action, gets no focus."),
        ("Cramped three-column grid", "Raw argument names as labels, no alignment, widths set by each widget; defaults show only as faint placeholders."),
        ("Repeated per-field buttons", "<span class='mono'>link</span> and <span class='mono'>↺</span> repeat on every row at full strength: noise that hides the values."),
        ("No modes, icons or shortcuts", "Commit mode and editing share one toolbar; no command icons; keyboard use is egui's default Tab order only."),
    ]
    legend = "".join(f'<div class="it"><span class="badge r">{i+1}</span><div><b>{t}</b><br>{d}</div></div>' for i, (t, d) in enumerate(issues))
    body = f"""<h1>Current state (main, after PR #275)</h1>
<p class="sub">Reconstructed from <span class="mono">liquers-lib/src/egui/editor.rs</span> with egui's default dark visuals; the theme layer does not exist yet. Query: <span class="mono">-R/data/sales.csv/-/ns-pl/gt-amount-1000/sort-amount-f/head-20</span></p>
<div class="row"><div class="rel">{box}
<span class="pin r" style="left:-10px;top:205px">1</span>
<span class="pin r" style="left:62px;top:182px">2</span>
<span class="pin r" style="left:-10px;top:66px">3</span>
<span class="pin r" style="left:-10px;top:140px">4</span>
<span class="pin r" style="left:240px;top:140px">5</span>
<span class="pin r" style="left:430px;top:6px">6</span>
</div><div class="legend" style="width:420px">{legend}</div></div>"""
    page("01-current-state", body, width=1180)


# ================================================================ 02 simple
def simple_window(light=False):
    steps = (f'<span class="chip">{ic("file-csv",13)} sales.csv</span><span class="arrow">›</span>'
             f'<span class="chip">{ic("funnel",13)} Greater than</span><span class="arrow">›</span><span class="chip">{ic("sort-ascending",13)} Sort by column</span><span class="arrow">›</span>'
             f'<span class="chip cur">{ic("list-numbers",13)} Get first rows</span>'
             f'<span class="sp"></span><span class="clabel" style="font-size:11.5px">Earlier steps are edited in Advanced</span>')
    head = card(chead("head", "Get first rows", "pl", icon="list-numbers", open_=True, modified=True)
                + toolbar("hybrid", True, simple=True)
                + grid(field("Number of rows", inp("20", w=True, focus=True) + '<span class="clabel">default 5</span>', actions="hover", hint="Return first N rows")),
                cls="open focus")
    cont = (f'<div class="addrow"><span class="next">{ic("arrow-bend-down-right",14)} Continue with</span>'
            f'<span class="chip">{ic("export",13)} to_csv</span><span class="chip">{ic("chart-bar",13)} describe</span><span class="chip">{ic("hash",13)} nrows</span>'
            f'<span class="chip add">{ic("plus",13)} Add action…</span><span class="kbd">Ctrl K</span></div>')
    foot = (f'<div style="display:flex;align-items:center;gap:10px;padding:10px 14px;border-top:1px solid var(--line);background:var(--bg)">'
            f'<span class="clabel">Filename</span><span class="inp" style="flex:none;width:170px"><span class="mono">top_sales.csv</span></span><span class="btn ghost sm">Default</span>'
            f'<span class="sp"></span><span class="clabel" style="font-size:11.5px">{ic("keyboard",14)} Tab fields · PgUp/PgDn actions · Ctrl J jump</span></div>')
    status = f'<div class="qstatus"><span class="okc">{ic("check-circle",13)} valid</span><span>3 actions</span><span>result: DataFrame</span></div>'
    return f"""<div class="win" style="width:900px">{titlebar("simple")}{qbar(Q_PARTS + [("", "")], extra='')}{status}
<div class="steps">{steps}</div><div class="stack">{head}{cont}</div>{foot}</div>"""


def simple():
    items = [
        ("Mode switch", "Simple shows one action, Advanced the whole query as a tree. <span class='kbd'>Ctrl B</span> toggles."),
        ("Query text stays", "Always editable; the source of truth both modes share."),
        ("Steps, not a tree", "Earlier steps are read-only chips; clicking one opens it in Advanced."),
        ("One card", "The last action, expanded, with its toolbar and a two-column form."),
        ("Continue with", "The command's <i>next</i> presets as chips, then the command picker."),
    ]
    legend = "".join(f'<div class="it"><span class="badge">{i+1}</span><div><b>{t}</b><br>{d}</div></div>' for i, (t, d) in enumerate(items))
    body = f"""<h1>Reference design: Simple mode</h1>
<p class="sub">Edit the last action only. No tree elements. The common case of tuning a query you just typed.</p>
<div class="row"><div class="rel">{simple_window()}
<span class="pin" style="left:400px;top:-8px">1</span><span class="pin" style="left:-10px;top:52px">2</span>
<span class="pin" style="left:-10px;top:120px">3</span><span class="pin" style="left:-10px;top:168px">4</span>
<span class="pin" style="left:-10px;top:330px">5</span></div>
<div class="legend">{legend}</div></div>"""
    page("02-simple-mode", body, width=1260)
    body_l = f"""<h1>Reference design: light theme</h1>
<p class="sub">The same window with the light token set. Both themes come from one token struct (survey §4), also emitted as CSS for the browser renderer.</p>
{simple_window(light=True)}"""
    page("07-light-theme", body_l, width=960, light=True)


# ================================================================ 03 advanced
def advanced():
    q = [("h", "-R/"), ("k", "data/sales.csv"), ("s", "/-/"), ("n", "ns-pl"), ("s", "/"),
         ("a", "gt"), ("s", "-"), ("p", "amount"), ("s", "-"), ("l", "~X~-R/config/limit.txt~E"), ("s", "/"),
         ("a", "sort"), ("s", "-"), ("p", "amount"), ("s", "-"), ("p", "f"), ("s", "/"), ("a", "head"), ("s", "-"), ("p", "20")]
    def tr(depth, icon, text, sel=False, caret=None, mono=True, extra=""):
        c = ic("caret-down" if caret == "d" else "caret-right", 11) if caret else '<span style="width:11px;display:inline-block"></span>'
        return (f'<div class="tr {"cur" if sel else ""}" style="padding-left:{6+depth*14}px"><span class="g">{c}</span>'
                f'{icon}<span class="{"mono" if mono else ""}" style="font-size:12px">{text}</span>{extra}</div>')
    t = lambda ns, i: tile(ns, i, size=18).replace('width:18px', 'width:18px').replace("<svg class=\"ic \" width=\"15\" height=\"15\"", "<svg class=\"ic \" width=\"12\" height=\"12\"")
    outline = (f'<div class="cap" style="margin-left:6px">Outline</div>'
               + tr(0, "", "Key part", caret="d", mono=False)
               + tr(1, t("res", "file-csv"), "Resource <span class=\"clabel mono\">sales.csv</span>", mono=False)
               + tr(0, "", "Query part", caret="d", mono=False)
               + tr(1, t("", "brackets-curly"), "Namespaces <span class=\"clabel mono\">pl</span>", mono=False)
               + tr(1, t("pl", "funnel"), "Greater than <span class=\"clabel mono\">gt</span>", caret="d", sel=True, mono=False)
               + tr(2, f'<span style="color:var(--q-link)">{ic("link",13)}</span>', "Value <span class=\"clabel\">link</span>", mono=False)
               + tr(3, t("res", "file-text"), "Resource <span class=\"clabel mono\">limit.txt</span>", mono=False)
               + tr(1, t("pl", "sort-ascending"), "Sort by column", mono=False)
               + tr(1, t("pl", "list-numbers"), "Get first rows", mono=False)
               + f'<div class="tr" style="margin-top:6px;color:var(--ink3)">{ic("plus",13)}<span style="font-size:12px">Add action</span><span class="sp"></span><span class="kbd">Ctrl K</span></div>')
    res = card(chead("-R", "Resource", "", icon="file-csv", open_=True, idx=0).replace("var(--ns-core)", "var(--ns-res)")
               + grid(field("Header", inp("-R  asset", sel=True, mono=True).replace("inp ", "inp w ").replace('class="inp w sel"', 'class="inp sel" style="flex:none;width:150px"'), actions="none"),
                      field("Key", inp("data/sales.csv") + f'<span class="btn sm icon">{ic("folder-open",15)}</span>', actions="none")))
    ns = card(chead("ns", "Namespaces", "", icon="brackets-curly", open_=True, idx=1)
              + f'<div style="padding:10px 14px;display:flex;gap:6px;align-items:center"><span class="lab" style="width:132px">Look up in</span>'
                f'<span class="chip mono">{ic("dots-six-vertical",12)} pl {ic("x",11)}</span><span class="chip add">{ic("plus",12)} add</span></div>')
    sub = card(chead("-R", "Resource", "", icon="file-text", open_=False, summary="config/limit.txt").replace("var(--ns-core)", "var(--ns-res)"), cls="sub-card")
    gt = card(chead("gt", "Greater than", "pl", icon="funnel", open_=True, idx=2)
              + toolbar("menu", True)
              + grid(field("Column", inp("amount", sel=True), req=True),
                     field("Value", f'<span class="inp" style="color:var(--q-link)">{ic("link",14)}<span class="mono">-R/config/limit.txt</span></span>', actions="link", req=True)
                     + f'<div></div><div class="treeline">{sub}</div><div></div>'),
              cls="open focus")
    sort = card(chead("sort", "Sort by column", "pl", icon="sort-ascending", open_=False, idx=3), cls="")
    head = card(chead("head", "Get first rows", "pl", icon="list-numbers", open_=False, idx=4, modified=True), cls="")
    stack = (f'<div class="glabel">Key part</div>{res}<div class="glabel" style="margin-top:8px">Query part</div>{ns}{gt}{sort}{head}'
             f'<div class="addrow"><span class="chip add">{ic("plus",13)} Add action…</span><span class="chip add">{ic("plus",13)} Directive</span><span class="sp"></span>'
             f'<span class="btn ghost sm">{ic("arrows-in-simple",14)} Collapse all</span><span class="btn ghost sm">{ic("arrows-out-simple",14)} Expand all</span></div>')
    win = f"""<div class="win" style="width:1060px">{titlebar("advanced")}{qbar(q)}
<div class="qstatus"><span class="okc">{ic("check-circle",13)} valid</span><span>3 actions · 1 link</span></div>
<div class="adv" style="border-top:1px solid var(--line)"><div class="outline">{outline}</div>
<div class="stack" style="padding-top:12px">{stack}</div></div></div>"""
    body = f"""<h1>Reference design: Advanced mode</h1>
<p class="sub">Every segment editable: the key part (resource) and the query part (namespaces, actions, directives). Tree elements appear: an outline, segment groups, and link arguments nested under their field with a guide line.</p>{win}"""
    page("03-advanced-mode", body, width=1120)


# ================================================================ 04 card anatomy
def anatomy_headers():
    def lab(t, tag=""):
        return f'<div class="cap" style="margin-top:10px;display:flex;gap:8px;align-items:center">{t}{tag}</div>'
    seq = [("gt", "Greater than", "funnel"), ("sort", "Sort by column", "sort-ascending"), ("head", "Get first rows", "list-numbers")]
    def stack(hv):
        r = card(chead("-R", "Resource", "", icon="file-csv", open_=False, hv=hv, summary="sales.csv" if hv != "a" else None).replace("var(--ns-core)", "var(--ns-res)"), cls="")
        for n, l, i in seq:
            r += card(chead(n, l, "pl", icon=i, open_=False, hv=hv), cls="")
        return f'<div class="col" style="gap:6px">{r}</div>'
    a = stack("a"); b = stack("b"); c = stack("c")
    exp = card(chead("gt", "Greater than", "pl", icon="funnel", open_=True, idx=2) + toolbar("menu", True), cls="open")
    exp_r = card(chead("gt", "Greater than", "pl", icon="funnel", open_=True, idx=2, realm="server") + toolbar("menu", True), cls="open")
    body = f"""<h1>Card headers that read as steps</h1>
<p class="sub">Collapsed cards should read as a human sequence ("Resource, Greater than, Sort by column, Get first rows"), not a sequence of ids. The label comes from the command metadata; the namespace/name stays visible but secondary.</p>
<div class="row">
<div style="width:400px">{lab("A · ids first (first draft)", '<span class="tag gray">was</span>')}{a}</div>
<div style="width:400px">{lab("B · icon and label only")}{b}</div>
<div style="width:400px">{lab("C · label, then namespace/name", '<span class="tag g">recommended</span>')}{c}</div></div>
<div class="row" style="margin-top:18px">
<div style="width:610px">{lab("Expanded: namespace/name above, label below")}{exp}</div>
<div style="width:610px">{lab("Expanded, multi-realm: the realm appears only when the registry has more than one")}{exp_r}</div></div>
<p class="sub" style="margin-top:14px">B is the most readable but hides which command runs; C keeps it, dimmed, and a hover on either shows the command doc. Many labels are still ids today (<span class="mono">state</span>, <span class="mono">n</span>, <span class="mono">sigma</span> for arguments); improving labels is part of the work.</p>"""
    page("04a-card-headers", body, width=1300)


def anatomy():
    collapsed = card(chead("sort", "Sort by column", "pl", icon="sort-ascending", open_=False, idx=3), cls="")
    collapsed_sum = card(chead("sort", "", "pl", icon="sort-ascending", open_=False, idx=3, summary="amount · descending"), cls="")
    collapsed_mod = card(chead("gt", "Greater than", "pl", icon="funnel", open_=False, idx=2, modified=True), cls="")
    collapsed_err = card(chead("head", "Get first rows", "pl", icon="list-numbers", open_=False, idx=4, err=True), cls="err")
    expanded = card(chead("sort", "Sort by column", "pl", icon="sort-ascending", open_=True, idx=3)
                    + toolbar("hybrid", True).replace("Top 100", "Region").replace("Top 10", "Amount ↓").replace("Top 20", "Date").replace('chip on', 'chip')
                    + grid(field("Column", inp("amount", sel=True), req=True, hint="Column to sort by"),
                           field("Ascending", f'<span class="seg2"><span>Ascending</span><span class="on">Descending</span></span>', actions="hover"),
                           field("Nulls last", check(False)),
                           ), cls="open focus")
    err_card = card(chead("head", "Get first rows", "pl", icon="list-numbers", open_=True, idx=4, err=True)
                    + grid(field("Number of rows", f'<span class="inp" style="border-color:var(--danger)"><span class="mono">twenty</span></span>')
                           + '<div></div><div class="hint" style="color:var(--danger)">Expected an integer. The query keeps 20 until this is fixed.</div><div></div>'), cls="open err")
    L = lambda n, t, d: f'<div class="it"><span class="badge">{n}</span><div><b>{t}</b><br>{d}</div></div>'
    legend = "".join([
        L(1, "Header", "Chevron, command icon, command name (mono), label (muted), and at the right: modified dot, error mark, position, namespace, ⋯ menu. Collapsed, the header is the whole card."),
        L(2, "Action toolbar", "Shown only when expanded: Reset, Presets, Next, then Move up/down (Advanced only) and Delete at the far right, away from Reset."),
        L(3, "Form grid", "Column 1 labels (right-aligned, argument label from metadata), column 2 editors (full width), a narrow column 3 for per-field actions."),
        L(4, "Per-field actions", "Link ↔ value and reset to default. Faint until the row is hovered or focused; solid when active (a link)."),
        L(5, "Help line", "The argument description under its editor, when it has one. The default is the placeholder text."),
    ])
    body = f"""<h1>Action card anatomy</h1>
<p class="sub">Each action gets its own frame. Collapsed, it is a single header line; expanded, a toolbar and a two-column form follow.</p>
<div class="row"><div class="col" style="width:640px">
<div class="cap">Collapsed (recommended): label first, namespace/name secondary</div>{collapsed}
<div class="cap" style="margin-top:6px">States: modified · error · optional parameter summary (open question)</div>{collapsed_mod}{collapsed_err}{collapsed_sum}
<div class="cap" style="margin-top:6px">Expanded and focused</div>
<div class="rel">{expanded}
<span class="pin" style="left:-10px;top:10px">1</span><span class="pin" style="left:-10px;top:50px">2</span>
<span class="pin" style="left:-10px;top:96px">3</span><span class="pin" style="left:630px;top:96px">4</span><span class="pin" style="left:-10px;top:124px">5</span></div>
<div class="cap" style="margin-top:6px">A refused edit, shown at its field</div>{err_card}
</div><div class="legend" style="width:360px;margin-top:22px">{legend}</div></div>"""
    page("04-card-anatomy", body, width=1100)


# ================================================================ 05 presets
def presets():
    def panel(title, tag, body, pros, cons, w=360):
        p = "".join(f'<div style="display:flex;gap:6px;color:var(--ink2);font-size:12px"><span style="color:var(--ok)">{ic("plus",12)}</span>{x}</div>' for x in pros)
        c = "".join(f'<div style="display:flex;gap:6px;color:var(--ink2);font-size:12px"><span style="color:var(--danger)">{ic("minus",12)}</span>{x}</div>' for x in cons)
        return f'''<div class="panel" style="width:{w}px;display:flex;flex-direction:column;gap:12px">
<div style="display:flex;align-items:center;gap:8px"><b style="font-size:14px">{title}</b><span class="sp"></span>{tag}</div>
<div class="rel" style="height:250px">{body}</div><div class="col" style="gap:5px">{p}{c}</div></div>'''
    hd = chead("head", "Get first rows", "pl", icon="list-numbers", open_=True)
    base = lambda tb: card(hd + tb + grid(field("Number of rows", inp("20", w=True))), cls="open")
    tb_menu = (f'<div class="ctool"><span class="btn ghost sm">{ic("arrow-counter-clockwise",14)} Reset</span><div class="vsep"></div>'
               f'<span class="btn sm" style="border-color:var(--accent)">{ic("lightning",14)} Presets {ic("caret-down",12)}</span>'
               f'<span class="btn ghost sm">{ic("arrow-right",14)} Next {ic("caret-down",12)}</span><div class="sp"></div><span class="btn ghost sm icon danger">{ic("trash",15)}</span></div>')
    menu = (f'<div class="pop" style="left:96px;top:88px;width:250px">'
            f'<div class="mi">Top 10<span class="d">head-10</span><span class="kbd">1</span></div>'
            f'<div class="mi hov">Top 20<span class="d">head-20</span><span class="kbd">2</span></div>'
            f'<div class="mi">Top 100<span class="d">head-100</span><span class="kbd">3</span></div>'
            f'<div class="mi">Preview<span class="d">head-5</span><span class="kbd">4</span></div>'
            f'<div class="msep"></div><div class="mi" style="color:var(--ink3)">{ic("info",13)} Hover shows description</div></div>')
    a = panel("A · Menu", '<span class="tag gray">today</span>', base(tb_menu) + menu,
              ["Compact, scales to any number", "Room for value preview and description"],
              ["Hidden: two clicks, poor discoverability", "Can't show which preset is active"])
    tb_btn = ('<div class="ctool" style="flex-wrap:wrap;row-gap:6px">'
              f'<span class="btn ghost sm">{ic("arrow-counter-clockwise",14)} Reset</span><div class="vsep"></div>'
              '<span class="chip">Top 10</span><span class="chip on">Top 20</span><span class="chip">Top 100</span><span class="chip">Preview</span>'
              '<span class="chip">Sample 1000</span><span class="chip">All but header</span>'
              f'<span class="btn ghost sm">{ic("arrow-right",14)} Next {ic("caret-down",12)}</span><span class="btn ghost sm icon danger">{ic("trash",15)}</span></div>')
    b = panel("B · Buttons", '<span class="tag gray">option</span>', base(tb_btn),
              ["One click, always visible", "Active preset highlighted"],
              ["Wraps and grows the toolbar with many or long labels", "Pushes Delete out of a stable position"])
    tb_h = (f'<div class="ctool"><span class="btn ghost sm">{ic("arrow-counter-clockwise",14)} Reset</span><div class="vsep"></div>'
            '<span class="chip">Top 10</span><span class="chip on">Top 20</span><span class="chip">Top 100</span>'
            f'<span class="btn ghost sm" style="background:var(--ctl)">+3 {ic("caret-down",12)}</span><div class="vsep"></div>'
            f'<span class="btn ghost sm">{ic("arrow-right",14)} Next {ic("caret-down",12)}</span><div class="sp"></div><span class="btn ghost sm icon danger">{ic("trash",15)}</span></div>')
    menu2 = (f'<div class="pop" style="left:230px;top:88px;width:190px">'
             f'<div class="mi">Preview<span class="kbd">4</span></div><div class="mi">Sample 1000<span class="kbd">5</span></div><div class="mi">All but header<span class="kbd">6</span></div></div>')
    c = panel("C · Chips + overflow", '<span class="tag g">recommended</span>', base(tb_h) + menu2,
              ["Common presets one click away, the rest in a menu", "Fixed toolbar height; Delete stays put", "Chip lights up when the fields match a preset"],
              ["Needs toolbar fitting (measure, then split)"], w=450)
    body = f"""<h1>Presets: menu, buttons, or both</h1>
<p class="sub">Presets fill all of an action's parameters at once. <b>Recommendation: C.</b> Show as many as fit in the toolbar (up to 3) as chips, put the rest in a "+N" menu, and number them so <span class="kbd">Alt 1</span>…<span class="kbd">Alt 9</span> apply one. <i>Next</i> stays a menu, because it adds a card instead of changing this one.<br>
Note: no registered command declares presets or next today, so the example presets for <span class="mono">head</span> are invented.</p>
<div class="row">{a}{b}{c}</div>"""
    page("05-presets", body, width=1260)


# ================================================================ 06 jump
def jump():
    text = "-R/data/sales.csv/-/ns-pl/gt-amount-1000/sort-amount-f/head-20"
    caret_at = text.index("1000") + 2
    parts = []
    # build with highlight
    pre = text[:text.index("gt-")]
    parts_html = (qspan([("h", "-R/"), ("k", "data/sales.csv"), ("s", "/-/"), ("n", "ns-pl"), ("s", "/")])
                  + '<span style="background:rgba(45,129,255,.07);border-radius:3px">'
                  + qspan([("a", "gt"), ("s", "-"), ("p", "amount"), ("s", "-")])
                  + '<span class="p hl">10<span class="caret"></span>00</span></span>'
                  + qspan([("s", "/"), ("a", "sort"), ("s", "-"), ("p", "amount"), ("s", "-"), ("p", "f"), ("s", "/"), ("a", "head"), ("s", "-"), ("p", "20")]))
    pop = (f'<div class="pop" style="left:310px;top:108px;min-width:0;padding:5px 9px;display:flex;gap:8px;align-items:center;font-size:12px">'
           f'{tile("pl","funnel",size=18).replace("width=\"15\" height=\"15\"","width=\"11\" height=\"11\"")}<span>Greater than</span><span class="arrow">›</span><b>Value</b>'
           f'<span class="clabel">Rows where column &gt; value</span><span class="kbd">Ctrl J</span></div>')
    gt = card(chead("gt", "Greater than", "pl", icon="funnel", open_=True, idx=2) + toolbar("menu", True)
              + grid(field("Column", inp("amount", sel=True), req=True), field("Value", inp("1000", focus=True), actions="hover", req=True)), cls="open focus")
    win = f"""<div class="win rel" style="width:900px">{titlebar("advanced", pending=False)}{qbar(parts_html, focus=True)}
<div class="qstatus"><span>cursor 38</span><span>Greater than › Value</span></div>
{pop}
<div class="stack" style="padding-top:8px">{card(chead("sort","Sort by column","pl",icon="sort-ascending",open_=False,idx=3),cls="")}</div></div>"""
    win2 = f"""<div class="win" style="width:900px">{titlebar("advanced", pending=False)}{qbar(qspan(Q_PARTS))}
<div class="stack" style="padding-top:8px">{card(chead("ns","Namespaces","",icon="brackets-curly",open_=False,idx=1),cls="")}{gt}{card(chead("sort","Sort by column","pl",icon="sort-ascending",open_=False,idx=3),cls="")}</div></div>"""
    # position table
    rows = []
    import itertools
    toks = [("-R/data/sales.csv", "resource", "Resource › Key ᴬ"),
            ("ns-pl", "ns", "Namespaces card ᴬ"),
            ("gt", "action", "gt header ᴬ"),
            ("amount", "param 0", "gt › Column ᴬ"),
            ("1000", "param 1", "gt › Value ᴬ"),
            ("sort", "action", "sort header ᴬ"),
            ("f", "param 1", "sort › Ascending ᴬ"),
            ("head", "action", "head header"),
            ("20", "param 0", "head › Number of rows")]
    pos = 0
    for t, k, d in toks:
        i = text.index(t, pos)
        pos = i + len(t)
        rows.append(f'<tr style="white-space:nowrap"><td class="mono" style="color:var(--q-par)">{html.escape(t)}</td><td class="mono">{i}–{i+len(t)}</td><td>{k}</td><td>{d}</td></tr>')
    table = f'''<table style="border-collapse:collapse;font-size:12px;color:var(--ink2);width:100%">
<tr style="color:var(--ink3);text-align:left"><th style="padding:4px 8px 4px 0">token</th><th>bytes</th><th>source</th><th>jump target</th></tr>
{"".join(rows).replace("<td","<td style=\"padding:4px 8px 4px 0;border-top:1px solid var(--line)\"")}</table>'''
    body = f"""<h1>Jump from the query text to the matching field</h1>
<p class="sub">Every action and parameter in a parsed query carries its source <span class="mono">Position</span> (<span class="mono">ActionRequest.position</span>, <span class="mono">ActionParameter::String(_, Position)</span>). The cursor offset picks the action and parameter; the editor's argument binding turns the parameter index into a field.</p>
<div class="row"><div class="col"><div class="cap">1 · Cursor in "1000": the action and parameter light up, a hint names the target</div>{win}
<div class="cap" style="margin-top:10px">2 · Ctrl J (or Ctrl-click): gt expands, its Value editor takes focus. Ctrl J again returns, with 1000 selected</div>{win2}</div>
<div class="panel" style="width:420px;margin-top:22px"><div class="cap">Position map of the example</div>{table}<p style="color:var(--ink3);font-size:11.5px;margin:8px 0 0">ᴬ Advanced mode only. Simple mode reaches the last action.</p>
<p style="color:var(--ink3);font-size:11.5px;margin:12px 0 0">Positions are start offsets only; a token ends where the next starts (escapes like <span class="mono">~I</span> make decoded length differ from source length). In Simple mode a target outside the last action shows "Open in Advanced (Ctrl B)" instead of jumping.</p></div></div>"""
    page("06-query-jump", body, width=1360)


# ================================================================ 08 icons
def icons():
    nss = [("pl", "table", "Polars DataFrame"), ("rec", "rows", "Record streams"), ("img", "image", "Images"),
           ("md", "file-text", "Markdown"), ("mem", "brain", "Agent memory"), ("lui", "layout", "UI elements"),
           ("template", "brackets-curly", "Templates"), ("", "terminal", "Root / core")]
    colmap = {"pl": "var(--ns-pl)", "img": "var(--ns-img)", "rec": "#d9893b", "md": "#5fa8d3", "mem": "#d35fa8", "lui": "#7f8cff", "template": "#b9a24a", "": "var(--ns-core)"}
    ns_html = "".join(f'<div style="display:flex;gap:10px;align-items:center"><div class="tile" style="background:{colmap[n]}">{ic(i,15)}</div><span class="mono" style="width:70px">{n or "(root)"}</span><span class="clabel">{d}</span></div>' for n, i, d in nss)
    cmd = [("pl", "funnel", "gt"), ("pl", "sort-ascending", "sort"), ("pl", "list-numbers", "head"), ("img", "arrows-out", "resize"), ("img", "drop", "blur"), ("pl", "export", "to_csv")]
    cmd_html = "".join(f'<div style="display:flex;gap:10px;align-items:center"><div class="tile" style="background:{colmap[n]}">{ic(i,15)}</div><span class="mono" style="width:70px">{c}</span><span class="clabel mono" style="white-space:nowrap">hint icon: "{i}"</span></div>' for n, i, c in cmd)
    mono_html = "".join(f'<div style="display:flex;gap:10px;align-items:center"><div class="tile mono" style="background:{colmap[n]}">{c[0].upper()}</div><span class="mono">{n}/{c}</span></div>' for n, c in [("rec", "window"), ("mem", "overview"), ("", "my_command")])
    body = f"""<h1>Command icons</h1>
<p class="sub"><b>Do actions have icons? Not yet.</b> <span class="mono">CommandMetadata.hints</span> is a free map, and <span class="mono">register_command!</span> accepts <span class="mono">hint icon: "…"</span>, but none of the 192 registered commands sets it. Proposal: a fallback chain, so every card has an icon from day one.</p>
<div class="row">
<div class="panel" style="width:360px"><div class="cap">1 · Command hint (wins)</div><div class="col">{cmd_html}</div></div>
<div class="panel" style="width:330px"><div class="cap">2 · Namespace icon and colour</div><div class="col">{ns_html}</div></div>
<div class="panel" style="width:300px"><div class="cap">3 · Monogram (last resort)</div><div class="col">{mono_html}</div>
<p class="clabel" style="font-size:12px;margin:14px 0 0">First letter on the namespace colour: still scannable, never blank. A hint naming an unknown icon also falls back here.</p></div>
</div>"""
    page("08-icons", body, width=1100)


current(); simple(); advanced(); anatomy_headers(); anatomy(); presets(); jump(); icons()
print("ok")


# ================================================================ 09 phosphor vs lucide
LUC = os.path.join(HERE, "..", "icons", "package", "icons")


def icl(name, size=16):
    svg = open(os.path.join(LUC, f"{name}.svg")).read()
    svg = re.sub(r"<!--.*?-->", "", svg, flags=re.S).strip()
    head, rest = svg.split(">", 1)
    head = re.sub(r'\s(width|height|class)="[^"]*"', "", head)
    svg = head + ">" + rest
    return svg.replace("<svg", f'<svg width="{size}" height="{size}" style="vertical-align:-3px;flex:none;stroke-width:1.75"', 1)


def icon_compare():
    rows = [("Filter (gt, eq, …)", "funnel", "filter", "pl"), ("Sort", "sort-ascending", "arrow-down-wide-narrow", "pl"),
            ("First rows", "list-numbers", "list-ordered", "pl"), ("Table (ns pl)", "table", "table", "pl"),
            ("Records (ns rec)", "rows", "rows-3", "rec"), ("Image (ns img)", "image", "image", "img"),
            ("Resize", "arrows-out", "expand", "img"), ("Blur", "drop", "droplet", "img"),
            ("Markdown", "file-text", "file-text", "md"), ("Memory", "brain", "brain", "mem"),
            ("UI element", "layout", "layout-dashboard", "lui"), ("Template", "brackets-curly", "braces", "template"),
            ("Export", "export", "upload", "pl"), ("Chart", "chart-bar", "chart-column", "pl"),
            ("CSV resource", "file-csv", "file-spreadsheet", "res")]
    toolbar_ic = [("Reset", "arrow-counter-clockwise", "rotate-ccw"), ("Presets", "lightning", "zap"), ("Next", "arrow-right", "arrow-right"),
                  ("Link", "link", "link"), ("Browse", "folder-open", "folder-open"), ("Delete", "trash", "trash-2"), ("Error", "warning-circle", "circle-alert")]
    col = {"pl": "var(--ns-pl)", "img": "var(--ns-img)", "rec": "#d9893b", "md": "#5fa8d3", "mem": "#d35fa8", "lui": "#7f8cff", "template": "#b9a24a", "res": "var(--ns-res)"}
    def t(svg, ns):
        return f'<div class="tile" style="background:{col[ns]}">{svg}</div>'
    tr = "".join(f'''<tr><td style="color:var(--ink2)">{c}</td>
<td>{t(ic(p,15), ns)}</td><td>{ic(p,18)}</td><td class="mono clabel">{p}</td>
<td style="border-left:1px solid var(--line2);padding-left:18px">{t(icl(l,15), ns)}</td><td>{icl(l,18)}</td><td class="mono clabel">{l}</td></tr>''' for c, p, l, ns in rows)
    tb_p = "".join(f'<span class="btn ghost sm" style="color:var(--ink)">{ic(p,15)} {n}</span>' for n, p, l in toolbar_ic)
    tb_l = "".join(f'<span class="btn ghost sm" style="color:var(--ink)">{icl(l,15)} {n}</span>' for n, p, l in toolbar_ic)
    weights = "".join(f'<span style="display:inline-flex;flex-direction:column;align-items:center;gap:4px;margin-right:14px">{open(os.path.join(ICONS, "..", w, f"funnel{"" if w=="regular" else "-"+w}.svg")).read().replace("<svg ", "<svg width=\"22\" height=\"22\" style=\"fill:currentColor\" ",1)}<span class="clabel" style="font-size:11px">{w}</span></span>' for w in ["thin", "light", "regular", "bold", "fill", "duotone"])
    strokes = "".join(f'<span style="display:inline-flex;flex-direction:column;align-items:center;gap:4px;margin-right:14px">{icl("filter",22).replace("stroke-width:1.75", f"stroke-width:{w}")}<span class="clabel" style="font-size:11px">stroke {w}</span></span>' for w in ["1", "1.5", "1.75", "2", "2.5"])
    facts = [("Icons", "1,512", "1,539 (v0.460)"), ("Styles", "6 weights: thin, light, regular, bold, fill, duotone", "1 outline style; stroke width adjustable in SVG, fixed in the font"),
             ("Licence", "MIT", "ISC"), ("egui", "<span class='mono'>egui-phosphor</span> crate: the font plus name constants, one call to add it", "No egui crate known: load <span class='mono'>lucide.ttf</span> into FontDefinitions ourselves and generate name → codepoint constants"),
             ("Browser renderer", "Web font or inline SVG", "Web font or inline SVG"),
             ("Character", "Rounder, slightly heavier; the fill weight gives a selected/active state for free", "Lighter, technical; close to VS Code / Linear look"),
             ("Data-specific glyphs", "file-csv, file-sql, table, rows, columns, chart-*", "sheet, table, rows-3, columns-3, database, chart-*; no file-csv")]
    ft = "".join(f'<tr><td style="color:var(--ink3);white-space:nowrap">{a}</td><td>{b}</td><td>{c}</td></tr>' for a, b, c in facts)
    css = '<style>.ct td{padding:5px 8px;border-top:1px solid var(--line);vertical-align:middle} .ft td{padding:7px 10px;border-top:1px solid var(--line);vertical-align:top;font-size:12.5px;color:var(--ink2)}</style>'
    body = f"""{css}<h1>Icons: Phosphor or Lucide</h1>
<p class="sub">The same concepts in both sets, on namespace tiles (card headers) and plain (toolbars). Both are free, large and available as a font, so either works in egui and in the browser.</p>
<div class="row"><div class="panel" style="width:720px">
<table class="ct" style="border-collapse:collapse;width:100%"><tr class="cap"><td>Concept</td><td colspan="3">Phosphor (regular)</td><td colspan="3" style="border-left:1px solid var(--line2);padding-left:18px">Lucide</td></tr>{tr}</table></div>
<div class="col" style="width:470px;gap:16px">
<div class="panel"><div class="cap">Card toolbar · Phosphor</div><div style="display:flex;flex-wrap:wrap;gap:2px">{tb_p}</div>
<div class="cap" style="margin-top:12px">Card toolbar · Lucide</div><div style="display:flex;flex-wrap:wrap;gap:2px">{tb_l}</div></div>
<div class="panel"><div class="cap">Phosphor weights</div>{weights}<div class="cap" style="margin-top:12px">Lucide stroke widths (SVG only)</div>{strokes}</div>
<div class="panel"><table class="ft" style="border-collapse:collapse;width:100%"><tr class="cap"><td></td><td>Phosphor</td><td>Lucide</td></tr>{ft}</table>
<p class="clabel" style="font-size:12px;margin:10px 0 0"><b style="color:var(--ink)">Leaning Phosphor:</b> the ready egui crate and the fill weight for active states. Lucide if you prefer the lighter, more technical look; it costs a small font-loading module.</p></div>
</div></div>"""
    page("09-icons-phosphor-vs-lucide", body, width=1280)


# ================================================================ 10 compact
COMPACT_CSS = """<style>
.cp{font-size:12.5px}
.cp .titlebar{padding:5px 8px;gap:8px}
.cp .qbar{margin:6px 8px 2px;padding:4px 8px;border-radius:6px}
.cp .qstatus{margin:0 10px 4px;font-size:11px}
.cp .stack{padding:2px 8px 8px;gap:0}
.cp .card{border-radius:0;box-shadow:none;border-width:0 0 1px 0;border-color:var(--line);background:transparent}
.cp .card.open{background:var(--card);border:1px solid var(--line2);border-radius:7px;margin:3px 0}
.cp .card.focus{border-color:rgba(45,129,255,.7);box-shadow:0 0 0 2px var(--wash)}
.cp .chead{height:28px;padding:0 4px 0 2px;gap:6px;background:transparent}
.cp .card.open .chead{background:var(--head);border-radius:7px 7px 0 0}
.cp .tile{width:18px !important;height:18px !important;border-radius:4px}
.cp .tile svg{width:12px;height:12px}
.cp .cname{font-size:12.5px}
.cp .grid{grid-template-columns:104px 1fr 40px;row-gap:4px;column-gap:10px;padding:6px 8px 8px}
.cp .inp{height:23px;border-radius:4px;padding:0 7px}
.cp .lab{font-size:12px}
.cp .fa .b{width:20px;height:20px}
.cp .btn.sm{height:22px}.cp .btn.icon.sm{width:22px}
.cp .chip{height:20px;padding:0 7px;font-size:11.5px}
.cp .steps{margin:2px 8px 6px}
.hico{display:flex;gap:0;color:var(--ink2)}
</style>"""


def compact():
    def hicons(presets=True):
        p = (f'<span class="btn ghost sm icon">{ic("arrow-counter-clockwise",14)}</span>'
             + (f'<span class="btn ghost sm" style="padding:0 5px">{ic("lightning",14)}{ic("caret-down",10)}</span>' if presets else "")
             + f'<span class="btn ghost sm" style="padding:0 5px">{ic("arrow-right",14)}{ic("caret-down",10)}</span>'
             + f'<span class="btn ghost sm icon danger">{ic("trash",14)}</span>')
        return f'<span class="hico">{p}</span>'
    def ch(name, label, ns, icon, open_=False, extra="", idx=None, mod=False):
        chev = ic("caret-down" if open_ else "caret-right", 12, "chev")
        ident = f"{ns}/{name}" if ns else name
        right = ('<span class="dot"></span>' if mod else "") + (hicons() if open_ else f'<span class="btn ghost sm icon">{ic("dots-three",14)}</span>')
        return f'''<div class="chead"><span class="chev">{chev}</span>{tile(ns or "", icon)}<span class="cname">{label}</span>
<span class="mono" style="font-size:11px;color:var(--ink3)">{ident}</span>{extra}<div class="sp"></div>{right}</div>'''
    def tb(mode):
        s_on, a_on = ("on", "") if mode == "simple" else ("", "on")
        return f'''<div class="titlebar"><div class="crumb" style="font-size:12px">{ic("file-text",13)} <b>top_sales</b></div><div class="sp"></div>
<div class="seg"><span class="{s_on}" style="padding:2px 6px">{ic("square",13)}</span><span class="{a_on}" style="padding:2px 6px">{ic("tree-structure",13)}</span></div>
<span class="btn ghost sm icon">{ic("arrow-counter-clockwise",14)}</span><span class="btn pri sm" style="height:22px">{ic("check",13)} Update</span></div>'''
    q = f'<div class="qbar">{ic("magnifying-glass",13)}<div class="q mono" style="font-size:11.5px">{qspan(Q_PARTS)}</div></div>'
    head_open = card(ch("head", "Get first rows", "pl", "list-numbers", True, mod=True)
                     + grid(field("Rows", inp("20", w=True, focus=True) + '<span class="clabel" style="font-size:11px">default 5</span>', actions="hover")), cls="open focus")
    simple_w = f'''<div class="win cp" style="width:470px">{tb("simple")}{q}
<div class="steps"><span class="chip">sales.csv</span><span class="arrow">›</span><span class="chip">Greater than</span><span class="arrow">›</span><span class="chip">Sort by column</span><span class="arrow">›</span><span class="chip cur">Get first rows</span></div>
<div class="stack">{head_open}<div class="addrow" style="padding-top:4px"><span class="chip">to_csv</span><span class="chip">describe</span><span class="chip add">{ic("plus",11)} Add</span></div></div></div>'''
    gt_open = card(ch("gt", "Greater than", "pl", "funnel", True, idx=2)
                   + grid(field("Column", inp("amount", sel=True)), field("Value", inp("1000"), actions="hover")), cls="open focus")
    adv_stack = (card(ch("-R", "Resource", "", "file-csv", extra='<span class="clabel mono" style="font-size:11px">sales.csv</span>').replace("var(--ns-core)", "var(--ns-res)"), cls="")
                 + card(ch("ns", "Namespaces", "", "brackets-curly", extra='<span class="clabel mono" style="font-size:11px">pl</span>'), cls="")
                 + gt_open
                 + card(ch("sort", "Sort by column", "pl", "sort-ascending"), cls="")
                 + card(ch("head", "Get first rows", "pl", "list-numbers", mod=True), cls="")
                 + f'<div class="addrow" style="padding-top:5px"><span class="chip add">{ic("plus",11)} Action</span><span class="chip add">{ic("plus",11)} Directive</span></div>')
    adv_w = f'''<div class="win cp" style="width:470px">{tb("advanced")}{q}<div class="stack">{adv_stack}</div></div>'''
    items = [("Collapsed cards are rows", "One 28 px line each, separated by hairlines; only the open card gets a frame."),
             ("Toolbar moves into the header", "Reset, Presets ▾, Next ▾, Delete as icon buttons (tooltips + shortcuts) on the open card's header."),
             ("Denser form", "23 px controls, 4 px row gap, help text in tooltips instead of a line under each field."),
             ("Slim chrome", "Icon-only mode switch, no file path, Revert as an icon; the query bar is one 24 px line."),
             ("Same model", "Only density tokens change, so it can be a setting (Comfortable / Compact) rather than a second design.")]
    legend = "".join(f'<div class="it"><span class="badge">{i+1}</span><div><b>{t}</b><br>{d}</div></div>' for i, (t, d) in enumerate(items))
    body = f"""{COMPACT_CSS}<h1>Compact variant</h1>
<p class="sub">For side panels and the query console: the same structure at roughly 60% of the height. Comfortable Simple mode is 430 px tall here, compact 200 px.</p>
<div class="row"><div class="col"><div class="cap">Simple, compact</div>{simple_w}</div>
<div class="col"><div class="cap">Advanced, compact</div>{adv_w}</div><div class="legend" style="width:280px;margin-top:24px">{legend}</div></div>"""
    page("10-compact", body, width=1320)


icon_compare(); compact()
print("ok2")

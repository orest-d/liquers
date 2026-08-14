"""Prototype of the parameter-entity-escaping encoder/decoder, to validate the
Phase 2 design's properties before implementation."""
import sys, random

# --- Annex B, tier B-1 (ASCII punctuation) + a few from B-2/B-3 for coverage
CURATED = {
    'excl':'!', 'quot':'"', 'num':'#', 'dollar':'$', 'percnt':'%', 'amp':'&',
    'apos':"'", 'lpar':'(', 'rpar':')', 'ast':'*', 'comma':',', 'colon':':',
    'semi':';', 'lt':'<', 'equals':'=', 'gt':'>', 'quest':'?', 'commat':'@',
    'lsqb':'[', 'bsol':'\\', 'rsqb':']', 'Hat':'^', 'grave':'`', 'lcub':'{',
    'verbar':'|', 'rcub':'}',
    # decode-only synonyms (never emitted)
    'lbrack':'[', 'rbrack':']', 'lbrace':'{', 'rbrace':'}', 'vert':'|', 'midast':'*',
    'sol':'/', 'lowbar':'_', 'plus':'+', 'period':'.',
    # B-2 / B-3 samples
    'nbsp':' ', 'copy':'©', 'deg':'°', 'mdash':'—',
    'hellip':'…', 'euro':'€', 'pi':'π', 'ne':'≠',
}
# The encoder's reverse index: first name wins, so declaration order is the tie-break.
# Synonyms are excluded, as Annex B says they are decode-only.
EMITTED_NAMES = ['excl','quot','num','dollar','percnt','amp','apos','lpar','rpar','ast',
                 'comma','colon','semi','lt','equals','gt','quest','commat','lsqb','bsol',
                 'rsqb','Hat','grave','lcub','verbar','rcub',
                 'nbsp','copy','deg','mdash','hellip','euro','pi','ne']
BY_CHAR = {}
for n in EMITTED_NAMES:
    BY_CHAR.setdefault(CURATED[n], n)

# --- step 1: mnemonics, longest decoded first
MNEMONICS = [('https://','~H'), ('http://','~h'), ('file://','~f'), ('://','~P'),
             ('~','~~'), (' ','~.'), ('/','~/'), ('-','~_')]
# decode-only mnemonic
DECODE_ONLY = [('~I','/')]

def is_literal(c):
    # encoder emits pure ASCII (D6)
    return c.isascii() and (c.isalnum() or c in '_+.')

def encode(s):
    out, i = [], 0
    while i < len(s):
        # step 1: longest mnemonic, plus the contextual ~<digit> form
        if s[i] == '-' and i + 1 < len(s) and s[i+1].isdigit():
            out.append('~' + s[i+1]); i += 2; continue
        hit = None
        for dec, enc in MNEMONICS:
            if s.startswith(dec, i):
                hit = (dec, enc); break
        if hit:
            out.append(hit[1]); i += len(hit[0]); continue
        c = s[i]
        if is_literal(c):                      # step 2
            out.append(c)
        elif c in BY_CHAR:                     # step 3
            out.append('~n%s~' % BY_CHAR[c])
        else:                                  # step 4
            out.append('~U%X~' % ord(c))       # uppercase hex, no leading zeros
        i += 1
    return ''.join(out)

class DecodeError(Exception): pass

NAME_CHARS = set('ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-')
RADIX = {'U':16, 'D':10, 'O':8, 'B':2}

def decode(t):
    out, i = [], 0
    while i < len(t):
        if t[i] != '~':
            out.append(t[i]); i += 1; continue
        if i + 1 >= len(t): raise DecodeError('dangling ~')
        o = t[i+1]
        if o == '~': out.append('~'); i += 2; continue
        if o == '_': out.append('-'); i += 2; continue
        if o == '.': out.append(' '); i += 2; continue
        if o == '/': out.append('/'); i += 2; continue
        if o == 'I': out.append('/'); i += 2; continue
        if o == 'H': out.append('https://'); i += 2; continue
        if o == 'h': out.append('http://'); i += 2; continue
        if o == 'f': out.append('file://'); i += 2; continue
        if o == 'P': out.append('://'); i += 2; continue
        if o.isdigit():
            j = i + 1
            while j < len(t) and t[j].isdigit(): j += 1
            out.append('-' + t[i+1:j]); i = j; continue
        if o in RADIX or o == 'n':
            j = i + 2
            if j < len(t) and t[j] == '~': j += 1        # D13 optional separator
            k = j
            while k < len(t) and t[k] in NAME_CHARS: k += 1
            if k == j: raise DecodeError('empty entity body at %d' % i)
            if k >= len(t) or t[k] != '~': raise DecodeError('unterminated entity at %d' % i)
            body = t[j:k]
            if o == 'n':
                if body not in CURATED: raise DecodeError('unknown named entity %r at %d' % (body, i))
                out.append(CURATED[body])
            else:
                try: cp = int(body, RADIX[o])
                except ValueError: raise DecodeError('bad digit in entity at %d' % i)
                if cp > 0x10FFFF: raise DecodeError('code point out of range at %d' % i)
                if 0xD800 <= cp <= 0xDFFF: raise DecodeError('surrogate at %d' % i)
                out.append(chr(cp))
            i = k + 1; continue
        raise DecodeError('unknown entity opener %r at %d' % (o, i))
    return ''.join(out)

# ---------------------------------------------------------------- properties
def gen_corpus():
    c = []
    c += [chr(i) for i in range(0x20, 0x7f)]                  # all printable ASCII
    c += ['é','Ł',' ','°','π','…','≠','€']
    c += ['\U0001F600','\U0010FFFF',chr(0),'\t','\n']       # astral, min, controls
    c += ['12:30','a,b','café','日本','-5','-42','a-4','~X~','~E','~~',
          'https://api.example.com/data','http://x','file:///tmp/a','://','a b c',
          '','~','~n','~U41~','~namp~','a~I b', 'é́']
    rnd = random.Random(20260814)
    pool = [chr(i) for i in range(0x20,0x7f)] + ['~','/','-',' ','é','π','\U0001F600',':']
    for _ in range(4000):
        c.append(''.join(rnd.choice(pool) for _ in range(rnd.randint(0,12))))
    return c

fails = []
for s in gen_corpus():
    e = encode(s)
    if not e.isascii(): fails.append(('ascii', s, e))
    try:
        d = decode(e)
    except DecodeError as ex:
        fails.append(('decode-error', s, e, str(ex))); continue
    if d != s: fails.append(('roundtrip', s, e, d))
    if encode(d) != e: fails.append(('idempotent', s, e, encode(d)))

print("corpus size:", len(gen_corpus()))
print("failures:", len(fails))
for f in fails[:10]: print("  ", f)

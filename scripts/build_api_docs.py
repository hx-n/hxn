#!/usr/bin/env python3
"""Generate the API reference under site/api/ from the released haxelibs.

Each library in LIBRARIES is installed at its version into a haxelib repository
of its own (target/api-haxelib by default), `haxe -xml` describes its types,
and every documented type gets a page in the site's layout. The sitemap and
llms.txt are updated to list the pages.

    python3 scripts/build_api_docs.py [--repo DIR] [--offline]

--offline uses the repository as it is, without installing.
"""
from html import escape
from pathlib import Path
import argparse
import json
import re
import shutil
import subprocess
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / 'site'
API = SITE / 'api'
ORIGIN = 'https://hxn.dev'

# name: the haxelib. prefix: the package documented. roots: what haxe is given
# to type. hide: type paths left out, as regular expressions. defines: for a
# binding without a HashLink library for this machine, a platform it has one for.
LIBRARIES = [
    dict(name='hxn', version='0.3.5', prefix='hxn.', roots=['--macro', "include('hxn', true, ['hxn.native'])"],
         hide=[r'^hxn\.(Wire|Native|Bind|Handle|TupleMacro)$', r'^hxn\.native\.', r'^hxn\.reactive\.'],
         summary='Futures, streams, signals, reactive values, atomics and callbacks: the Haxe classes every hxn library uses.'),
    dict(name='hxn-window', version='0.2.0', prefix='hxn.window.', roots=['--macro', "include('hxn.window')"],
         summary='Windows, input, monitors, gamepads and the clipboard over winit, with native handles for GPU surfaces.'),
    dict(name='hxn-av', version='0.2.0', prefix='hxn.av.', roots=['--macro', "include('hxn.av')"],
         summary='Audio and video with the platform\'s codecs: playback, frames, PCM audio, an equalizer, AAC, H.264 and MP4.'),
    dict(name='hxnsl', version='0.2.0', prefix='hxnsl.', roots=['hxnsl.Shader', 'hxnsl.Layout'],
         summary='Shaders written in HXSL and translated at compile time to WGSL, MSL, HLSL, SPIR-V and GLSL ES.'),
    dict(name='hxn-gpu', version='0.2.0', prefix='hxn.gpu.', roots=['--macro', "include('hxn.gpu', true, ['hxn.gpu.backend'])"],
         hide=[r'^hxn\.gpu\.(backend|metal|vulkan|d3d12|gles|webgpu|webgl2)\.', r'^hxn\.gpu\.Setup$'],
         summary='One GPU API over every hxn GPU driver, with features and limits per device and a default backend per platform.'),
    dict(name='hxn-gpu-metal', version='0.2.0', prefix='hxn.gpu.metal.', roots=['--macro', "include('hxn.gpu.metal')"],
         summary='Metal on macOS and iOS: compute and render passes, with hxnsl shaders compiled into Metal libraries.'),
    dict(name='hxn-gpu-vulkan', version='0.3.0', prefix='hxn.gpu.vulkan.', roots=['--macro', "include('hxn.gpu.vulkan')"],
         summary='Vulkan on Linux, Windows, Android and macOS: compute, rendering, explicit barriers and swapchains.'),
    dict(name='hxn-gpu-d3d12', version='0.2.0', prefix='hxn.gpu.d3d12.', roots=['--macro', "include('hxn.gpu.d3d12')"], defines=['hxn_triple=x86_64-pc-windows-msvc'],
         summary='Direct3D 12 on Windows: compute, rendering, textures and swapchains, with transitions or enhanced barriers.'),
    dict(name='hxn-gpu-gles', version='0.3.0', prefix='hxn.gpu.gles.', roots=['--macro', "include('hxn.gpu.gles')"],
         summary='OpenGL ES 3.0 over EGL on Android, Linux, macOS and Windows: rendering, textures and windows.'),
    dict(name='hxn-gpu-webgpu', version='0.3.1', prefix='hxn.gpu.webgpu.', roots=['--macro', "include('hxn.gpu.webgpu')"], defines=['wasm', 'hxn_threads'],
         summary='WebGPU in the browser, on hxcpp and Ash compiled to wasm: compute, rendering and the web page\'s canvas.'),
    dict(name='hxn-gpu-webgl2', version='0.3.0', prefix='hxn.gpu.webgl2.', roots=['--macro', "include('hxn.gpu.webgl2')"], defines=['wasm', 'hxn_threads'],
         summary='WebGL2 in the browser, on hxcpp and Ash compiled to wasm, for browsers without WebGPU.'),
]

# Generated helpers every binding has, which programs do not use.
HIDDEN = [r'Wire$', r'\.Hxn\w+(Native|Future|Inbox)$', r'_Impl_$']

KINDS = [('class', 'Classes'), ('interface', 'Interfaces'), ('enum', 'Enums'), ('abstract', 'Abstracts'), ('typedef', 'Typedefs')]


def run(args, cwd):
    result = subprocess.run(args, cwd=cwd, capture_output=True, text=True)
    if result.returncode != 0:
        raise SystemExit(f'{" ".join(args)} failed:\n{result.stdout}{result.stderr}')
    return result.stdout


def describe(repo, lib, offline):
    """The library's types, as haxe -xml writes them."""
    spec = f'{lib["name"]}:{lib["version"]}'
    if not offline:
        run(['haxelib', 'install', lib['name'], lib['version'], '--always', '--quiet'], repo)
    out = repo / f'{lib["name"]}.xml'
    defines = [a for d in lib.get('defines', []) for a in ('-D', d)]
    run(['haxe', '-lib', spec, *lib['roots'], *defines, '-xml', str(out), '-hl', str(repo / 'out.hl'), '--no-output'], repo)
    return ET.parse(out).getroot()


# Model ----------------------------------------------------------------------

def meta_names(node):
    meta = node.find('meta')
    return {m.get('n') for m in meta.findall('m')} if meta is not None else set()


def doc_of(node):
    d = node.find('haxe_doc')
    return d.text if d is not None and d.text else ''


def hidden(path, lib):
    patterns = HIDDEN + lib.get('hide', [])
    return any(re.search(p, path) for p in patterns)


def collect(root, lib):
    types = {}
    for node in root:
        path = node.get('path', '')
        if not path.startswith(lib['prefix']) or node.get('private') == '1' or hidden(path, lib):
            continue
        names = meta_names(node)
        if ':noCompletion' in names or ':dox' in names and 'hide' in ET.tostring(node.find('meta')).decode():
            continue
        kind = node.tag
        if kind == 'class' and node.get('interface') == '1':
            kind = 'interface'
        types[path] = dict(path=path, kind=kind, node=node, lib=lib['name'],
                           name=path.rsplit('.', 1)[-1], package=path.rsplit('.', 1)[0])
    return types


# Types as text ----------------------------------------------------------------

class Linker:
    def __init__(self, everything):
        self.everything = everything
        self.short = {}
        self.local = {}

    def find(self, name):
        """The type a doc comment names: a full path, or a short name in this library or one only."""
        return self.everything.get(name) or self.local.get(name) or self.short.get(name)

    def name(self, path):
        short = path.rsplit('.', 1)[-1]
        target = self.everything.get(path)
        if target:
            return f'<a href="{type_url(target)}">{escape(short)}</a>'
        return escape(short)

    def type(self, node):
        if node is None:
            return 'Void'
        tag = node.tag
        if tag in ('x', 'c', 'e', 't'):
            path = node.get('path')
            params = [self.type(p) for p in node]
            if path == 'Null' and len(params) == 1:
                return f'Null&lt;{params[0]}&gt;'
            text = self.name(path)
            return text + (f'&lt;{", ".join(params)}&gt;' if params else '')
        if tag == 'f':
            children = list(node)
            names = node.get('a', '').split(':') if node.get('a') else []
            args = [self.type(c) for c in children[:-1]]
            parts = []
            for i, arg in enumerate(args):
                label = names[i] if i < len(names) else ''
                parts.append(f'{escape(label)}:{arg}' if label else arg)
            ret = self.type(children[-1]) if children else 'Void'
            return f'({", ".join(parts)}) -&gt; {ret}'
        if tag == 'a':
            fields = [f'{escape(f.tag)}:{self.type(first_type(f))}' for f in node]
            return '{ ' + ', '.join(fields) + ' }'
        if tag == 'd':
            return 'Dynamic'
        return escape(tag)


def first_type(node):
    for child in node:
        if child.tag not in ('meta', 'haxe_doc', 'overloads'):
            return child
    return None


def signature(field, linker):
    """A field's declaration: `function name(a:Int):Void` or `var name:Int`."""
    t = first_type(field)
    # An abstract's instance method takes the value as a first argument named this.
    skip = 1 if t is not None and t.tag == 'f' and t.get('a', '').split(':')[0] == 'this' else 0
    name = escape(field.tag if field.tag != 'new' else 'new')
    if field.get('set') == 'method' and t is not None and t.tag == 'f':
        children = list(t)
        names = t.get('a', '').split(':') if t.get('a') else []
        defaults = t.get('v', '').split(':') if t.get('v') else []
        args = []
        for i, c in enumerate(children[:-1]):
            if i < skip:
                continue
            label = names[i] if i < len(names) else f'arg{i}'
            text = f'{escape(label)}:{linker.type(c)}'
            if i < len(defaults) and defaults[i]:
                text += f' = {escape(defaults[i])}'
            args.append(text)
        ret = linker.type(children[-1]) if children else 'Void'
        tail = '' if field.tag == 'new' else f':{ret}'
        return f'function {name}({", ".join(args)}){tail}'
    access = ''
    get, set_ = field.get('get'), field.get('set')
    if field.get('final') == '1':
        keyword = 'final'
    else:
        keyword = 'var'
        if get or set_:
            g = {'accessor': 'get', 'null': 'null', 'inline': 'default'}.get(get, 'default')
            s = {'accessor': 'set', 'null': 'null', 'method': 'never', 'dynamic': 'dynamic'}.get(set_, 'default')
            if (g, s) != ('default', 'default'):
                access = f'({g}, {s})'
    return f'{keyword} {name}{access}:{linker.type(t)}'


# Docs -------------------------------------------------------------------------

def dedent(text):
    lines = text.replace('\t', '    ').split('\n')
    lines = [re.sub(r'^\s*\* ?', '', l) if re.match(r'^\s*\*(\s|$)', l) else l for l in lines]
    while lines and not lines[0].strip():
        lines.pop(0)
    while lines and not lines[-1].strip():
        lines.pop()
    if not lines:
        return ''
    # The first line follows the comment's opening, so it sets no indentation.
    rest = [l for l in lines[1:] if l.strip()]
    indent = min((len(l) - len(l.lstrip()) for l in rest), default=0)
    return '\n'.join([lines[0].strip()] + [l[indent:] for l in lines[1:]])


def inline(text, linker):
    out = []
    for i, part in enumerate(re.split(r'(`[^`]+`)', text)):
        if i % 2:
            code = part[1:-1]
            target = linker.find(code)
            owner, _, field = re.sub(r'\(\)$', '', code).rpartition('.')
            if target:
                out.append(f'<a href="{type_url(target)}"><code>{escape(code)}</code></a>')
            elif owner and linker.find(owner) and re.fullmatch(r'\w+', field):
                out.append(f'<a href="{type_url(linker.find(owner))}#{escape(field)}"><code>{escape(code)}</code></a>')
            else:
                out.append(f'<code>{escape(code)}</code>')
            continue
        part = escape(part, quote=False)
        part = re.sub(r'\[([^\]]+)\]\((https?://[^)\s]+)\)', r'<a href="\2">\1</a>', part)
        part = re.sub(r'\*\*([^*]+)\*\*', r'<strong>\1</strong>', part)
        part = re.sub(r'(?<![\w*])\*([^*\s][^*]*)\*(?!\w)', r'<em>\1</em>', part)
        out.append(part)
    return ''.join(out)


def markdown(text, linker):
    """Doc comments' markdown: paragraphs, lists, code blocks and inline code."""
    text = dedent(text)
    if not text:
        return ''
    blocks, html = [], []
    lines = text.split('\n')
    i = 0
    while i < len(lines):
        line = lines[i]
        if line.strip().startswith('```'):
            code = []
            i += 1
            while i < len(lines) and not lines[i].strip().startswith('```'):
                code.append(lines[i])
                i += 1
            i += 1
            html.append(f'<div class="code-panel"><pre tabindex="0"><code>{escape(chr(10).join(code))}</code></pre></div>')
            continue
        if re.match(r'^\s*([-*]|\d+\.)\s+', line):
            ordered = bool(re.match(r'^\s*\d+\.', line))
            items = []
            while i < len(lines) and lines[i].strip():
                if re.match(r'^\s*([-*]|\d+\.)\s+', lines[i]):
                    items.append(re.sub(r'^\s*([-*]|\d+\.)\s+', '', lines[i]))
                else:
                    items[-1] += ' ' + lines[i].strip()
                i += 1
            tag = 'ol' if ordered else 'ul'
            html.append(f'<{tag}>' + ''.join(f'<li>{inline(item, linker)}</li>' for item in items) + f'</{tag}>')
            continue
        if not line.strip():
            i += 1
            continue
        para = []
        while i < len(lines) and lines[i].strip() and not lines[i].strip().startswith('```') \
                and not re.match(r'^\s*([-*]|\d+\.)\s+', lines[i]):
            para.append(lines[i].strip())
            i += 1
        joined = ' '.join(para)
        if joined.startswith('@'):
            joined = re.sub(r'^@(param|return|throws)\s*', lambda m: {'param': 'Parameter: ', 'return': 'Returns: ', 'throws': 'Throws: '}[m.group(1)], joined)
        html.append(f'<p>{inline(joined, linker)}</p>')
    return ''.join(html)


def first_sentence(text):
    text = ' '.join(dedent(text).split())
    if not text:
        return ''
    match = re.match(r'(.+?[.!?])(\s|$)', text)
    return match.group(1) if match else text


def plain(text):
    return re.sub(r'`([^`]+)`', r'\1', first_sentence(text))


def meta_description(text, context):
    """A search result's snippet: the doc's first sentence, with context when it is short, under 160 characters."""
    text = plain(text)
    if len(text) < 70:
        text = f'{text} {context}'.strip()
    if len(text) > 160:
        text = text[:157].rsplit(' ', 1)[0].rstrip(',;:') + '…'
    return text


def breadcrumb(parts):
    """The heading's eyebrow as links up the reference: API / library / package."""
    links = [f'<a href="{href}">{escape(name)}</a>' if href else escape(name) for name, href in parts]
    return f'<nav class="eyebrow api-breadcrumb" aria-label="Breadcrumb">{" / ".join(links)}</nav>'


# Pages ------------------------------------------------------------------------

def lib_url(name):
    return f'/api/{name}/'


def type_url(t):
    return f'/api/{t["lib"]}/{t["path"]}/'


def page(url, title, description, crumbs, body, schema_type='TechArticle', extra=None):
    canonical = ORIGIN + url
    full_title = f'{title} — hxn'
    graph = [{'@type': schema_type, '@id': canonical + '#page', 'url': canonical, 'name': full_title,
              'headline': title, 'description': description, 'inLanguage': 'en',
              'isPartOf': {'@id': ORIGIN + '/#website'}, 'image': ORIGIN + '/og-hero.png', **(extra or {})},
             {'@type': 'BreadcrumbList', 'itemListElement': [
                 {'@type': 'ListItem', 'position': i + 1, 'name': name, 'item': ORIGIN + href}
                 for i, (name, href) in enumerate([('hxn', '/')] + crumbs)]}]
    schema = json.dumps({'@context': 'https://schema.org', '@graph': graph}).replace('</', '<\\/')
    t, d = escape(full_title), escape(description)
    return f'''<!doctype html>
<html lang="en" prefix="og: https://ogp.me/ns#">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="color-scheme" content="dark">
  <meta name="theme-color" content="#000000">
  <title>{t}</title>
  <meta name="description" content="{d}">
  <meta name="robots" content="index, follow, max-image-preview:large">
  <link rel="canonical" href="{canonical}">
  <link rel="describedby" href="/llms.txt" type="text/markdown">
  <meta property="og:type" content="article">
  <meta property="og:site_name" content="hxn">
  <meta property="og:locale" content="en_GB">
  <meta property="og:title" content="{t}">
  <meta property="og:description" content="{d}">
  <meta property="og:url" content="{canonical}">
  <meta property="og:image" content="{ORIGIN}/og-hero.png">
  <meta property="og:image:type" content="image/png">
  <meta property="og:image:width" content="2400">
  <meta property="og:image:height" content="1260">
  <meta property="og:image:alt" content="hxn — Native Rust bindings for Haxe">
  <meta name="twitter:card" content="summary_large_image">
  <meta name="twitter:title" content="{t}">
  <meta name="twitter:description" content="{d}">
  <meta name="twitter:image" content="{ORIGIN}/og-hero.png">
  <meta name="twitter:image:alt" content="hxn — Native Rust bindings for Haxe">
  <link rel="icon" href="/logo.png" type="image/png">
  <link rel="apple-touch-icon" href="/logo.png">
  <link rel="stylesheet" href="/styles.css">
  <script src="/script.js" defer></script>
  <script type="application/ld+json">{schema}</script>
</head>
<body>
  <a class="skip-link" href="#content">Skip to content</a>
  <header class="header">
    <a class="wordmark" href="/" aria-label="hxn home">hxn<span aria-hidden="true"> /</span></a>
    <nav class="header-links" aria-label="Main"><a href="/tutorials/">Tutorials</a><a href="/api/" aria-current="{'page' if url == '/api/' else 'true'}">API</a><a class="source-link" href="https://github.com/hx-n/hxn">GitHub ↗</a></nav>
  </header>
  <main class="tutorial-layout api-layout">
{body}
  </main>
  <footer><p>HashLink · Ash · hxcpp</p><a href="https://github.com/hx-n/hxn/blob/main/LICENSE">Open source · Apache 2.0</a></footer>
</body>
</html>
'''


def library_sidebar(lib, types, current=None):
    links = [f'<a class="tutorial-index-link" href="/api/">API reference</a>']
    here = ' aria-current="page"' if current is None else ''
    links.append(f'<a class="api-sidebar-library" href="{lib_url(lib["name"])}"{here}>{escape(lib["name"])} <span>{escape(lib["version"])}</span></a>')
    for t in sorted(types.values(), key=lambda t: t['name'].lower()):
        mark = ' aria-current="page"' if current == t['path'] else ''
        links.append(f'<a href="{type_url(t)}"{mark}>{escape(t["name"])}</a>')
    return f'    <aside class="tutorial-sidebar api-sidebar"><nav aria-label="{escape(lib["name"])} types">{"".join(links)}</nav></aside>'


def type_list(types, linker):
    rows = []
    for kind, title in KINDS:
        of_kind = sorted((t for t in types if t['kind'] == kind), key=lambda t: t['name'].lower())
        if not of_kind:
            continue
        items = ''.join(
            f'<li data-api-entry><a href="{type_url(t)}"><code>{escape(t["name"])}</code></a>'
            f'<span>{inline(first_sentence(doc_of(t["node"])), linker)}</span></li>' for t in of_kind)
        rows.append(f'<section class="lesson-section" id="{kind}"><h2>{title}</h2><ul class="api-list">{items}</ul></section>')
    return ''.join(rows)


def member(field, linker, anchor_prefix=''):
    anchor = anchor_prefix + field.tag
    doc = markdown(doc_of(field), linker)
    return (f'<div class="api-member" id="{escape(anchor)}"><pre class="api-signature"><code>{signature(field, linker)}</code></pre>'
            f'{doc}</div>')


def row(anchor, sig, doc, linker):
    """An enum constructor or value: its declaration beside its doc."""
    return (f'<li id="{escape(anchor)}"><code>{sig}</code>'
            f'<div>{markdown(doc, linker)}</div></li>')


def compact(rows):
    return f'<ul class="api-list api-values">{"".join(rows)}</ul>' if rows else ''


def fields_of(node):
    return [c for c in node if c.tag not in ('meta', 'haxe_doc', 'extends', 'implements', 'overloads', 'impl', 'this', 'from', 'to', 'a')]


def type_body(t, linker):
    node, kind = t['node'], t['kind']
    sections, toc = [], []

    def section(key, title, html):
        if html:
            toc.append(f'<a href="#{key}">{title}</a>')
            sections.append(f'<section class="lesson-section" id="{key}"><h2>{title}</h2>{html}</section>')

    facts = []
    params = node.get('params')
    if params:
        facts.append(f'Type parameters: <code>{escape(params.replace(":", ", "))}</code>')
    for tag, label in (('extends', 'Extends'), ('implements', 'Implements')):
        for e in node.findall(tag):
            facts.append(f'{label} <code>{linker.name(e.get("path"))}</code>')
    if kind == 'abstract':
        this = node.find('this')
        if this is not None and first_type(this) is not None:
            facts.append(f'Over <code>{linker.type(first_type(this))}</code>')
    if kind == 'typedef':
        target = first_type(node)
        if target is not None and target.tag != 'a':
            facts.append(f'Alias of <code>{linker.type(target)}</code>')

    if kind in ('class', 'interface'):
        fields = [f for f in fields_of(node) if f.get('public') == '1' or kind == 'interface']
        ctor = [f for f in fields if f.tag == 'new']
        statics = [f for f in fields if f.get('static') == '1' and f.tag != 'new']
        members = [f for f in fields if f.get('static') != '1' and f.tag != 'new']
        section('constructor', 'Constructor', ''.join(member(f, linker) for f in ctor))
        section('static-fields', 'Static fields', ''.join(member(f, linker) for f in statics if f.get('set') != 'method'))
        section('static-methods', 'Static methods', ''.join(member(f, linker) for f in statics if f.get('set') == 'method'))
        section('properties', 'Properties', ''.join(member(f, linker) for f in members if f.get('set') != 'method'))
        section('methods', 'Methods', ''.join(member(f, linker) for f in members if f.get('set') == 'method'))
    elif kind == 'enum':
        items = []
        for c in fields_of(node):
            args = [x for x in c if x.tag not in ('meta', 'haxe_doc')]
            names = c.get('a', '').split(':') if c.get('a') else []
            sig = escape(c.tag)
            if names:
                sig += '(' + ', '.join(f'{escape(n)}:{linker.type(a)}' for n, a in zip(names, args)) + ')'
            items.append(row(c.tag, sig, doc_of(c), linker))
        section('constructors', 'Constructors', compact(items))
    elif kind == 'abstract':
        impl = node.find('impl/class')
        values, methods = [], []
        if impl is not None:
            for f in fields_of(impl):
                if f.get('public') != '1':
                    continue
                names = meta_names(f)
                if ':enum' in names and ':value' in names:
                    value = f.find("meta/m[@n=':value']/e")
                    v = f' = {escape(re.sub(r"^cast ", "", value.text))}' if value is not None and value.text else ''
                    values.append(row(f.tag, escape(f.tag) + v, doc_of(f), linker))
                elif f.tag != '_new':
                    methods.append(member(f, linker))
        section('values', 'Values', compact(values))
        section('methods', 'Fields and methods', ''.join(methods))
    elif kind == 'typedef':
        target = first_type(node)
        if target is not None and target.tag == 'a':
            section('fields', 'Fields', ''.join(member(f, linker) for f in target))

    facts_html = f'<ul class="api-facts">{"".join(f"<li>{f}</li>" for f in facts)}</ul>' if facts else ''
    toc_html = f'<nav class="lesson-toc" aria-label="On this page">{"".join(toc)}</nav>' if len(toc) > 1 else ''
    return facts_html, toc_html, ''.join(sections)


def write(url, html):
    path = SITE / url.lstrip('/') / 'index.html'
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(html)


def build(repo, offline):
    if not offline:
        repo.mkdir(parents=True, exist_ok=True)
        if not (repo / '.haxelib').exists():
            run(['haxelib', 'newrepo'], repo)
    per_lib, everything = {}, {}
    for lib in LIBRARIES:
        types = collect(describe(repo, lib, offline), lib)
        per_lib[lib['name']] = types
        everything.update(types)
    linker = Linker(everything)
    # A short name in a doc comment links when only one documented type has it.
    counts = {}
    for t in everything.values():
        counts.setdefault(t['name'], []).append(t)
    linker.short = {name: ts[0] for name, ts in counts.items() if len(ts) == 1}

    if API.exists():
        shutil.rmtree(API)
    urls = ['/api/']
    for lib in LIBRARIES:
        types = per_lib[lib['name']]
        linker.local = {t['name']: t for t in types.values()}
        url = lib_url(lib['name'])
        urls.append(url)
        repo_url = f'https://github.com/hx-n/{lib["name"]}'
        heading = (f'<header class="lesson-heading">{breadcrumb([("API", "/api/"), (lib["name"] + " " + lib["version"], None)])}'
                   f'<h1>{escape(lib["name"])}</h1><p class="lesson-description">{escape(lib["summary"])}</p>'
                   f'<p class="api-links"><code>haxelib install {escape(lib["name"])} {escape(lib["version"])}</code>'
                   f'<a href="{repo_url}">Source ↗</a><a href="https://lib.haxe.org/p/{escape(lib["name"])}/">haxelib ↗</a></p></header>')
        body = (library_sidebar(lib, types) + '\n    <article id="content" class="lesson">' + heading
                + type_list(types.values(), linker) + '</article>')
        items = [{'@type': 'ListItem', 'position': i + 1, 'name': t['path'], 'url': ORIGIN + type_url(t)}
                 for i, t in enumerate(sorted(types.values(), key=lambda t: t['name'].lower()))]
        write(url, page(url, f'{lib["name"]} {lib["version"]} API reference for Haxe',
                        meta_description(lib['summary'], ''), [('API', '/api/'), (lib['name'], url)], body, 'CollectionPage',
                        {'mainEntity': {'@type': 'ItemList', 'numberOfItems': len(items), 'itemListElement': items}}))
        for t in types.values():
            turl = type_url(t)
            urls.append(turl)
            facts, toc, sections = type_body(t, linker)
            doc = markdown(doc_of(t['node']), linker)
            kind = {'typedef': 'TYPEDEF', 'abstract': 'ABSTRACT', 'enum': 'ENUM', 'interface': 'INTERFACE', 'class': 'CLASS'}[t['kind']]
            heading = (f'<header class="lesson-heading">{breadcrumb([("API", "/api/"), (lib["name"], lib_url(lib["name"])), (kind.lower() + " in " + t["package"], None)])}'
                       f'<h1>{escape(t["name"])}</h1>{facts}</header>')
            article = f'<div class="api-doc">{doc}</div>' if doc else ''
            body = (library_sidebar(lib, types, t['path']) + '\n    <article id="content" class="lesson">'
                    + heading + toc + article + sections + '</article>')
            description = meta_description(doc_of(t['node']), f'{t["name"]} is a Haxe {t["kind"]} in {t["package"]}, from the {lib["name"]} {lib["version"]} haxelib.')
            write(turl, page(turl, f'{t["name"]} ({t["package"]}) · {lib["name"]} API', description,
                             [('API', '/api/'), (lib['name'], lib_url(lib['name'])), (t['name'], turl)], body, 'APIReference',
                             {'programmingLanguage': 'Haxe', 'executableLibraryName': lib['name'], 'assemblyVersion': lib['version'],
                              'targetPlatform': 'HashLink, Ash, hxcpp', 'about': t['path']}))

    linker.local = {}
    cards = ''.join(
        f'<a class="tutorial-card" href="{lib_url(lib["name"])}"><span class="eyebrow">{escape(lib["version"])} / {len(per_lib[lib["name"]])} types</span>'
        f'<h2>{escape(lib["name"])}<span aria-hidden="true"> →</span></h2><p>{escape(lib["summary"])}</p></a>' for lib in LIBRARIES)
    entries = ''.join(
        f'<li data-api-entry><a href="{type_url(t)}"><code>{escape(t["name"])}</code></a><span>{escape(t["lib"])} · {escape(t["package"])}</span></li>'
        for t in sorted(everything.values(), key=lambda t: (t['name'].lower(), t['lib'])))
    sidebar = '    <aside class="tutorial-sidebar api-sidebar"><nav aria-label="Libraries"><a class="tutorial-index-link" href="/api/" aria-current="page">API reference</a>' + ''.join(
        f'<a href="{lib_url(lib["name"])}">{escape(lib["name"])} <span>{escape(lib["version"])}</span></a>' for lib in LIBRARIES) + '</nav></aside>'
    description = 'The Haxe API of every hxn library at its released version: hxn, hxn-window, hxn-av, hxnsl, hxn-gpu and the hxn GPU drivers.'
    body = (sidebar + '\n    <article id="content" class="lesson"><header class="lesson-heading"><p class="eyebrow">API</p>'
            f'<h1>API reference</h1><p class="lesson-description">{escape(description)}</p></header>'
            f'<div class="tutorial-cards">{cards}</div>'
            '<section class="lesson-section" id="types"><h2>Every type</h2>'
            '<label class="api-filter"><span class="sr-only">Filter types</span><input type="search" placeholder="Filter types by name, library or package" data-api-filter autocomplete="off"></label>'
            f'<ul class="api-list api-all">{entries}</ul></section></article>')
    libs = [{'@type': 'ListItem', 'position': i + 1, 'name': lib['name'], 'url': ORIGIN + lib_url(lib['name'])} for i, lib in enumerate(LIBRARIES)]
    write('/api/', page('/api/', 'Haxe API reference for the hxn libraries', description, [('API', '/api/')], body, 'CollectionPage',
                        {'mainEntity': {'@type': 'ItemList', 'numberOfItems': len(libs), 'itemListElement': libs}}))
    update_sitemap(urls)
    update_llms()
    print(f'{len(urls)} pages, {len(everything)} types')


def update_sitemap(urls):
    path = SITE / 'sitemap.xml'
    lines = [l for l in path.read_text().splitlines() if f'<loc>{ORIGIN}/api/' not in l]
    end = lines.index('</urlset>')
    lines[end:end] = [f'  <url><loc>{ORIGIN}{u}</loc></url>' for u in urls]
    path.write_text('\n'.join(lines) + '\n')


def update_llms():
    path = SITE / 'llms.txt'
    text = path.read_text()
    section = '## API reference\n\n' + f'- [API reference]({ORIGIN}/api/): Every type of every hxn library, at its released version.\n' + ''.join(
        f'- [{lib["name"]} {lib["version"]}]({ORIGIN}{lib_url(lib["name"])}): {lib["summary"]}\n' for lib in LIBRARIES)
    text = re.sub(r'## API reference\n\n(?:- .*\n)*\n?', '', text)
    text = text.replace('## Docs\n', section + '\n## Docs\n', 1)
    path.write_text(text)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    parser.add_argument('--repo', type=Path, default=ROOT / 'target' / 'api-haxelib')
    parser.add_argument('--offline', action='store_true')
    options = parser.parse_args()
    build(options.repo.resolve(), options.offline)

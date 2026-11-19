#!/usr/bin/env python3
"""Check static page navigation, metadata and examples against repository sources."""
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit
import hashlib
import json
import re
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
SITE = ROOT / 'site'

# Hosted installers must be the same bytes as the versions covered by CI.
for installer in ('install.sh', 'install.ps1'):
    assert (SITE / installer).read_bytes() == (ROOT / installer).read_bytes(), \
        f'site: {installer} differs from the tested root installer'


class Page(HTMLParser):
    def __init__(self, path):
        super().__init__(convert_charrefs=True)
        self.path = path
        self.ids = set()
        self.links = []
        self.meta = {}
        self.canonical = None
        self.describedby = None
        self.h1_count = 0
        self.snippets = []
        self.copy_targets = []
        self.in_code = None
        self.schema = None
        self.schemas = []
        self.feed(path.read_text())

    def handle_starttag(self, tag, attributes):
        attrs = dict(attributes)
        if 'id' in attrs:
            assert attrs['id'] not in self.ids, f'{self.path}: duplicate id {attrs["id"]}'
            self.ids.add(attrs['id'])
        if tag == 'h1':
            self.h1_count += 1
        if tag == 'meta':
            self.meta[attrs.get('name', attrs.get('property'))] = attrs.get('content')
        if tag == 'link' and attrs.get('rel') == 'canonical':
            self.canonical = attrs['href']
        if tag == 'link' and attrs.get('rel') == 'describedby':
            self.describedby = attrs['href']
        if 'data-copy-target' in attrs:
            self.copy_targets.append(attrs['data-copy-target'])
        if tag in ('a', 'link', 'img', 'script'):
            target = attrs.get('href', attrs.get('src'))
            if target:
                self.links.append(target)
        if tag == 'code' and 'data-source' in attrs:
            self.in_code = [attrs['data-source'], '']
        if tag == 'script' and attrs.get('type') == 'application/ld+json':
            self.schema = ''

    def handle_data(self, data):
        if self.in_code is not None:
            self.in_code[1] += data
        if self.schema is not None:
            self.schema += data

    def handle_endtag(self, tag):
        if tag == 'code' and self.in_code is not None:
            self.snippets.append(self.in_code)
            self.in_code = None
        if tag == 'script' and self.schema is not None:
            self.schemas.append(json.loads(self.schema))
            self.schema = None


def normalized(text):
    return '\n'.join(line.strip() for line in text.strip().splitlines())


pages = {path: Page(path) for path in SITE.rglob('*.html')}
urls = set()
for path, page in pages.items():
    relative = path.relative_to(SITE).as_posix()
    expected = 'https://hxn.dev/' + (relative[:-10] if relative.endswith('index.html') else relative)
    assert page.canonical == expected, f'{path}: canonical URL should be {expected}'
    assert page.canonical not in urls, f'{path}: duplicate canonical URL'
    urls.add(page.canonical)
    assert page.h1_count == 1, f'{path}: expected one h1'
    assert page.describedby == '/llms.txt', f'{path}: no describedby link to /llms.txt'
    assert page.schemas, f'{path}: missing structured data'
    assert page.meta.get('description'), f'{path}: missing description'
    assert page.meta.get('og:url') == page.canonical, f'{path}: Open Graph URL mismatch'
    for key in ('og:title', 'og:description', 'og:image', 'og:image:alt', 'twitter:card', 'twitter:title', 'twitter:description', 'twitter:image'):
        assert page.meta.get(key), f'{path}: missing {key}'
    for target in page.copy_targets:
        assert target in page.ids, f'{path}: missing copy target {target}'
    for target in page.links:
        url = urlsplit(target)
        if url.scheme or url.netloc:
            continue
        destination = (SITE / unquote(url.path).lstrip('/')) if url.path.startswith('/') else path.parent / unquote(url.path)
        if not url.path:
            destination = path
        if destination.is_dir():
            destination /= 'index.html'
        destination = destination.resolve()
        assert destination.is_relative_to(SITE), f'{path}: link escapes site: {target}'
        assert destination.exists(), f'{path}: broken link {target}'
        if url.fragment:
            assert destination in pages and unquote(url.fragment) in pages[destination].ids, f'{path}: missing anchor {target}'
    for source, snippet in page.snippets:
        file = (ROOT / source).resolve()
        assert file.is_relative_to(ROOT) and file.is_file(), f'{path}: invalid source {source}'
        assert normalized(snippet) in normalized(file.read_text()), f'{path}: example drifted from {source}'

sitemap = ET.parse(SITE / 'sitemap.xml')
listed = {node.text for node in sitemap.findall('.//{http://www.sitemaps.org/schemas/sitemap/0.9}loc')}
assert listed == urls, f'Sitemap mismatch: {listed ^ urls}'

def local(url):
    """The file under site/ that an https://hxn.dev/ URL serves."""
    path = SITE / unquote(urlsplit(url).path).lstrip('/')
    return path / 'index.html' if path.is_dir() else path


# llms.txt (llmstxt.org): an H1, a blockquote summary, then H2 sections of links.
llms = (SITE / 'llms.txt').read_text()
lines = llms.split('\n')
assert lines[0].startswith('# ') and lines[2].startswith('> '), 'llms.txt: no H1 followed by a blockquote summary'
assert not re.search(r'^#{3,} ', llms, re.M), 'llms.txt: a heading below H2'
listed_links = re.findall(r'^- \[[^\]]+\]\((\S+?)\)', llms, re.M)
for url in listed_links:
    if url.startswith('https://hxn.dev/'):
        assert local(url).is_file(), f'llms.txt: {url} is not in the site'

# The Agent Skills discovery index, v0.2.0, which scripts/sync_site_skills.py writes.
index = json.loads((SITE / '.well-known' / 'agent-skills' / 'index.json').read_text())
assert index['$schema'] == 'https://schemas.agentskills.io/discovery/0.2.0/schema.json', 'skills index: unknown $schema'
for skill in index['skills']:
    name = skill['name']
    assert re.fullmatch(r'[a-z0-9]+(-[a-z0-9]+)*', name) and len(name) <= 64, f'skills index: invalid name {name}'
    assert skill['type'] == 'skill-md', f'skills index: {name} is not a skill-md'
    data = local('https://hxn.dev' + skill['url']).read_bytes()
    assert skill['digest'] == 'sha256:' + hashlib.sha256(data).hexdigest(), f'skills index: {name} digest mismatch'
    head = data.decode().split('\n---', 1)[0]
    assert f'\nname: {name}\n' in head, f'skills index: {name} SKILL.md names another skill'
    assert f'\ndescription: {skill["description"]}' in head, f'skills index: {name} description differs from its SKILL.md'
    assert len(skill['description']) <= 1024, f'skills index: {name} description is too long'
    for link in re.findall(r'\[[^\]]*\]\(([^)]+)\)', data.decode()):
        assert urlsplit(link).scheme in ('https', 'http', 'mailto'), f'skills index: {name} has a relative documentation link: {link}'
    assert 'https://hxn.dev' + skill['url'] in listed_links, f'llms.txt: {name} skill is not listed'
assert (SITE / '.well-known/agent-skills/hxn/SKILL.md').read_bytes() == (ROOT / 'skills/hxn/SKILL.md').read_bytes(), \
    'site: the hxn skill differs from skills/hxn/SKILL.md; run scripts/sync_site_skills.py'
print(f'PASS: {len(index["skills"])} agent skills and llms.txt.')
print(f'PASS: {len(pages)} pages, local links, SEO, copy targets and {sum(len(p.snippets) for p in pages.values())} source excerpts.')

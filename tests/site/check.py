#!/usr/bin/env python3
"""Check static page navigation, metadata and examples against repository sources."""
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit
import json
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
SITE = ROOT / 'site'


class Page(HTMLParser):
    def __init__(self, path):
        super().__init__(convert_charrefs=True)
        self.path = path
        self.ids = set()
        self.links = []
        self.meta = {}
        self.canonical = None
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
print(f'PASS: {len(pages)} pages, local links, SEO, copy targets and {sum(len(p.snippets) for p in pages.values())} source excerpts.')

#!/usr/bin/env python3
"""Copy the hx-n agent skills into site/.well-known/agent-skills/ and write its index.json.

Each skill is read from a checkout beside this one (../hxn-window/skills/hxn-window/SKILL.md),
or from the repository's main branch on GitHub with --github. The index follows the Agent
Skills discovery format, v0.2.0: https://github.com/cloudflare/agent-skills-discovery-rfc
"""
from pathlib import Path
import hashlib
import json
import sys
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'site' / '.well-known' / 'agent-skills'
SCHEMA = 'https://schemas.agentskills.io/discovery/0.2.0/schema.json'
# (repository, skill name), in the order the index lists them.
SKILLS = [('hxn', 'hxn'), ('hxn-window', 'hxn-window'), ('hxn-av', 'hxn-av'), ('hxnsl', 'hxnsl'), ('hxn-gpu-metal', 'hxn-gpu-metal'), ('hxn-gpu-vulkan', 'hxn-gpu-vulkan'), ('hxn-gpu-d3d12', 'hxn-gpu-d3d12')]


def frontmatter(text):
    """The `name` and `description` of a SKILL.md, from its YAML frontmatter."""
    lines = text.split('\n')
    assert lines[0] == '---', 'SKILL.md does not start with frontmatter'
    fields = {}
    for line in lines[1:lines.index('---', 1)]:
        key, _, value = line.partition(':')
        fields[key.strip()] = value.strip()
    return fields


def read(repo, name, github):
    path = f'skills/{name}/SKILL.md'
    if repo == 'hxn':
        return (ROOT / path).read_bytes()
    if github:
        with urllib.request.urlopen(f'https://raw.githubusercontent.com/hx-n/{repo}/main/{path}') as response:
            return response.read()
    return (ROOT.parent / repo / path).read_bytes()


def main():
    github = '--github' in sys.argv[1:]
    entries = []
    for repo, name in SKILLS:
        data = read(repo, name, github)
        fields = frontmatter(data.decode())
        assert fields.get('name') == name, f'{repo}: the skill is named {fields.get("name")}, not {name}'
        target = OUT / name / 'SKILL.md'
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        entries.append({
            'name': name,
            'type': 'skill-md',
            'description': fields['description'],
            'url': f'/.well-known/agent-skills/{name}/SKILL.md',
            'digest': 'sha256:' + hashlib.sha256(data).hexdigest(),
        })
    (OUT / 'index.json').write_text(json.dumps({'$schema': SCHEMA, 'skills': entries}, indent=2) + '\n')
    print(f'{len(entries)} skills written to {OUT.relative_to(ROOT)}')


if __name__ == '__main__':
    main()

# Comments, commits and issues

[Design](../design.md) · [Conventions](../conventions.md)

- A comment describes what the code does, and why it is shaped that way when
  that is not obvious. No history of what was tried, and no measurements.
- A commit message is short: what changed, and a few sentences of why when
  that is not self-evident.
- Record reproducers, measurements, considered alternatives and open
  questions in `git-bug` issues. Cite the issue's full id in the commit's
  `git-bug:` trailer.
- Use `git-bug` for plans, backlog items, status and history. Keep the
  documentation focused on the current design and conventions.
- An issue has a kind (`bug`, `perf`, `debt`) and an area
  (`area:abi`, `area:hl`, `area:cpp`, `area:future`, `area:reactive`,
  `area:wasm`, `area:gen`, `area:bind`, `area:cli`, `area:docs`).

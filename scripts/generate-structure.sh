#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/structure"

if [ -f "$HOME/.cargo/env" ]; then
  # shellcheck source=/dev/null
  . "$HOME/.cargo/env"
fi

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'missing required command: %s\n' "$1" >&2
    exit 127
  fi
}

list_source_files() {
  if command -v rg >/dev/null 2>&1; then
    rg --files src benchmark
  else
    find src benchmark -type f | sort
  fi
}

write_source_tree() {
  if command -v tree >/dev/null 2>&1; then
    {
      printf 'Cargo.toml\n\n'
      tree -a -L 4 src benchmark
    }
  else
    {
      printf 'Cargo.toml\n\n'
      find src benchmark | sort
    }
  fi
}

require_cmd cargo
require_cmd dot
require_cmd jq
require_cmd perl

cd "$ROOT_DIR"

mkdir -p "$OUT_DIR"

rm -f \
  "$OUT_DIR/modules-traits.dot" \
  "$OUT_DIR/modules-traits.svg" \
  "$OUT_DIR/modules-traits-types.dot" \
  "$OUT_DIR/modules-traits-types.svg" \
  "$OUT_DIR/modules-traits-types-functions.dot" \
  "$OUT_DIR/modules-traits-types-functions.svg" \
  "$OUT_DIR/modules-traits-tree.txt" \
  "$OUT_DIR/source-tree.txt" \
  "$OUT_DIR/source-line-counts.txt" \
  "$OUT_DIR/targets.tsv" \
  "$OUT_DIR/targets.txt" \
  "$OUT_DIR/index.html"

generate_variant() {
  local name="$1"
  shift

  cargo modules dependencies \
    --lib \
    --no-externs \
    --no-uses \
    --layout dot \
    "$@" \
    > "$OUT_DIR/$name.dot" \
    2>/dev/null

  dot \
    -Gnewrank=true \
    -Gnodesep=0.55 \
    -Granksep=1.0 \
    -Goutputorder=edgesfirst \
    -Tsvg \
    "$OUT_DIR/$name.dot" \
    > "$OUT_DIR/$name.svg"
}

generate_variant modules-traits --no-fns --no-types
generate_variant modules-traits-types --no-fns
generate_variant modules-traits-types-functions

cargo modules structure \
  --lib \
  --no-fns \
  --no-types \
  2>/dev/null \
  | perl -pe 's/\e\[[0-9;]*m//g' \
  > "$OUT_DIR/modules-traits-tree.txt"

cargo metadata --no-deps --format-version 1 \
  | jq -r '.packages[0].targets[] | ([.name] + .kind) | @tsv' \
  > "$OUT_DIR/targets.tsv"

awk -F '\t' '{ printf "%-32s %s\n", $1, $2 }' "$OUT_DIR/targets.tsv" > "$OUT_DIR/targets.txt"

write_source_tree > "$OUT_DIR/source-tree.txt"

{
  printf 'lines  path\n'
  printf -- '-----  ----\n'
  {
    printf 'Cargo.toml\n'
    list_source_files
  } | while IFS= read -r file; do
    wc -l "$file"
  done | sort -nr
} > "$OUT_DIR/source-line-counts.txt"

cat > "$OUT_DIR/index.html" <<'EOF'
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Rubik Structure Variants</title>
  <style>
    :root {
      color-scheme: light;
      --bg: #f4f0e8;
      --panel: #fffdf8;
      --ink: #18222f;
      --muted: #5f6b75;
      --line: #d6c9b5;
      --accent: #1c6d5a;
      --accent-soft: #dbeee8;
    }

    * {
      box-sizing: border-box;
    }

    body {
      margin: 0;
      padding: 32px 24px 64px;
      background:
        radial-gradient(circle at top left, rgba(28, 109, 90, 0.12), transparent 30%),
        linear-gradient(180deg, #f8f4ec 0%, var(--bg) 100%);
      color: var(--ink);
      font: 16px/1.5 "Iosevka Web", "Iosevka", "JetBrains Mono", monospace;
    }

    main {
      max-width: 1600px;
      margin: 0 auto;
    }

    h1,
    h2 {
      margin: 0 0 12px;
      font-weight: 700;
      letter-spacing: 0.02em;
    }

    h1 {
      font-size: 32px;
    }

    h2 {
      font-size: 20px;
    }

    p {
      margin: 0 0 14px;
      color: var(--muted);
    }

    code {
      color: var(--ink);
      background: var(--accent-soft);
      padding: 0 0.25rem;
      border-radius: 4px;
    }

    a {
      color: var(--accent);
      text-decoration-thickness: 2px;
      text-underline-offset: 0.15em;
    }

    .hero {
      margin-bottom: 28px;
      padding: 24px;
      border: 1px solid var(--line);
      border-radius: 18px;
      background: rgba(255, 253, 248, 0.92);
      box-shadow: 0 14px 40px rgba(24, 34, 47, 0.08);
    }

    .grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));
      gap: 20px;
      margin-bottom: 20px;
    }

    .panel {
      padding: 20px;
      border: 1px solid var(--line);
      border-radius: 16px;
      background: var(--panel);
      box-shadow: 0 10px 24px rgba(24, 34, 47, 0.05);
    }

    .panel.full {
      margin-top: 20px;
    }

    .links {
      display: flex;
      flex-wrap: wrap;
      gap: 10px 18px;
      margin-top: 10px;
    }

    .links a {
      display: inline-block;
      padding: 8px 12px;
      border: 1px solid var(--line);
      border-radius: 999px;
      background: #fff;
      text-decoration: none;
    }

    iframe,
    img {
      width: 100%;
      border: 1px solid var(--line);
      border-radius: 12px;
      background: #fff;
    }

    iframe {
      min-height: 220px;
    }

    iframe.tall {
      min-height: 460px;
    }

    img {
      margin-top: 12px;
      padding: 12px;
    }

    .note {
      font-size: 14px;
    }

    @media (max-width: 720px) {
      body {
        padding: 20px 14px 40px;
      }

      .hero,
      .panel {
        padding: 16px;
      }
    }
  </style>
</head>
<body>
  <main>
    <section class="hero">
      <h1>Rubik Structure Variants</h1>
      <p>
        Generated from <code>cargo-modules</code> and Graphviz for the <code>rubik</code> package.
        These graphs focus on the library crate and compare ownership-only structure at different detail levels.
      </p>
      <div class="links">
        <a href="modules-traits.svg">Open modules-traits SVG</a>
        <a href="modules-traits-types.svg">Open modules-traits-types SVG</a>
        <a href="modules-traits-types-functions.svg">Open modules-traits-types-functions SVG</a>
        <a href="modules-traits-tree.txt">Open Module Tree</a>
        <a href="source-tree.txt">Open Source Tree</a>
        <a href="source-line-counts.txt">Open Line Counts</a>
        <a href="targets.txt">Open Targets</a>
      </div>
    </section>

    <section class="grid">
      <article class="panel">
        <h2>Targets</h2>
        <p>Package targets discovered from <code>cargo metadata</code>.</p>
        <iframe src="targets.txt" title="Cargo targets"></iframe>
      </article>

      <article class="panel">
        <h2>Module Tree</h2>
        <p>Library ownership tree with functions and types filtered out.</p>
        <iframe src="modules-traits-tree.txt" title="Library module tree"></iframe>
      </article>
    </section>

    <section class="grid">
      <article class="panel">
        <h2>Source Tree</h2>
        <p>Filesystem view of <code>src/</code> and <code>benchmark/</code>.</p>
        <iframe class="tall" src="source-tree.txt" title="Source tree"></iframe>
      </article>

      <article class="panel">
        <h2>Largest Files</h2>
        <p>Source and benchmark files sorted by line count.</p>
        <iframe class="tall" src="source-line-counts.txt" title="Source line counts"></iframe>
      </article>
    </section>

    <section class="panel full">
      <h2>modules-traits.svg</h2>
      <p>
        Ownership-only view with modules and traits. Generated with <code>--no-fns --no-types</code>.
      </p>
      <p class="note">
        This is the cleanest high-level map of the crate. Open the standalone SVG above if you want browser zoom.
      </p>
      <img src="modules-traits.svg" alt="Modules and traits ownership graph">
    </section>

    <section class="panel full">
      <h2>modules-traits-types.svg</h2>
      <p>
        Ownership-only view with modules, traits, and types. Generated with <code>--no-fns</code>.
      </p>
      <img src="modules-traits-types.svg" alt="Modules, traits, and types ownership graph">
    </section>

    <section class="panel full">
      <h2>modules-traits-types-functions.svg</h2>
      <p>
        Full ownership-only view with modules, traits, types, and functions.
      </p>
      <img src="modules-traits-types-functions.svg" alt="Full ownership graph">
    </section>
  </main>
</body>
</html>
EOF

printf 'Generated in %s\n' "$OUT_DIR"

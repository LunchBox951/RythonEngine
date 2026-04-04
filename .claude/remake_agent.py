#!/usr/bin/env python3
"""Regenerates AGENT.md in the project root with a current source map."""

import fnmatch
import os
import tempfile
from pathlib import Path


def load_gitignore_patterns(root: Path) -> list[tuple[str, bool]]:
    """Parse .gitignore and return (pattern, dir_only) tuples.

    Handles comment lines, blank lines, dir-only patterns (trailing /),
    and root-anchored patterns (leading /). Negation patterns are skipped.
    """
    gitignore = root / ".gitignore"
    patterns: list[tuple[str, bool]] = []
    if not gitignore.is_file():
        return patterns
    for line in gitignore.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#") or line.startswith("!"):
            continue
        dir_only = line.endswith("/")
        pattern = line.strip("/")
        patterns.append((pattern, dir_only))
    return patterns


def is_ignored(entry: Path, patterns: list[tuple[str, bool]]) -> bool:
    if entry.name == ".git":
        return True
    is_dir = entry.is_dir()
    for pattern, dir_only in patterns:
        if dir_only and not is_dir:
            continue
        if fnmatch.fnmatch(entry.name, pattern):
            return True
    return False


def build_tree(root: Path, patterns: list[tuple[str, bool]], prefix: str = "") -> list[str]:
    lines = []
    entries = sorted(root.iterdir(), key=lambda p: (p.is_file(), p.name.lower()))

    visible = [e for e in entries if not is_ignored(e, patterns)]

    for i, entry in enumerate(visible):
        is_last = i == len(visible) - 1
        connector = "└── " if is_last else "├── "
        extension = "    " if is_last else "│   "

        if entry.is_dir():
            lines.append(f"{prefix}{connector}{entry.name}/")
            lines.extend(build_tree(entry, patterns, prefix + extension))
        else:
            lines.append(f"{prefix}{connector}{entry.name}")

    return lines


def main():
    project_root = Path(__file__).parent.parent
    patterns = load_gitignore_patterns(project_root)

    lines = build_tree(project_root, patterns)
    tree_content = "\n".join(lines)

    agent_md = f"# Source Map\n\n```\n{project_root.name}/\n{tree_content}\n```\n"

    agent_md_path = project_root / "AGENT.md"
    with tempfile.NamedTemporaryFile(mode="w", dir=agent_md_path.parent, delete=False) as tmp:
        tmp.write(agent_md)
        tmp_path = tmp.name
    os.replace(tmp_path, agent_md_path)
    print(f"Written to {agent_md_path}")


if __name__ == "__main__":
    main()

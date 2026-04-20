"""Shared helpers used by both `scripts/bundle.py` (pre-build artifact generator)
and `scripts/package.py` (release packager).

Kept separate so the two scripts agree on what the stdlib distribution looks
like — any drift would break the sealed-build hash verification.
"""

# Stdlib directories excluded from the distribution — IDEs, test suites, and
# build tools that a shipped game does not need.
STDLIB_EXCLUDES = frozenset({
    "test",
    "tests",
    "idlelib",
    "tkinter",
    "turtledemo",
    "ensurepip",
    "__pycache__",
    "site-packages",
    "lib2to3",
})

# File suffix processed by CPython's `site` module at startup. A `.pth` file
# anywhere on `sys.path` can inject arbitrary directories — so no sealed
# release distribution may contain one, and `bundle.py` aborts if it finds any.
PTH_SUFFIX = ".pth"
